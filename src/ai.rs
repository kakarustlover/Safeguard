use crate::client::TelegramClient;
use crate::db;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio::time;

pub struct StreamResult {
    pub full_text: String,
    pub model_used: String,
}

pub struct HistoryItem {
    pub role: String,
    pub content: String,
}

pub const RICH_MIN_CHARS: usize = 100;

// ================= Gemini =================

pub mod gemini {
    use super::*;

    /// ✅ **اصلاح‌شده**: `system_instruction` را به عنوان یک پارامتر سطح بالا برمی‌گرداند.
    fn build_interactions_input(
        history: &[HistoryItem],
        system_prompt: Option<&str>,
    ) -> (Vec<Value>, Option<String>) {
        let mut steps: Vec<Value> = Vec::new();
        for item in history {
            let step_type = if item.role == "assistant" { "model_output" } else { "user_input" };
            steps.push(json!({
                "type": step_type,
                "content": [{ "type": "text", "text": item.content }]
            }));
        }

        let sys = system_prompt
            .filter(|sp| !sp.trim().is_empty())
            .map(|sp| sp.to_string());

        (steps, sys)
    }

    pub fn is_context_full_error(err: &str) -> bool {
        let lower = err.to_lowercase();
        lower.contains("context")
            || lower.contains("token")
            || lower.contains("too long")
            || lower.contains("maximum")
            || lower.contains("413")
            || lower.contains("400")
    }

    async fn try_stream_request(
        http: &reqwest::Client,
        tg: &TelegramClient,
        gemini_api_key: &str,
        model: &str,
        system_prompt: Option<&str>,
        history: &[HistoryItem],
        chat_id: i64,
        placeholder_message_id: i64,
    ) -> Result<String, String> {
        let url = "https://generativelanguage.googleapis.com/v1beta/interactions";
        let (input, sys_instruction) = build_interactions_input(history, system_prompt);

        let mut body = json!({
            "model": model,
            "input": input,
            "stream": true,
            "store": false
        });
        if let Some(sys) = sys_instruction {
            body["system_instruction"] = json!(sys);
        }

        let resp = http
            .post(url)
            .header("x-goog-api-key", gemini_api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("خطا در اتصال به Gemini: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Gemini خطا داد ({}): {}", status, text));
        }

        let mut byte_stream = resp.bytes_stream();
        let mut full_text = String::new();
        let mut buffer = String::new();
        let mut last_edit = Instant::now();
        let min_edit_interval = Duration::from_millis(700);

