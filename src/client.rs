use serde_json::{json, Value};

pub struct TelegramClient {
    pub token: String,
    pub http: reqwest::Client,
}

pub struct EmojiText {
    pub custom_emoji_id: Option<String>,
    pub text: String,
}

impl EmojiText {
    pub fn plain(text: impl Into<String>) -> Self {
        EmojiText {
            custom_emoji_id: None,
            text: text.into(),
        }
    }

    pub fn with_emoji(custom_emoji_id: impl Into<String>, text: impl Into<String>) -> Self {
        EmojiText {
            custom_emoji_id: Some(custom_emoji_id.into()),
            text: text.into(),
        }
    }
}

fn build_text_with_emojis(parts: &[EmojiText]) -> (String, Vec<Value>) {
    let mut full_text = String::new();
    let mut entities = Vec::new();

    for part in parts {
        if let Some(emoji_id) = &part.custom_emoji_id {
            let placeholder = "⚡";
            let offset_utf16 = full_text.encode_utf16().count() as i64;
            let length_utf16 = placeholder.encode_utf16().count() as i64;

            entities.push(json!({
                "type": "custom_emoji",
                "offset": offset_utf16,
                "length": length_utf16,
                "custom_emoji_id": emoji_id
            }));

            full_text.push_str(placeholder);
        }
        full_text.push_str(&part.text);
    }

    (full_text, entities)
}

impl TelegramClient {
    pub fn new(token: String, http: reqwest::Client) -> Self {
        TelegramClient { token, http }
    }