        while let Some(chunk_result) = byte_stream.next().await {
            let chunk = chunk_result.map_err(|e| format!("خطا در خواندن stream: {}", e))?;
            let chunk_str = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_str);

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer.drain(..=newline_pos);
                if line.is_empty() {
                    continue;
                }
                let data_part = match line.strip_prefix("data: ") {
                    Some(d) => d,
                    None => continue,
                };
                if data_part == "[DONE]" {
                    continue;
                }
                let parsed: Value = match serde_json::from_str(data_part) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let event_type = parsed["event_type"].as_str().unwrap_or("");
                match event_type {
                    "step.delta" => {
                        let delta_type = parsed["delta"]["type"].as_str().unwrap_or("");
                        if delta_type == "text" {
                            if let Some(text_piece) = parsed["delta"]["text"].as_str() {
                                full_text.push_str(text_piece);
                                if last_edit.elapsed() >= min_edit_interval && !full_text.is_empty() {
                                    let _ = tg.edit_message(chat_id, placeholder_message_id, &full_text).await;
                                    last_edit = Instant::now();
                                }
                            }
                        }
                    }
                    "error" => {
                        let msg = parsed["error"]["message"].as_str().unwrap_or("خطای نامشخص از Gemini");
                        let code = parsed["error"]["code"].as_str().unwrap_or("");
                        return Err(format!("Gemini خطا داد ({}): {}", code, msg));
                    }
                    _ => {}
                }
            }
        }

        if full_text.is_empty() {
            full_text = "متاسفانه پاسخی دریافت نشد.".to_string();
        }
        let _ = tg.edit_message(chat_id, placeholder_message_id, &full_text).await;
        Ok(full_text)
    }

    pub const MODEL: &str = "gemini-3.5-flash-lite";

    pub async fn stream(
        http: &reqwest::Client,
        tg: &TelegramClient,
        gemini_api_key: &str,
        model: &str,
        system_prompt: Option<&str>,
        history: &[HistoryItem],
        chat_id: i64,
        placeholder_message_id: i64,
    ) -> Result<StreamResult, String> {
        let fallback_models = vec![model.to_string(), MODEL.to_string(), "gemini-3.5-flash".to_string()];
        let mut last_error = String::new();

        for current_model in fallback_models.iter() {
            match try_stream_request(
                http, tg, gemini_api_key, current_model, system_prompt, history, chat_id, placeholder_message_id,
            )
            .await
            {
                Ok(full_text) => {
                    return Ok(StreamResult { full_text, model_used: current_model.clone() });
                }
                Err(e) => {
                    if e.contains("503") || e.contains("UNAVAILABLE") || e.contains("high demand")
                        || e.contains("unavailable") || e.contains("gateway_timeout")
                    {
                        last_error = e;
                        time::sleep(Duration::from_secs(2)).await;
                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Err(format!("همه مدل‌های جایگزین در دسترس نیستند. آخرین خطا: {}", last_error))
    }

    pub async fn stream_fixed(
        http: &reqwest::Client,
        tg: &TelegramClient,
        gemini_api_key: &str,
        model: &str,
        system_prompt: Option<&str>,
        history: &[HistoryItem],
        chat_id: i64,
        placeholder_message_id: i64,
    ) -> Result<StreamResult, String> {
        let full_text = try_stream_request(
            http, tg, gemini_api_key, model, system_prompt, history, chat_id, placeholder_message_id,
        )
        .await?;
        Ok(StreamResult { full_text, model_used: model.to_string() })
    }

    /// ✅ **اصلاح‌شده**: پاسخ را از فیلد `steps` (نه `output`) می‌خواند.
    pub async fn complete_once(
        http: &reqwest::Client,
        gemini_api_key: &str,
        system_prompt: Option<&str>,
        user_text: &str,
    ) -> Result<String, String> {
        let url = "https://generativelanguage.googleapis.com/v1beta/interactions";
        let (input, sys_instruction) = build_interactions_input(
            &[HistoryItem { role: "user".into(), content: user_text.into() }],
            system_prompt,
        );

        let mut body = json!({
            "model": MODEL,
            "input": input,
            "stream": false,
            "store": false
        });
        if let Some(sys) = sys_instruction {
            body["system_instruction"] = json!(sys);
        }

        let resp = http
            .post(url)
            .header("x-goog-api-key", gemini_api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("خطا در اتصال به Gemini: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Gemini خطا داد ({}): {}", status, text));
        }

        let v: Value = resp.json().await.map_err(|e| format!("خطا در پارس پاسخ Gemini: {}", e))?;

        // ✅ **اصلاح‌شده**: استفاده از "steps" به جای "output"
        let mut result = String::new();
        if let Some(steps) = v["steps"].as_array() {
            for step in steps {
                if step["type"] == "model_output" {
                    if let Some(content) = step["content"].as_array() {
                        for c in content {
                            if let Some(t) = c["text"].as_str() {
                                result.push_str(t);
                            }
                        }
                    }
                }
            }
        }
        if result.is_empty() {
            return Err("پاسخ خالی از Gemini".to_string());
        }
        Ok(result)
    }
}

// ================= Groq (GPT-OSS 120B) =================

pub mod groq {
    use super::*;

    pub const MODEL: &str = "openai/gpt-oss-120b";
    const URL: &str = "https://api.groq.com/openai/v1/chat/completions";

    pub async fn stream(
        http: &reqwest::Client,
        tg: &TelegramClient,
        groq_api_key: &str,
        system_prompt: Option<&str>,
        history: &[(String, String)],
        chat_id: i64,
        placeholder_message_id: i64,
    ) -> Result<StreamResult, String> {
        let mut messages: Vec<Value> = Vec::new();
        if let Some(sp) = system_prompt {
            if !sp.trim().is_empty() {
                messages.push(json!({ "role": "system", "content": sp }));
            }
        }
        for (role, content) in history {
            let openai_role = if role == "model" || role == "assistant" { "assistant" } else { "user" };
            messages.push(json!({ "role": openai_role, "content": content }));
        }

        let body = json!({ "model": MODEL, "messages": messages, "stream": true });

        let resp = http
            .post(URL)
            .header("Authorization", format!("Bearer {}", groq_api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("خطا در اتصال به Groq: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Groq ({}): {}", status, text));
        }

        let mut byte_stream = resp.bytes_stream();
        let mut full_text = String::new();
        let mut buffer = String::new();
        let mut last_edit = Instant::now();
        let min_edit_interval = Duration::from_millis(700);

        while let Some(chunk_result) = byte_stream.next().await {
            let chunk = chunk_result.map_err(|e| format!("خطا در stream: {}", e))?;
            let chunk_str = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_str);

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer.drain(..=newline_pos);
                if line.is_empty() {
                    continue;
                }
                let data_part = match line.strip_prefix("data: ") {
                    Some(d) => d,
                    None => continue,
                };
                if data_part == "[DONE]" {
                    continue;
                }
                if let Ok(parsed) = serde_json::from_str::<Value>(data_part) {
                    if let Some(delta_text) = parsed["choices"][0]["delta"]["content"].as_str() {
                        full_text.push_str(delta_text);
                        if last_edit.elapsed() >= min_edit_interval && !full_text.is_empty() {
                            let _ = tg.edit_message(chat_id, placeholder_message_id, &full_text).await;
                            last_edit = Instant::now();
                        }
                    }
                }
            }
        }

        if full_text.is_empty() {
            full_text = "متاسفانه پاسخی دریافت نشد.".to_string();
        }
        let _ = tg.edit_message(chat_id, placeholder_message_id, &full_text).await;

        Ok(StreamResult { full_text, model_used: MODEL.to_string() })
    }

    pub fn is_context_full_error(err: &str) -> bool {
        gemini::is_context_full_error(err)
    }
}

// ================= DeepSeek (Orca Router) =================

pub mod deepseek {
    use super::*;

    pub const MODEL: &str = "deepseek/deepseek-v4-flash-free";
    const ORCA_URL: &str = "https://api.orcarouter.ai/v1/chat/completions";

    pub async fn stream(
        http: &reqwest::Client,
        tg: &TelegramClient,
        orca_api_key: &str,
        system_prompt: Option<&str>,
        history: &[(String, String)],
        chat_id: i64,
        placeholder_message_id: i64,
    ) -> Result<StreamResult, String> {
        let mut messages: Vec<Value> = Vec::new();
        if let Some(sp) = system_prompt {
            if !sp.trim().is_empty() {
                messages.push(json!({ "role": "system", "content": sp }));
            }
        }
        for (role, content) in history {
            let openai_role = if role == "model" || role == "assistant" { "assistant" } else { "user" };
            messages.push(json!({ "role": openai_role, "content": content }));
        }

        let body = json!({ "model": MODEL, "messages": messages, "stream": true });

        let resp = http
            .post(ORCA_URL)
            .header("Authorization", format!("Bearer {}", orca_api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("خطا در اتصال به OrcaRouter: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("OrcaRouter ({}): {}", status, text));
        }

        let mut byte_stream = resp.bytes_stream();
        let mut full_text = String::new();
        let mut buffer = String::new();
        let mut last_edit = Instant::now();
        let min_edit_interval = Duration::from_millis(700);

        while let Some(chunk_result) = byte_stream.next().await {
            let chunk = chunk_result.map_err(|e| format!("خطا در stream: {}", e))?;
            let chunk_str = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_str);

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer.drain(..=newline_pos);
                if line.is_empty() {
                    continue;
                }
                let data_part = match line.strip_prefix("data: ") {
                    Some(d) => d,
                    None => continue,
                };
                if data_part == "[DONE]" {
                    continue;
                }
                if let Ok(parsed) = serde_json::from_str::<Value>(data_part) {
                    if let Some(delta_text) = parsed["choices"][0]["delta"]["content"].as_str() {
                        full_text.push_str(delta_text);
                        if last_edit.elapsed() >= min_edit_interval && !full_text.is_empty() {
                            let _ = tg.edit_message(chat_id, placeholder_message_id, &full_text).await;
                            last_edit = Instant::now();
                        }
                    }
                }
            }
        }

        if full_text.is_empty() {
            full_text = "متاسفانه پاسخی دریافت نشد.".to_string();
        }
        let _ = tg.edit_message(chat_id, placeholder_message_id, &full_text).await;

        Ok(StreamResult { full_text, model_used: MODEL.to_string() })
    }
}