    fn api_url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.token, method)
    }

    pub async fn get_updates(&self, offset: i64) -> reqwest::Result<Value> {
        let url = self.api_url("getUpdates");
        let resp = self
            .http
            .get(&url)
            .query(&[
                ("offset", offset.to_string()),
                ("timeout", "30".to_string()),
                ("allowed_updates", "[\"message\",\"callback_query\",\"my_chat_member\"]".to_string()),
            ])
            .send()
            .await?;
        resp.json::<Value>().await
    }

    pub async fn send_message(&self, chat_id: i64, text: &str) -> reqwest::Result<Value> {
        let url = self.api_url("sendMessage");
        let body = json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "Markdown"
        });
        let resp = self.http.post(&url).json(&body).send().await?;
        resp.json::<Value>().await
    }

    /// یک پیام ساده (بدون Markdown، برای جلوگیری از خطای پارس روی متن کاربر) به‌عنوان ریپلای روی
    /// یک پیام مشخص می‌فرستد. برای پیام‌های کوتاه سیستم مقام/قفل‌ها استفاده می‌شود.
    pub async fn reply_message(
        &self,
        chat_id: i64,
        reply_to_message_id: i64,
        text: &str,
    ) -> reqwest::Result<Value> {
        let url = self.api_url("sendMessage");
        let body = json!({
            "chat_id": chat_id,
            "text": text,
            "reply_parameters": { "message_id": reply_to_message_id }
        });
        let resp = self.http.post(&url).json(&body).send().await?;
        resp.json::<Value>().await
    }

    pub async fn delete_message(&self, chat_id: i64, message_id: i64) -> reqwest::Result<Value> {
        let url = self.api_url("deleteMessage");
        let body = json!({ "chat_id": chat_id, "message_id": message_id });
        let resp = self.http.post(&url).json(&body).send().await?;
        resp.json::<Value>().await
    }

    pub async fn send_message_with_emojis(
        &self,
        chat_id: i64,
        parts: &[EmojiText],
    ) -> reqwest::Result<Value> {
        let url = self.api_url("sendMessage");
        let (full_text, entities) = build_text_with_emojis(parts);

        let mut body = json!({
            "chat_id": chat_id,
            "text": full_text
        });
        if !entities.is_empty() {
            body["entities"] = json!(entities);
        }

        let resp = self.http.post(&url).json(&body).send().await?;
        resp.json::<Value>().await
    }

    pub async fn send_reply_with_emojis(
        &self,
        chat_id: i64,
        reply_to_message_id: i64,
        parts: &[EmojiText],
    ) -> reqwest::Result<Value> {
        let url = self.api_url("sendMessage");
        let (full_text, entities) = build_text_with_emojis(parts);

        let mut body = json!({
            "chat_id": chat_id,
            "text": full_text,
            "reply_parameters": { "message_id": reply_to_message_id }
        });
        if !entities.is_empty() {
            body["entities"] = json!(entities);
        }

        let resp = self.http.post(&url).json(&body).send().await?;
        resp.json::<Value>().await
    }

    pub async fn send_model_keyboard(
        &self,
        chat_id: i64,
        parts: &[EmojiText],
        models: &[(&str, &str)],
    ) -> reqwest::Result<Value> {
        let url = self.api_url("sendMessage");
        let (full_text, entities) = build_text_with_emojis(parts);

        let mut keyboard_rows: Vec<Vec<Value>> = Vec::new();
        for pair in models.chunks(2) {
            let row: Vec<Value> = pair
                .iter()
                .map(|(name, color)| {
                    if color.is_empty() {
                        json!({ "text": name })
                    } else {
                        json!({ "text": name, "style": color })
                    }
                })
                .collect();
            keyboard_rows.push(row);
        }

        let mut body = json!({
            "chat_id": chat_id,
            "text": full_text,
            "reply_markup": {
                "keyboard": keyboard_rows,
                "resize_keyboard": true
            }
        });
        if !entities.is_empty() {
            body["entities"] = json!(entities);
        }

        let resp = self.http.post(&url).json(&body).send().await?;
        resp.json::<Value>().await
    }

    /// کیبورد Inline رنگی برای انتخاب مدل (زیر پیام، برای گروه).
    /// هر دکمه: (متن، callback_data، رنگ)
    pub async fn send_inline_model_keyboard(
        &self,
        chat_id: i64,
        reply_to_message_id: i64,
        text: &str,
        buttons: &[(String, String, String)],
    ) -> reqwest::Result<Value> {
        let url = self.api_url("sendMessage");

        let mut keyboard_rows: Vec<Vec<Value>> = Vec::new();
        for pair in buttons.chunks(2) {
            let row: Vec<Value> = pair
                .iter()
                .map(|(txt, cb, color)| {
                    let mut b = json!({ "text": txt, "callback_data": cb });
                    if !color.is_empty() {
                        b["style"] = json!(color);
                    }
                    b
                })
                .collect();
            keyboard_rows.push(row);
        }

        let body = json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "Markdown",
            "reply_parameters": { "message_id": reply_to_message_id },
            "reply_markup": { "inline_keyboard": keyboard_rows }
        });

        let resp = self.http.post(&url).json(&body).send().await?;
        resp.json::<Value>().await
    }

    pub async fn edit_message(
        &self,
        chat_id: i64,
        message_id: i64,
        new_text: &str,
    ) -> reqwest::Result<Value> {
        let url = self.api_url("editMessageText");
        let body = json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": new_text
        });
        let resp = self.http.post(&url).json(&body).send().await?;
        resp.json::<Value>().await
    }

    pub async fn send_typing(&self, chat_id: i64) -> reqwest::Result<()> {
        let url = self.api_url("sendChatAction");
        let body = json!({ "chat_id": chat_id, "action": "typing" });
        let _ = self.http.post(&url).json(&body).send().await?;
        Ok(())
    }

    pub async fn set_reaction(
        &self,
        chat_id: i64,
        message_id: i64,
        emoji: &str,
    ) -> reqwest::Result<()> {
        let url = self.api_url("setMessageReaction");
        let body = json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "reaction": [{ "type": "emoji", "emoji": emoji }]
        });
        let _ = self.http.post(&url).json(&body).send().await?;
        Ok(())
    }

    pub async fn get_chat_member_status(
        &self,
        chat_ref: &str,
        user_id: i64,
    ) -> reqwest::Result<Option<String>> {
        let url = self.api_url("getChatMember");
        let resp = self
            .http
            .get(&url)
            .query(&[
                ("chat_id", chat_ref.to_string()),
                ("user_id", user_id.to_string()),
            ])
            .send()
            .await?;
        let v: Value = resp.json().await?;
        Ok(v["result"]["status"].as_str().map(|s| s.to_string()))
    }

    pub async fn get_chat_administrators(&self, chat_id: i64) -> reqwest::Result<Value> {
        let url = self.api_url("getChatAdministrators");
        let body = json!({ "chat_id": chat_id });
        let resp = self.http.post(&url).json(&body).send().await?;
        resp.json::<Value>().await
    }

    pub async fn export_chat_invite_link(&self, chat_id: i64) -> reqwest::Result<Value> {
        let url = self.api_url("exportChatInviteLink");
        let body = json!({ "chat_id": chat_id });
        let resp = self.http.post(&url).json(&body).send().await?;
        resp.json::<Value>().await
    }

    pub async fn send_inline_message(
        &self,
        chat_id: i64,
        text: &str,
        rows: &[Vec<InlineButton<'_>>],
    ) -> reqwest::Result<Value> {
        let url = self.api_url("sendMessage");

        let keyboard: Vec<Vec<Value>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|btn| {
                        let mut b = json!({ "text": btn.text });
                        if !btn.color.is_empty() {
                            b["style"] = json!(btn.color);
                        }
                        match btn.action {
                            InlineAction::Url(u) => b["url"] = json!(u),
                            InlineAction::Callback(c) => b["callback_data"] = json!(c),
                        }
                        b
                    })
                    .collect()
            })
            .collect();

        let body = json!({
            "chat_id": chat_id,
            "text": text,
            "reply_markup": { "inline_keyboard": keyboard }
        });

        let resp = self.http.post(&url).json(&body).send().await?;
        resp.json::<Value>().await
    }

    pub async fn answer_callback_query(&self, callback_query_id: &str) -> reqwest::Result<()> {
        let url = self.api_url("answerCallbackQuery");
        let body = json!({ "callback_query_id": callback_query_id });
        let _ = self.http.post(&url).json(&body).send().await?;
        Ok(())
    }

    // ================= Rich Message (Bot API 10.1+) =================

    /// نسبت حروف فارسی به کل حروف الفبایی متن را حساب می‌کند تا RTL بودن تشخیص داده شود.
    fn detect_rtl(text: &str) -> bool {
        let mut persian = 0usize;
        let mut latin = 0usize;
        for ch in text.chars() {
            if ('\u{0600}'..='\u{06FF}').contains(&ch) || ('\u{FB50}'..='\u{FDFF}').contains(&ch) {
                persian += 1;
            } else if ch.is_ascii_alphabetic() {
                latin += 1;
            }
        }
        persian > latin
    }

    /// یک پیام تازه را به‌صورت Rich Message می‌فرستد. اگر تلگرام Markdown را رد کند،
    /// به HTML خام (escape‌شده) و در نهایت به متن ساده برمی‌گردد تا پیام هرگز گم نشود.
    /// دکمه‌های inline اختیاری‌اند.
    pub async fn send_rich_message(
        &self,
        chat_id: i64,
        reply_to_message_id: Option<i64>,
        markdown: &str,
        buttons: Option<&[Vec<InlineButton<'_>>]>,
    ) -> reqwest::Result<Value> {
        let is_rtl = Self::detect_rtl(markdown);
        let keyboard = buttons.map(|rows| {
            let kb: Vec<Vec<Value>> = rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|btn| {
                            let mut b = json!({ "text": btn.text });
                            match btn.action {
                                InlineAction::Url(u) => b["url"] = json!(u),
                                InlineAction::Callback(c) => b["callback_data"] = json!(c),
                            }
                            b
                        })
                        .collect()
                })
                .collect();
            json!({ "inline_keyboard": kb })
        });

        // تلاش ۱: sendRichMessage با Markdown
        let url = self.api_url("sendRichMessage");
        let mut body = json!({
            "chat_id": chat_id,
            "rich_message": {
                "markdown": markdown,
                "is_rtl": is_rtl
            }
        });
        if let Some(mid) = reply_to_message_id {
            body["reply_parameters"] = json!({ "message_id": mid });
        }
        if let Some(kb) = &keyboard {
            body["reply_markup"] = kb.clone();
        }
        let resp = self.http.post(&url).json(&body).send().await?;
        let v: Value = resp.json().await?;
        if v["ok"].as_bool() == Some(true) {
            return Ok(v);
        }

        // تلاش ۲: sendRichMessage با HTML (متن خام، بدون قالب‌بندی خاص، فقط escape‌شده)
        let escaped = markdown
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let mut body_html = json!({
            "chat_id": chat_id,
            "rich_message": {
                "html": format!("<p>{}</p>", escaped),
                "is_rtl": is_rtl
            }
        });
        if let Some(mid) = reply_to_message_id {
            body_html["reply_parameters"] = json!({ "message_id": mid });
        }
        if let Some(kb) = &keyboard {
            body_html["reply_markup"] = kb.clone();
        }
        let resp2 = self.http.post(&url).json(&body_html).send().await?;
        let v2: Value = resp2.json().await?;
        if v2["ok"].as_bool() == Some(true) {
            return Ok(v2);
        }

        // تلاش ۳: پیام متنی ساده معمولی (بدون هیچ قالب‌بندی‌ای) - آخرین خط دفاعی
        let plain_url = self.api_url("sendMessage");
        let mut body_plain = json!({
            "chat_id": chat_id,
            "text": markdown
        });
        if let Some(mid) = reply_to_message_id {
            body_plain["reply_parameters"] = json!({ "message_id": mid });
        }
        if let Some(kb) = &keyboard {
            body_plain["reply_markup"] = kb.clone();
        }
        let resp3 = self.http.post(&plain_url).json(&body_plain).send().await?;
        resp3.json::<Value>().await
    }

    /// یک پیام موجود را با Rich Message ویرایش می‌کند، با همان زنجیره بازگشت سه‌مرحله‌ای.
    pub async fn edit_rich_message(
        &self,
        chat_id: i64,
        message_id: i64,
        markdown: &str,
    ) -> reqwest::Result<Value> {
        let is_rtl = Self::detect_rtl(markdown);

        // تلاش ۱: ویرایش با Rich Markdown
        let url = self.api_url("editMessageText");
        let body = json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "rich_message": {
                "markdown": markdown,
                "is_rtl": is_rtl
            }
        });
        let resp = self.http.post(&url).json(&body).send().await?;
        let v: Value = resp.json().await?;
        if v["ok"].as_bool() == Some(true) {
            return Ok(v);
        }

        // تلاش ۲: ویرایش با Rich HTML
        let escaped = markdown
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let body_html = json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "rich_message": {
                "html": format!("<p>{}</p>", escaped),
                "is_rtl": is_rtl
            }
        });
        let resp2 = self.http.post(&url).json(&body_html).send().await?;
        let v2: Value = resp2.json().await?;
        if v2["ok"].as_bool() == Some(true) {
            return Ok(v2);
        }

        // تلاش ۳: ویرایش با متن ساده (همان چیزی که از قبل آنجا بود می‌ماند اگر این هم شکست بخورد)
        let body_plain = json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": markdown
        });
        let resp3 = self.http.post(&url).json(&body_plain).send().await?;
        resp3.json::<Value>().await
    }
}

pub enum InlineAction<'a> {
    Url(&'a str),
    Callback(&'a str),
}

pub struct InlineButton<'a> {
    pub text: &'a str,
    pub color: &'a str,
    pub action: InlineAction<'a>,
}