// ================= فراموشی خودکار حافظه =================

pub async fn run_with_auto_forget<F, Fut>(
    chat_id_for_db: i64,
    is_group: bool,
    mut history: Vec<(String, String, i64)>,
    mut attempt: F,
) -> Result<StreamResult, String>
where
    F: FnMut(Vec<(String, String)>) -> Fut,
    Fut: std::future::Future<Output = Result<StreamResult, String>>,
{
    const MAX_ROUNDS: usize = 5;

    for _round in 0..MAX_ROUNDS {
        let plain: Vec<(String, String)> =
            history.iter().map(|(r, c, _)| (r.clone(), c.clone())).collect();

        match attempt(plain).await {
            Ok(res) => return Ok(res),
            Err(e) => {
                let is_memory_error = gemini::is_context_full_error(&e);
                if !is_memory_error || history.len() <= 1 {
                    return Err(e);
                }
                let cut = (history.len() / 2).max(1);
                let forgotten: Vec<i64> = history.drain(0..cut).map(|(_, _, id)| id).collect();

                if let Some(&last_kept_id) = forgotten.last() {
                    let _ = db::mark_forgotten_before(chat_id_for_db, is_group, last_kept_id);
                }
                continue;
            }
        }
    }

    Err("حتی بعد از فراموشی چندباره، باز هم پاسخ نگرفتیم.".to_string())
}

// ================= ریچ (Rich Message polish) =================

const RICH_SYSTEM_PROMPT: &str = r#"You are a text formatter. You will receive a message that was generated by an AI assistant and sent in Telegram. Your ONLY job is to clean up and organize its formatting for Telegram's Rich Message feature (Markdown).

Rules:
- Do NOT change the meaning, facts, numbers, links, usernames, or any content. Only reformat.
- Fix any mixed-up or jumbled Persian/English text direction issues.
- Where it genuinely helps clarity, use headers, bullet lists, numbered lists, tables, or blockquotes.
- If the text is already clean and well-organized, or too short/simple to benefit, return it UNCHANGED.
- Never add commentary, explanations, or anything not in the original text.
- Output ONLY the reformatted text in Markdown, nothing else."#;

fn extract_signature(text: &str) -> Vec<String> {
    let mut sig = Vec::new();
    for word in text.split_whitespace() {
        let cleaned: String = word.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        if cleaned.len() >= 2 {
            sig.push(cleaned);
        }
        if word.starts_with("http://") || word.starts_with("https://") || word.starts_with('@') {
            sig.push(word.to_string());
        }
    }
    sig
}

fn content_preserved(original: &str, polished: &str) -> bool {
    let orig_sig = extract_signature(original);
    let new_sig = extract_signature(polished);
    for item in &orig_sig {
        if !new_sig.contains(item) {
            return false;
        }
    }
    true
}

/// ✅ **اصلاح‌شده**: پیام Rich جدید می‌فرستد و پیام قدیمی را حذف می‌کند (رویکرد امن‌تر).
pub async fn polish_and_send_rich(
    http: &reqwest::Client,
    tg: &TelegramClient,
    gemini_api_key: &str,
    chat_id: i64,
    old_message_id: i64,
    original_text: &str,
) {
    if original_text.chars().count() <= RICH_MIN_CHARS {
        return;
    }

    let polished = match gemini::complete_once(http, gemini_api_key, Some(RICH_SYSTEM_PROMPT), original_text).await {
        Ok(p) => p,
        Err(_) => return, // خطا: متن خام می‌ماند
    };

    if !content_preserved(original_text, &polished) {
        return; // چیزی گم شده: متن خام می‌ماند
    }

    // پیام Rich جدید بفرست
    let result = tg.send_rich_message(chat_id, None, &polished, None).await;
    if result.is_ok() {
        // پیام قدیمی را حذف کن
        let _ = tg.delete_message(chat_id, old_message_id).await;
    }
}
