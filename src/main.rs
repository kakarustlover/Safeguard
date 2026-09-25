mod db;
mod client;
mod ai;
mod protection;

use mimalloc::MiMalloc;
use serde_json::{json, Value};
use client::{EmojiText, InlineAction, InlineButton, TelegramClient};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

// ============ آنتی‌اسپم ============
mod antispam {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    const MIN_INTERVAL: Duration = Duration::from_secs(5);
    const SPAM_BURST_COUNT: usize = 3;
    const SPAM_WINDOW: Duration = Duration::from_secs(2);
    pub const MUTE_DURATION_SECS: i64 = 600;

    struct UserState {
        request_in_progress: bool,
        last_completed_at: Option<Instant>,
        recent_messages: Vec<Instant>,
    }

    impl UserState {
        fn new() -> Self {
            UserState {
                request_in_progress: false,
                last_completed_at: None,
                recent_messages: Vec::new(),
            }
        }
    }

    static STATE: Mutex<Option<HashMap<i64, UserState>>> = Mutex::new(None);

    fn with_state<F, R>(f: F) -> R
    where
        F: FnOnce(&mut HashMap<i64, UserState>) -> R,
    {
        let mut guard = STATE.lock().unwrap();
        if guard.is_none() {
            *guard = Some(HashMap::new());
        }
        f(guard.as_mut().unwrap())
    }

    pub enum CheckResult {
        Allowed,
        StillProcessing,
        TooSoon,
        SpamDetected,
    }

    pub fn check_and_register(user_id: i64) -> CheckResult {
        with_state(|map| {
            let state = map.entry(user_id).or_insert_with(UserState::new);
            let now = Instant::now();
            state.recent_messages.push(now);
            state.recent_messages.retain(|t| now.duration_since(*t) <= SPAM_WINDOW);

            if state.recent_messages.len() >= SPAM_BURST_COUNT {
                return CheckResult::SpamDetected;
            }
            if state.request_in_progress {
                return CheckResult::StillProcessing;
            }
            if let Some(last) = state.last_completed_at {
                if now.duration_since(last) < MIN_INTERVAL {
                    return CheckResult::TooSoon;
                }
            }
            state.request_in_progress = true;
            CheckResult::Allowed
        })
    }

    pub fn mark_completed(user_id: i64) {
        with_state(|map| {
            if let Some(state) = map.get_mut(&user_id) {
                state.request_in_progress = false;
                state.last_completed_at = Some(Instant::now());
            }
        });
    }

    pub fn clear_user(user_id: i64) {
        with_state(|map| {
            map.remove(&user_id);
        });
    }
}

// ============ پیکربندی (از متغیرهای محیطی Railway) ============

fn env_var(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| {
        eprintln!("❌ متغیر محیطی {} تنظیم نشده. در Railway → Variables اضافه‌اش کن.", key);
        std::process::exit(1);
    })
}

fn env_var_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

static BOT_TOKEN_CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static GEMINI_API_KEY_CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static ORCA_API_KEY_CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static GROQ_API_KEY_CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static OWNER_ID_CELL: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
static BOT_USERNAME_CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static SUPPORT_URL_CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static DB_PATH_CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn bot_token() -> &'static str {
    BOT_TOKEN_CELL.get_or_init(|| env_var("BOT_TOKEN"))
}
fn gemini_api_key() -> &'static str {
    GEMINI_API_KEY_CELL.get_or_init(|| env_var("GEMINI_API_KEY"))
}
fn orca_api_key() -> &'static str {
    ORCA_API_KEY_CELL.get_or_init(|| env_var("ORCA_API_KEY"))
}
fn groq_api_key() -> &'static str {
    GROQ_API_KEY_CELL.get_or_init(|| env_var("GROQ_API_KEY"))
}
fn owner_id() -> i64 {
    *OWNER_ID_CELL.get_or_init(|| {
        env_var("OWNER_ID")
            .parse()
            .unwrap_or_else(|_| {
                eprintln!("❌ OWNER_ID باید یک عدد باشه.");
                std::process::exit(1);
            })
    })
}
fn bot_username() -> &'static str {
    BOT_USERNAME_CELL.get_or_init(|| env_var("BOT_USERNAME"))
}
fn support_url() -> &'static str {
    SUPPORT_URL_CELL.get_or_init(|| env_var("SUPPORT_URL"))
}
fn db_path() -> &'static str {
    DB_PATH_CELL.get_or_init(|| env_var_or("DB_PATH", "bot.db"))
}

const DEFAULT_GROUP_MODEL: &str = "gemini-3.5-flash-lite";

const EMOJI_GUIDE_HINT: &str = "5242540542964831316";
const EMOJI_THINKING: &str = "5330257244166572322";

const EMOJI_CLAUDE: &str = "5244474931810438131";
const EMOJI_GEMINI: &str = "5246819764910727208";
const EMOJI_GROK: &str = "5247179567206016466";
const EMOJI_CHATGPT: &str = "5251681865458293966";
const EMOJI_DARTH_VADER: &str = "5235488215254736349";
const EMOJI_STAR: &str = "5246727573437723110";
const EMOJI_WHALE_DEEPSEEK: &str = "5832498002362638135";
const EMOJI_LIGHTNING: &str = "5774172509691715895";

const BTN_MODEL_35_LITE: &str = "🟢 Gemini 3.5 Flash Lite";
const BTN_MODEL_DEEPSEEK: &str = "🔵 DeepSeek V4 Flash";
const BTN_MODEL_GROQ: &str = "🔴 GPT-OSS-120B";

const MODEL_DESC_GEMINI: &str = "🟢 *Gemini 3.5 Flash Lite*\nسریع، سبک و دقیق. گزینه‌ی عالی برای چت روزمره، سوالات عمومی و پاسخ‌های فوری. تعادل خوبی بین سرعت و کیفیت داره.";
const MODEL_DESC_DEEPSEEK: &str = "🔵 *DeepSeek V4 Flash*\nمدل چینی قدرتمند برای کارهای پیچیده. توی تحلیل، استدلال منطقی، ریاضیات و برنامه‌نویسی فوق‌العاده عمل می‌کنه. اگه دنبال جواب عمیق و دقیق هستی، این انتخاب درسته.";
const MODEL_DESC_GROQ: &str = "🔴 *GPT-OSS-120B*\nمدل قدرتمند با سرعت پاسخ‌دهی بالا. دقت خوبی داره و برای کارهای سریع و روزمره گزینه‌ی مناسبیه.";

const BTN_CHAT_AI: &str = "🔵 چت با هوش مصنوعی";
const BTN_SUPPORT: &str = "🔴 پشتیبانی";
const BTN_BACK: &str = "❌ بازگشت";

const MAIN_MENU: &[(&str, &str)] = &[
    (BTN_CHAT_AI, "primary"),
    (BTN_SUPPORT, "danger"),
];

const CHAT_MODEL_MENU: &[(&str, &str)] = &[
    (BTN_MODEL_35_LITE, "success"),
    (BTN_MODEL_DEEPSEEK, "primary"),
    (BTN_MODEL_GROQ, "danger"),
    (BTN_BACK, ""),
];

const BACK_ONLY: &[(&str, &str)] = &[(BTN_BACK, "")];

const GROUP_MODEL_MENU: &[(&str, &str)] = &[
    (BTN_MODEL_35_LITE, "success"),
    (BTN_MODEL_DEEPSEEK, "primary"),
    (BTN_MODEL_GROQ, "danger"),
];

fn resolve_chat_model_id(button_text: &str) -> Option<&'static str> {
    let t = button_text.trim();
    if t == BTN_MODEL_35_LITE || t == "gemini" || t == "gemini-3.5-flash-lite" {
        Some("gemini-3.5-flash-lite")
    } else if t == BTN_MODEL_DEEPSEEK || t == "deepseek" {
        Some(ai::deepseek::MODEL)
    } else if t == BTN_MODEL_GROQ || t == "groq" || t == "gpt" {
        Some(ai::groq::MODEL)
    } else {
        None
    }
}

fn model_display_name(model_id: &str) -> &'static str {
    if model_id == "gemini-3.5-flash-lite" {
        "Gemini 3.5 Flash Lite"
    } else if model_id == ai::deepseek::MODEL {
        "DeepSeek V4 Flash"
    } else if model_id == ai::groq::MODEL {
        "GPT-OSS-120B"
    } else {
        "مدل انتخابی"
    }
}

fn is_deepseek_model(m: &str) -> bool {
    m == ai::deepseek::MODEL
}
fn is_groq_model(m: &str) -> bool {
    m == ai::groq::MODEL
}

fn first_name_for(user_id: i64) -> Option<&'static str> {
    match user_id {
        7383778633 => Some("کاکا"),
        6837607506 => Some("امین"),
        8907987684 => Some("پابلو"),
        5902213872 => Some("آراز"),
        8233397688 => Some("شیرین"),
        8172471963 => Some("سامی پدو"),
        7030598934 => Some("گربه ماهی"),
        6774873781 => Some("ویچر"),
        1974887406 | 8747692661 => Some("امیر حسین پنیک"),
        _ => None,
    }
}

fn chat_intro_parts() -> Vec<EmojiText> {
    let text = format!(
        "به بخش چت با هوش مصنوعی خوش اومدی 💬\n\nقبل از شروع یکی از مدل‌ها رو انتخاب کن:\n\n{}\n\n{}\n\n{}\n\nاز دکمه‌های پایین انتخاب کن 👇",
        MODEL_DESC_GEMINI, MODEL_DESC_DEEPSEEK, MODEL_DESC_GROQ
    );
    vec![EmojiText::plain(text)]
}

fn back_to_root_parts() -> Vec<EmojiText> {
    vec![EmojiText::plain("🔙 برگشتی به منوی اصلی، از دکمه‌های زیر انتخاب کن:")]
}

fn use_buttons_parts() -> Vec<EmojiText> {
    vec![EmojiText::plain("برای شروع از دکمه‌های زیر استفاده کن 👇")]
}

fn make_title_from_text(text: &str) -> String {
    let snippet: String = text.chars().take(30).collect();
    snippet.replace('\n', " ")
}

fn resolve_target_user_id(input: &str) -> Option<i64> {
    if let Ok(id) = input.parse::<i64>() {
        return Some(id);
    }
    db::find_user_id_by_username(input).unwrap_or(None)
}

fn pick_alternative_model(requested: &str) -> &'static str {
    if requested.contains("deepseek") {
        if db::is_model_shown("gemini-3.5-flash-lite") {
            "gemini-3.5-flash-lite"
        } else if db::is_model_shown("groq") {
            ai::groq::MODEL
        } else {
            "gemini-3.5-flash-lite"
        }
    } else if requested.contains("groq") || requested.contains("gpt-oss") {
        if db::is_model_shown("gemini-3.5-flash-lite") {
            "gemini-3.5-flash-lite"
        } else if db::is_model_shown("deepseek") {
            ai::deepseek::MODEL
        } else {
            "gemini-3.5-flash-lite"
        }
    } else {
        if db::is_model_shown("deepseek") {
            ai::deepseek::MODEL
        } else if db::is_model_shown("groq") {
            ai::groq::MODEL
        } else {
            ai::deepseek::MODEL
        }
    }
}

async fn mute_for_spam(
    tg: &TelegramClient,
    user_id: i64,
    username: Option<&str>,
    location: &str,
    duration_secs: i64,
) -> i64 {
    let until = db::now_ts() + duration_secs;
    let _ = db::add_mute(user_id, until, "spam");
    antispam::clear_user(user_id);

    let minutes = duration_secs / 60;
    let who = match username {
        Some(u) => format!("@{} (آیدی: {})", u, user_id),
        None => format!("آیدی: {}", user_id),
    };

    let notify = format!(
        "🛡️ *گزارش امنیتی*\n\nیک کاربر به‌خاطر اسپم، به مدت {} دقیقه سکوت شد.\n\n👤 کاربر: {}\n📍 محل: {}\n⏱️ مدت سکوت: {} دقیقه",
        minutes, who, location, minutes
    );
    let _ = tg.send_message(owner_id(), &notify).await;

    minutes
}

#[tokio::main]
async fn main() {
    db::set_db_path(db_path().to_string());
    db::init_db(owner_id(), DEFAULT_GROUP_MODEL).expect("راه‌اندازی دیتابیس شکست خورد");
    println!("✅ دیتابیس آماده شد (مسیر: {})", db_path());

    let http = reqwest::Client::builder()
        .build()
        .expect("ساخت HTTP client شکست خورد");

    let tg = TelegramClient::new(bot_token().to_string(), http.clone());
    println!("🤖 ربات شروع به کار کرد");

    let mut offset: i64 = 0;

    loop {
        let updates = match tg.get_updates(offset).await {
            Ok(u) => u,
            Err(e) => {
                eprintln!("خطا در دریافت آپدیت: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        let results = match updates["result"].as_array() {
            Some(r) => r.clone(),
            None => continue,
        };

        for update in results {
            if let Some(update_id) = update["update_id"].as_i64() {
                offset = update_id + 1;
            }

            let http = http.clone();
            let update = update.clone();

            tokio::spawn(async move {
                let tg = TelegramClient::new(bot_token().to_string(), http.clone());

                if let Some(cb) = update.get("callback_query") {
                    if !cb.is_null() {
                        handle_callback_query(&tg, cb.clone()).await;
                        return;
                    }
                }

                if let Some(mcm) = update.get("my_chat_member") {
                    if !mcm.is_null() {
                        handle_my_chat_member(&tg, mcm.clone()).await;
                        return;
                    }
                }

                handle_update(&tg, &http, gemini_api_key(), orca_api_key(), groq_api_key(), update).await;
            });
        }
    }
}

// ============ callback query ============

async fn handle_callback_query(tg: &TelegramClient, cb: Value) {
    let cb_id = cb["id"].as_str().unwrap_or("");
    let clicker_id = cb["from"]["id"].as_i64().unwrap_or(0);
    let data = cb["data"].as_str().unwrap_or("");
    let msg = &cb["message"];
    let chat_id = msg["chat"]["id"].as_i64().unwrap_or(0);
    let message_id = msg["message_id"].as_i64().unwrap_or(0);

    if data.starts_with("model:") {
        let parts: Vec<&str> = data.split(':').collect();
        if parts.len() != 3 {
            let _ = tg.answer_callback_query(cb_id).await;
            return;
        }
        let model_key = parts[1];
        let owner_id: i64 = match parts[2].parse() {
            Ok(id) => id,
            Err(_) => {
                let _ = tg.answer_callback_query(cb_id).await;
                return;
            }
        };

        if clicker_id != owner_id {
            let _ = tg.answer_callback_query(cb_id).await;
            let _ = tg
                .send_message(
                    chat_id,
                    "⚠️ این دکمه برای شما نیست. اگه می‌خوای مدل خودت رو انتخاب کنی، `/model` رو بزن.",
                )
                .await;
            return;
        }

        let _ = tg.answer_callback_query(cb_id).await;

        let model_id = match model_key {
            "gemini" => "gemini-3.5-flash-lite",
            "deepseek" => ai::deepseek::MODEL,
            "groq" => ai::groq::MODEL,
            _ => return,
        };

        if !db::is_model_shown(model_id) {
            let alt = pick_alternative_model(model_id);
            let _ = db::set_user_group_model(clicker_id, alt);
            let new_text = format!(
                "⚠️ *{}* موقتاً در دسترس نیست.\n\nبه‌جاش مدل *{}* برات فعال شد. وقتی مدل اصلی برگشت، دوباره `/model` بزن.",
                model_display_name(model_id),
                model_display_name(alt)
            );
            let _ = tg.edit_message(chat_id, message_id, &new_text).await;
            return;
        }

        let _ = db::set_user_group_model(clicker_id, model_id);

        let new_text = format!(
            "✅ *{}* برای شما فعال شد.\n\nاز این به بعد توی این گروه، جواب‌ها با این مدل داده می‌شن. اگه خواستی تغییر بدی، دوباره `/model` رو بزن.",
            model_display_name(model_id)
        );
        let _ = tg.edit_message(chat_id, message_id, &new_text).await;
        return;
    }

    // ✅ **اصلاح‌شده**: بازگشت به منوی اصلی، نه پیام معرفی قدیمی
    if let Some(model_key) = data.strip_prefix("chatmodel:") {
        let _ = tg.answer_callback_query(cb_id).await;

        if model_key == "back" {
            let _ = db::set_stage(chat_id, "root");
            let parts = back_to_root_parts();
            let _ = tg.send_model_keyboard(chat_id, &parts, MAIN_MENU).await;
            return;
        }

        let model_id = match model_key {
            "gemini" => "gemini-3.5-flash-lite",
            "deepseek" => ai::deepseek::MODEL,
            "groq" => ai::groq::MODEL,
            _ => return,
        };

        if !db::is_model_shown(model_id) {
            let alt = pick_alternative_model(model_id);
            let active = db::get_active_chat(chat_id).unwrap_or(None);
            if let Some(active) = &active {
                let _ = db::set_chat_model(active.id, alt);
            }
            let new_text = format!(
                "⚠️ *{}* موقتاً در دسترس نیست.\n\nبه‌جاش مدل *{}* برات فعال شد.",
                model_display_name(model_id),
                model_display_name(alt)
            );
            let _ = tg.edit_message(chat_id, message_id, &new_text).await;
            return;
        }

        let active = db::get_active_chat(chat_id).unwrap_or(None);
        if active.is_none() {
            let _ = db::create_chat(chat_id, "چت جدید");
        }
        let active = db::get_active_chat(chat_id).unwrap_or(None);
        if let Some(active) = &active {
            let _ = db::set_chat_model(active.id, model_id);
        }
        let _ = db::set_stage(chat_id, "chat_active");
        let new_text = format!("✅ *{}* فعال شد. حالا می‌تونی بی‌واسطه پیام بدی.", model_display_name(model_id));
        let _ = tg.edit_message(chat_id, message_id, &new_text).await;
        return;
    }

    let _ = tg.answer_callback_query(cb_id).await;
}

// ============ my_chat_member: ثبت اولین مالک وقتی ربات به گروه اضافه می‌شود ============

async fn handle_my_chat_member(tg: &TelegramClient, mcm: Value) {
    let chat_id = match mcm["chat"]["id"].as_i64() {
        Some(id) => id,
        None => return,
    };
    let chat_type = mcm["chat"]["type"].as_str().unwrap_or("");
    if chat_type != "group" && chat_type != "supergroup" {
        return;
    }

    let new_status = mcm["new_chat_member"]["status"].as_str().unwrap_or("");
    if new_status != "member" && new_status != "administrator" {
        return;
    }

    let adder_id = match mcm["from"]["id"].as_i64() {
        Some(id) => id,
        None => return,
    };

    let _ = db::ensure_first_owner(chat_id, adder_id);

    let welcome = "🎭 *SafeGuard is now protecting this group.*\n\nThe person who added me is the first owner. Use `ad` (reply to a member) to add bot admins, or `panel`-style text commands to configure locks and protections.";
    let _ = tg.send_message(chat_id, welcome).await;
}

// ============ /start: پیام Rich معرفی SafeGuard ============

async fn send_start_rich_message(tg: &TelegramClient, chat_id: i64, reply_to: i64) {
    // ✅ **اصلاح‌شده**: استفاده از فرمت `![alt](tg://emoji?id=...)` برای ایموجی‌های سفارشی
    // ✅ **اصلاح‌شده**: جدول با سرستون‌های مشخص برای رندر صحیح
    let text = format!(
        "# I'm SafeGuard ![{alt}](tg://emoji?id={darth})\n\n\
         **Your Smart Group Protector, Powered by artificial intelligence** \
         ![{alt}](tg://emoji?id={claude})![{alt}](tg://emoji?id={gemini})![{alt}](tg://emoji?id={grok})\n\n\
         ## What I can do?\n\n\
         | Feature | Description |\n\
         |---|---|\n\
         | **AI Moderation** | Detect and Remove Explicit or Inappropriate Content using AI |\n\
         | **Spam Blocking** | Block spam, flood, and unwanted links automatically |\n\
         | **AI Chat** | Chat with Powerful AI Models Right Inside the Group |\n\
         | **Member Control** | Warn, Mute, Ban, and Beyond |\n\n\
         **You can also Chat with AI Right here** ![{alt}](tg://emoji?id={chatgpt}) Just send /chat",
        alt = "🎭",
        darth = EMOJI_DARTH_VADER,
        claude = EMOJI_CLAUDE,
        gemini = EMOJI_GEMINI,
        grok = EMOJI_GROK,
        chatgpt = EMOJI_CHATGPT,
    );

    let group_link = format!("https://t.me/{}?startgroup=true", bot_username().trim_start_matches('@'));
    let buttons = vec![vec![InlineButton {
        text: "➕ Add me to a group",
        color: "primary",
        action: InlineAction::Url(&group_link),
    }]];

    let reply_id = if reply_to > 0 { Some(reply_to) } else { None };
    let _ = tg.send_rich_message(chat_id, reply_id, &text, Some(&buttons)).await;
}

// ============ /chat: منوی انتخاب مدل با Rich Message ============

async fn send_chat_model_rich_message(tg: &TelegramClient, chat_id: i64) {
    let text = format!(
        "# Pick Your AI ![{alt}](tg://emoji?id={lightning})\n\n\
         Every Model has its own Strengths — choose the one that fits what you need.\n\n\
         ## MODEL\n\n\
         | Model | Description |\n\
         |---|---|\n\
         | Gemini ![{alt}](tg://emoji?id={gemini}) | Quick, everyday chats ![{alt}](tg://emoji?id={star}) |\n\
         | ChatGPT ![{alt}](tg://emoji?id={chatgpt}) | Speed over precision ![{alt}](tg://emoji?id={star}) |\n\
         | DeepSeek ![{alt}](tg://emoji?id={whale}) | Fast and Powerful model ![{alt}](tg://emoji?id={star}) |\n\n\
         **Tap a model to begin →**",
        alt = "🎭",
        lightning = EMOJI_LIGHTNING,
        gemini = EMOJI_GEMINI,
        chatgpt = EMOJI_CHATGPT,
        whale = EMOJI_WHALE_DEEPSEEK,
        star = EMOJI_STAR,
    );

    let buttons = vec![
        vec![
            InlineButton { text: "Gemini", color: "success", action: InlineAction::Callback("chatmodel:gemini") },
            InlineButton { text: "ChatGPT", color: "primary", action: InlineAction::Callback("chatmodel:groq") },
        ],
        vec![
            InlineButton { text: "DeepSeek", color: "primary", action: InlineAction::Callback("chatmodel:deepseek") },
        ],
        vec![
            InlineButton { text: "🔙 Back", color: "danger", action: InlineAction::Callback("chatmodel:back") },
        ],
    ];

    let _ = tg.send_rich_message(chat_id, None, &text, Some(&buttons)).await;
}

// ============ handle update ============

async fn handle_update(
    tg: &TelegramClient,
    http: &reqwest::Client,
    gemini_api_key: &str,
    orca_api_key: &str,
    groq_api_key: &str,
    update: Value,
) {
    let message = &update["message"];
    if message.is_null() {
        return;
    }

    let chat_id = match message["chat"]["id"].as_i64() {
        Some(id) => id,
        None => return,
    };

    let chat_type = message["chat"]["type"].as_str().unwrap_or("private");
    let is_group = chat_type == "group" || chat_type == "supergroup";

    let user_id = match message["from"]["id"].as_i64() {
        Some(id) => id,
        None => return,
    };

    let message_id = message["message_id"].as_i64().unwrap_or(0);
    let username = message["from"]["username"].as_str();

    let _ = db::ensure_user(user_id, username);

    if db::is_banned(user_id).unwrap_or(false) {
        return;
    }

    if is_group {
        let group_title = message["chat"]["title"].as_str().unwrap_or("بدون عنوان");
        let group_username = message["chat"]["username"].as_str();

        // ✅ **اصلاح‌شده**: قبل از ساخت لینک جدید، وجود لینک در دیتابیس را بررسی می‌کند
        let join_link: Option<String> = if let Some(uname) = group_username {
            Some(format!("https://t.me/{}", uname))
        } else {
            let existing = db::get_group_join_link(chat_id).unwrap_or(None);
            match existing {
                Some(link) if !link.is_empty() => Some(link),
                _ => {
                    match tg.export_chat_invite_link(chat_id).await {
                        Ok(v) => v["result"].as_str().map(|s| s.to_string()),
                        Err(_) => None,
                    }
                }
            }
        };

        let _ = db::touch_group_full(chat_id, group_title, group_username, join_link.as_deref());
        let _ = db::touch_group_member(chat_id, user_id, username);
    }

    let text_opt_early = message["text"].as_str().map(|s| s.to_string());

    if let Some(text) = &text_opt_early {
        let trimmed = text.trim();
        if trimmed == "/off" && user_id == owner_id() {
            let _ = db::set_bot_enabled(false);
            let _ = tg.send_message(chat_id, "🔴 ربات خاموش شد.").await;
            return;
        }
        if trimmed == "/on" && user_id == owner_id() {
            let _ = db::set_bot_enabled(true);
            let _ = tg.send_message(chat_id, "🟢 ربات روشن شد.").await;
            return;
        }
    }

    if !db::is_bot_enabled().unwrap_or(true) {
        return;
    }

    if let Ok(Some(until)) = db::is_muted(user_id) {
        if is_group {
            if !is_directed_at_bot(message) {
                return;
            }
        }

        let remaining = (until - db::now_ts()).max(0);
        let mins = (remaining + 59) / 60;
        let msg = format!(
            "🚫 کاربر عزیز، به‌خاطر ارسال پیام‌های پشت‌سرهم، به مدت {} دقیقه دیگه نمی‌تونی از ربات استفاده کنی.\n\nکمی استراحت کن، بعد دوباره برگرد. 🙏",
            mins
        );
        if is_group {
            let _ = tg
                .send_reply_with_emojis(chat_id, message_id, &[EmojiText::plain(msg)])
                .await;
        } else {
            let _ = tg.send_message(chat_id, &msg).await;
        }
        return;
    }

    if is_group {
        handle_group_message(
            tg, http, gemini_api_key, orca_api_key, groq_api_key, chat_id, user_id, message_id,
            message,
        )
        .await;
        return;
    }

    // ========== پیوی ==========
    let text_opt = text_opt_early;

    if let Some(text) = &text_opt {
        if text.starts_with("/see") {
            if user_id != owner_id() {
                return;
            }
            handle_see_command(tg, chat_id, text).await;
            return;
        }

        if text.starts_with("/list") {
            if user_id != owner_id() {
                return;
            }
            handle_list_command(tg, chat_id, text).await;
            return;
        }

        if text.starts_with("/add ") || text == "/add" {
            if user_id != owner_id() {
                return;
            }
            handle_add_mute(tg, chat_id, text).await;
            return;
        }

        if text.starts_with("/remove ") || text == "/remove" {
            if user_id != owner_id() {
                return;
            }
            handle_remove_mute(tg, chat_id, text).await;
            return;
        }

        if text.trim() == "/ison" {
            if user_id != owner_id() {
                return;
            }
            let _ = db::set_system_prompt_enabled(true);
            let _ = tg.send_message(chat_id, "🟢 پرامپت سیستمی *روشن* شد. از این به بعد توی گروه‌ها اعمال می‌شه.").await;
            return;
        }
        if text.trim() == "/isoff" {
            if user_id != owner_id() {
                return;
            }
            let _ = db::set_system_prompt_enabled(false);
            let _ = tg.send_message(chat_id, "🔴 پرامپت سیستمی *خاموش* شد.").await;
            return;
        }
        if text.starts_with("/how") {
            if user_id != owner_id() {
                return;
            }
            handle_how_command(tg, chat_id, text).await;
            return;
        }

        if text.trim() == "/deepseekoffshow" {
            if user_id != owner_id() {
                return;
            }
            let _ = db::set_model_shown("deepseek", false);
            let _ = tg.send_message(chat_id, "🔴 *DeepSeek V4 Flash* موقتاً از دسترس خارج شد.\n\nاگه کسی انتخابش کنه، بهش می‌گیم در دسترس نیست و از مدل جایگزین استفاده می‌کنه.").await;
            return;
        }
        if text.trim() == "/gptoffshow" {
            if user_id != owner_id() {
                return;
            }
            let _ = db::set_model_shown("groq", false);
            let _ = tg.send_message(chat_id, "🔴 *GPT-OSS-120B* موقتاً از دسترس خارج شد.\n\nاگه کسی انتخابش کنه، بهش می‌گیم در دسترس نیست و از مدل جایگزین استفاده می‌کنه.").await;
            return;
        }
        if text.trim() == "/gemenioffshow" {
            if user_id != owner_id() {
                return;
            }
            let _ = db::set_model_shown("gemini", false);
            let _ = tg.send_message(chat_id, "🔴 *Gemini 3.5 Flash Lite* موقتاً از دسترس خارج شد.\n\nاگه کسی انتخابش کنه، بهش می‌گیم در دسترس نیست و از مدل جایگزین استفاده می‌کنه.").await;
            return;
        }

        if text.trim() == "/deepseekshowon" {
            if user_id != owner_id() {
                return;
            }
            let _ = db::set_model_shown("deepseek", true);
            let _ = tg.send_message(chat_id, "🟢 *DeepSeek V4 Flash* دوباره در دسترس قرار گرفت.").await;
            return;
        }
        if text.trim() == "/gptshowon" {
            if user_id != owner_id() {
                return;
            }
            let _ = db::set_model_shown("groq", true);
            let _ = tg.send_message(chat_id, "🟢 *GPT-OSS-120B* دوباره در دسترس قرار گرفت.").await;
            return;
        }
        if text.trim() == "/gemenishowon" {
            if user_id != owner_id() {
                return;
            }
            let _ = db::set_model_shown("gemini", true);
            let _ = tg.send_message(chat_id, "🟢 *Gemini 3.5 Flash Lite* دوباره در دسترس قرار گرفت.").await;
            return;
        }

        if text.starts_with("/addadmin") {
            if user_id != owner_id() {
                return;
            }
            let parts: Vec<&str> = text.split_whitespace().collect();
            if parts.len() != 2 {
                let _ = tg.send_message(chat_id, "فرمت: `/addadmin آیدی_عددی`").await;
                return;
            }
            if let Ok(target) = parts[1].parse::<i64>() {
                let _ = db::add_admin(target);
                let _ = tg.send_message(chat_id, &format!("✅ آیدی {} ادمین شد.", target)).await;
            }
            return;
        }
        if text.starts_with("/removeadmin") {
            if user_id != owner_id() {
                return;
            }
            let parts: Vec<&str> = text.split_whitespace().collect();
            if parts.len() != 2 {
                let _ = tg.send_message(chat_id, "فرمت: `/removeadmin آیدی_عددی`").await;
                return;
            }
            if let Ok(target) = parts[1].parse::<i64>() {
                if target == owner_id() {
                    let _ = tg.send_message(chat_id, "مالک اصلی رو نمی‌تونی حذف کنی.").await;
                    return;
                }
                let _ = db::remove_admin(target);
                let _ = tg.send_message(chat_id, &format!("✅ آیدی {} از ادمین‌ها حذف شد.", target)).await;
            }
            return;
        }
        if text.starts_with("/ban") {
            if !db::is_admin(user_id).unwrap_or(false) {
                return;
            }
            let parts: Vec<&str> = text.split_whitespace().collect();
            if parts.len() != 2 {
                let _ = tg.send_message(chat_id, "فرمت: `/ban آیدی یا @یوزرنیم`").await;
                return;
            }
            if let Some(target) = resolve_target_user_id(parts[1]) {
                let _ = db::set_banned(target, true);
                antispam::clear_user(target);
                let _ = tg.send_message(chat_id, &format!("🚫 کاربر {} بن شد.", target)).await;
            } else {
                let _ = tg.send_message(chat_id, "کاربر پیدا نشد.").await;
            }
            return;
        }
        if text.starts_with("/unban") {
            if !db::is_admin(user_id).unwrap_or(false) {
                return;
            }
            let parts: Vec<&str> = text.split_whitespace().collect();
            if parts.len() != 2 {
                let _ = tg.send_message(chat_id, "فرمت: `/unban آیدی یا @یوزرنیم`").await;
                return;
            }
            if let Some(target) = resolve_target_user_id(parts[1]) {
                let _ = db::set_banned(target, false);
                let _ = tg.send_message(chat_id, &format!("✅ کاربر {} آزاد شد.", target)).await;
            } else {
                let _ = tg.send_message(chat_id, "کاربر پیدا نشد.").await;
            }
            return;
        }

        if text.trim() == "/changemodel" {
            if user_id != owner_id() {
                return;
            }
            let current = db::get_group_model(DEFAULT_GROUP_MODEL).unwrap_or_else(|_| DEFAULT_GROUP_MODEL.to_string());
            let msg = format!(
                "مدل پیش‌فرض گروه‌ها: *{}*\n\nمدل جدید رو انتخاب کن:",
                model_display_name(&current)
            );
            let _ = tg.send_model_keyboard(chat_id, &[EmojiText::plain(msg)], GROUP_MODEL_MENU).await;
            let _ = db::set_stage(chat_id, "group_model_select");
            return;
        }

        if text.trim() == "/start" {
            let _ = db::set_stage(chat_id, "root");
            let active = db::get_active_chat(chat_id).unwrap_or(None);
            if active.is_none() {
                let _ = db::create_chat(chat_id, "چت جدید");
            }
            send_start_rich_message(tg, chat_id, 0).await;
            return;
        }

        let trimmed = text.trim();

        let stage_check = db::get_stage(chat_id).unwrap_or_else(|_| "root".to_string());
        if stage_check == "group_model_select" && user_id == owner_id() {
            if let Some(model_id) = resolve_chat_model_id(trimmed) {
                let _ = db::set_group_model(model_id);
                let _ = db::set_stage(chat_id, "root");
                let _ = tg.send_message(chat_id, &format!("✅ مدل گروه‌ها به *{}* تغییر کرد.", model_display_name(model_id))).await;
                return;
            }
        }

        if trimmed == BTN_CHAT_AI || trimmed.to_lowercase() == "/chat" || trimmed.to_lowercase() == "chat" {
            let _ = db::set_stage(chat_id, "chat_model_select");
            send_chat_model_rich_message(tg, chat_id).await;
            return;
        }

        if trimmed == BTN_SUPPORT {
            let rows = vec![vec![InlineButton {
                text: "ارتباط با پشتیبانی 💬",
                color: "primary",
                action: InlineAction::Url(support_url()),
            }]];
            let _ = tg
                .send_inline_message(chat_id, "هر سوال یا مشکلی داشتی مستقیم پیام بده 👇", &rows)
                .await;
            return;
        }

        if trimmed == BTN_BACK {
            let _ = db::set_stage(chat_id, "root");
            let parts = back_to_root_parts();
            let _ = tg.send_model_keyboard(chat_id, &parts, MAIN_MENU).await;
            return;
        }

        if let Some(model_id) = resolve_chat_model_id(trimmed) {
            if !db::is_model_shown(model_id) {
                let alt = pick_alternative_model(model_id);
                let active = db::get_active_chat(chat_id).unwrap_or(None);
                let chat_row_id = match active {
                    Some(c) => c.id,
                    None => db::create_chat(chat_id, "چت جدید").unwrap_or(0),
                };
                let _ = db::set_chat_model(chat_row_id, alt);
                let _ = db::set_stage(chat_id, "chat_active");
                let msg = format!(
                    "⚠️ *{}* موقتاً در دسترس نیست.\n\nبه‌جاش مدل *{}* برات فعال شد. وقتی مدل اصلی برگشت، دوباره از منو انتخابش کن.",
                    model_display_name(model_id),
                    model_display_name(alt)
                );
                let _ = tg.send_model_keyboard(chat_id, &[EmojiText::plain(msg)], BACK_ONLY).await;
                return;
            }

            let active = db::get_active_chat(chat_id).unwrap_or(None);
            let chat_row_id = match active {
                Some(c) => c.id,
                None => db::create_chat(chat_id, "چت جدید").unwrap_or(0),
            };
            let _ = db::set_chat_model(chat_row_id, model_id);
            let _ = db::set_stage(chat_id, "chat_active");
            let msg = format!(
                "✅ مدل *{}* فعال شد.\n\nحالا هرچی می‌خوای بپرس 💬",
                model_display_name(model_id)
            );
            let _ = tg.send_model_keyboard(chat_id, &[EmojiText::plain(msg)], BACK_ONLY).await;
            return;
        }

        if text == "/model" || text == "/newchat" {
            if text == "/newchat" {
                let _ = db::create_chat(chat_id, "چت جدید");
            }
            let _ = db::set_stage(chat_id, "chat_model_select");
            let parts = chat_intro_parts();
            let _ = tg.send_model_keyboard(chat_id, &parts, CHAT_MODEL_MENU).await;
            return;
        }
    }

    let text = match &text_opt {
        Some(t) => t.clone(),
        None => {
            let parts = use_buttons_parts();
            let _ = tg.send_model_keyboard(chat_id, &parts, MAIN_MENU).await;
            return;
        }
    };

    let stage = db::get_stage(chat_id).unwrap_or_else(|_| "root".to_string());
    if stage != "chat_active" {
        let parts = use_buttons_parts();
        let _ = tg.send_model_keyboard(chat_id, &parts, MAIN_MENU).await;
        return;
    }

    match antispam::check_and_register(user_id) {
        antispam::CheckResult::Allowed => {}
        antispam::CheckResult::StillProcessing => {
            let _ = tg.send_message(chat_id, "⏳ صبر کن تا جواب پیام قبلی بیاد.").await;
            return;
        }
        antispam::CheckResult::TooSoon => {
            let _ = tg.send_message(chat_id, "⏱ هر ۵ ثانیه فقط یک درخواست می‌تونی بفرستی.").await;
            return;
        }
        antispam::CheckResult::SpamDetected => {
            let mins = mute_for_spam(tg, user_id, username, "پیوی", antispam::MUTE_DURATION_SECS).await;
            let _ = tg.send_message(chat_id, &format!(
                "🚫 کاربر عزیز، به‌خاطر ارسال پیام‌های پشت‌سرهم، به مدت {} دقیقه نمی‌تونی از ربات استفاده کنی.\n\nکمی استراحت کن، بعد دوباره برگرد. 🙏",
                mins
            )).await;
            return;
        }
    }

    let active = match db::get_active_chat(chat_id).unwrap_or(None) {
        Some(c) => c,
        None => {
            let new_id = db::create_chat(chat_id, "چت جدید").unwrap_or(0);
            db::Chat {
                id: new_id,
                title: "چت جدید".to_string(),
                model: None,
            }
        }
    };

    let mut model = match &active.model {
        Some(m) => m.clone(),
        None => {
            antispam::mark_completed(user_id);
            let _ = db::set_stage(chat_id, "chat_model_select");
            let parts = chat_intro_parts();
            let _ = tg.send_model_keyboard(chat_id, &parts, CHAT_MODEL_MENU).await;
            return;
        }
    };

    if !db::is_model_shown(&model) {
        let alt = pick_alternative_model(&model).to_string();
        model = alt;
        let _ = db::set_chat_model(active.id, &model);
    }

    let history_with_ids = db::get_history_with_ids(active.id).unwrap_or_default();
    if history_with_ids.is_empty() {
        let _ = db::set_chat_title(active.id, &make_title_from_text(&text));
    }
    let _ = db::add_message(active.id, "user", &text);
    let new_user_msg_id = db::last_message_id(active.id).unwrap_or(0);
    let _ = db::increment_usage(user_id);

    let _ = tg.send_typing(chat_id).await;

    let placeholder_parts = vec![
        EmojiText::plain("در حال تفکر "),
        EmojiText::with_emoji(EMOJI_THINKING, ""),
    ];
    let placeholder = tg.send_message_with_emojis(chat_id, &placeholder_parts).await;
    let placeholder_message_id = match placeholder {
        Ok(v) => v["result"]["message_id"].as_i64().unwrap_or(0),
        Err(_) => 0,
    };

    if placeholder_message_id == 0 {
        let _ = tg.send_message(chat_id, "خطا در فرستادن پیام.").await;
        return;
    }

    let mut combined_history_ids: Vec<(String, String, i64)> = history_with_ids;
    combined_history_ids.push(("user".to_string(), text.clone(), new_user_msg_id));

    let model_for_closure = model.clone();
    let result = ai::run_with_auto_forget(active.id, false, combined_history_ids, |plain_history| {
        let model = model_for_closure.clone();
        async move {
            if is_deepseek_model(&model) {
                let hist: Vec<(String, String)> = plain_history
                    .iter()
                    .map(|(r, c)| {
                        let role = if r == "assistant" { "model".to_string() } else { "user".to_string() };
                        (role, c.clone())
                    })
                    .collect();
                ai::deepseek::stream(http, tg, orca_api_key, None, &hist, chat_id, placeholder_message_id).await
            } else if is_groq_model(&model) {
                let hist: Vec<(String, String)> = plain_history
                    .iter()
                    .map(|(r, c)| {
                        let role = if r == "assistant" { "model".to_string() } else { "user".to_string() };
                        (role, c.clone())
                    })
                    .collect();
                ai::groq::stream(http, tg, groq_api_key, None, &hist, chat_id, placeholder_message_id).await
            } else {
                let hist: Vec<ai::HistoryItem> = plain_history
                    .iter()
                    .map(|(r, c)| ai::HistoryItem { role: r.clone(), content: c.clone() })
                    .collect();
                ai::gemini::stream(http, tg, gemini_api_key, &model, None, &hist, chat_id, placeholder_message_id).await
            }
        }
    })
    .await;

    antispam::mark_completed(user_id);

    match result {
        Ok(sr) => {
            let _ = db::add_message(active.id, "assistant", &sr.full_text);
            ai::polish_and_send_rich(
                http,
                tg,
                gemini_api_key,
                chat_id,
                placeholder_message_id,
                &sr.full_text,
            )
            .await;
        }
        Err(e) => {
            eprintln!("خطا پیوی: {}", e);
            let msg = if user_id == owner_id() {
                format!("🔴 *خطای واقعی (فقط مالک)*\n\nمدل: {}\n\n{}", model, e)
            } else {
                "⚠️ یه مشکل فنی پیش اومد و نتونستم جوابت رو بدم.\n\nلطفاً چند لحظه بعد دوباره تلاش کن. 🙏".to_string()
            };
            let _ = tg.edit_message(chat_id, placeholder_message_id, &msg).await;
        }
    }
}

// ============ /see ============

async fn handle_see_command(tg: &TelegramClient, chat_id: i64, text: &str) {
    let parts: Vec<&str> = text.split_whitespace().collect();

    if parts.len() == 1 {
        let groups = db::list_groups_full().unwrap_or_default();
        let private_users = db::list_private_users().unwrap_or_default();
        let group_user_ids = db::list_group_users().unwrap_or_default();

        let mut msg = String::new();
        msg.push_str("📊 *آمار کامل ربات*\n");
        msg.push_str("━━━━━━━━━━━━━━━━━━━━━━\n\n");

        msg.push_str(&format!("👥 *گروه‌ها* ({}):\n", groups.len()));
        msg.push_str("━━━━━━━━━━━━━━━━━━━━━━\n");
        if groups.is_empty() {
            msg.push_str("— هیچ گروهی ثبت نشده\n");
        } else {
            for g in &groups {
                msg.push_str(&format!("▪️ {}\n", g.title));
                if let Some(uname) = &g.username {
                    msg.push_str(&format!("   📛 @{}\n", uname));
                }
                if let Some(link) = &g.join_link {
                    msg.push_str(&format!("   🔗 {}\n", link));
                } else {
                    msg.push_str("   🔗 بدون لینک\n");
                }
                msg.push('\n');
            }
        }

        msg.push_str(&format!("\n👤 *کاربران پیوی* ({}):\n", private_users.len()));
        msg.push_str("━━━━━━━━━━━━━━━━━━━━━━\n");
        if private_users.is_empty() {
            msg.push_str("— هیچ کاربری ثبت نشده\n");
        } else {
            for (uid, uname) in &private_users {
                match uname {
                    Some(u) => msg.push_str(&format!("▪️ @{} (آیدی: `{}`)\n", u, uid)),
                    None => msg.push_str(&format!("▪️ بدون یوزرنیم (آیدی: `{}`)\n", uid)),
                }
            }
        }

        msg.push_str(&format!("\n\n💬 کاربران فعال در گروه‌ها: *{}* نفر", group_user_ids.len()));

        send_long_message(tg, chat_id, &msg).await;
        return;
    }

    let target_input = parts[1];
    let target = match resolve_target_user_id(target_input) {
        Some(id) => id,
        None => {
            let _ = tg.send_message(chat_id, "❌ کاربر پیدا نشد.").await;
            return;
        }
    };

    let info = db::get_user_info(target).unwrap_or(None);
    let total_msgs = db::count_user_all_messages(target).unwrap_or(0);

    let mut header = String::new();
    header.push_str("👤 *اطلاعات کاربر*\n");
    header.push_str("━━━━━━━━━━━━━━━━━━━━━━\n\n");
    if let Some(info) = &info {
        if let Some(u) = &info.username {
            header.push_str(&format!("📛 یوزرنیم: @{}\n", u));
        } else {
            header.push_str("📛 یوزرنیم: ندارد\n");
        }
        header.push_str(&format!("🆔 آیدی عددی: `{}`\n", info.user_id));
        header.push_str(&format!("📨 مجموع پیام‌ها: {}\n", info.message_count));
    }

    if let Ok(Some(m)) = db::get_user_group_model(target) {
        header.push_str(&format!("\n🎯 مدل گروه: *{}*\n", model_display_name(&m)));
    }

    header.push_str(&format!("\n📊 مجموع پیام‌های ثبت‌شده: *{}*\n", total_msgs));
    header.push_str("\n━━━━━━━━━━━━━━━━━━━━━━\n");
    header.push_str("💬 *آخرین ۵۰ پیام:*\n\n");

    let msgs = db::get_user_all_messages(target, 50, 0).unwrap_or_default();
    if msgs.is_empty() {
        header.push_str("— هیچ پیامی ثبت نشده\n");
    } else {
        for (i, m) in msgs.iter().enumerate() {
            let who = if m.role == "user" { "👤 کاربر" } else { "🤖 ربات" };
            let src = if m.source == "private" { "پیوی" } else { "گروه" };
            header.push_str(&format!(
                "{}. [{} - {}]\n{}\n\n",
                i + 1,
                src,
                who,
                truncate_chars(&m.content, 200)
            ));
        }
    }

    send_long_message(tg, chat_id, &header).await;
}

// ============ /list ============

async fn handle_list_command(tg: &TelegramClient, chat_id: i64, text: &str) {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() < 2 {
        let _ = tg.send_message(chat_id, "فرمت: `/list <آیدی یا یوزرنیم گروه>`").await;
        return;
    }

    let group_ref = parts[1];
    let group_id = match db::find_group_by_ref(group_ref) {
        Ok(Some(id)) => id,
        _ => {
            let _ = tg.send_message(chat_id, "❌ گروه پیدا نشد.").await;
            return;
        }
    };

    let group_info = db::list_groups_full()
        .unwrap_or_default()
        .into_iter()
        .find(|g| g.chat_id == group_id);

    let group_title = group_info
        .as_ref()
        .map(|g| g.title.clone())
        .unwrap_or_else(|| "گروه".to_string());

    let admins_val = tg.get_chat_administrators(group_id).await.unwrap_or(Value::Null);
    let mut admin_ids: Vec<i64> = Vec::new();
    if let Some(arr) = admins_val["result"].as_array() {
        for a in arr {
            if let Some(uid) = a["user"]["id"].as_i64() {
                admin_ids.push(uid);
            }
        }
    }

    let members = db::list_group_members(group_id, 250).unwrap_or_default();
    let total_in_db = db::count_group_members(group_id).unwrap_or(0);

    let mut msg = String::new();
    msg.push_str(&format!("📋 *ممبرای گروه {}*\n", group_title));
    msg.push_str("━━━━━━━━━━━━━━━━━━━━━━\n\n");

    if !admin_ids.is_empty() {
        msg.push_str(&format!("👑 *ادمین‌ها* ({}):\n", admin_ids.len()));
        for uid in &admin_ids {
            let uname = members
                .iter()
                .find(|m| m.user_id == *uid)
                .and_then(|m| m.username.clone());
            match uname {
                Some(u) => msg.push_str(&format!("▪️ @{} (آیدی: `{}`)\n", u, uid)),
                None => msg.push_str(&format!("▪️ آیدی: `{}`\n", uid)),
            }
        }
        msg.push('\n');
    }

    msg.push_str(&format!(
        "👤 *اعضای شناخته‌شده* (نمایش {} از {}):\n",
        members.len(),
        total_in_db
    ));
    if members.is_empty() {
        msg.push_str("— هنوز عضوی توی دیتابیس ثبت نشده\n");
    } else {
        for (i, m) in members.iter().enumerate() {
            if admin_ids.contains(&m.user_id) {
                continue;
            }
            match &m.username {
                Some(u) => msg.push_str(&format!("{}. @{} (آیدی: `{}`)\n", i + 1, u, m.user_id)),
                None => msg.push_str(&format!("{}. آیدی: `{}`\n", i + 1, m.user_id)),
            }
        }
        if total_in_db > members.len() as i64 {
            msg.push_str(&format!(
                "\n⚠️ {} عضو دیگه هم هست که توی این لیست نیست.",
                total_in_db - members.len() as i64
            ));
        }
    }

    send_long_message(tg, chat_id, &msg).await;
}

// ============ /add /remove ============

async fn handle_add_mute(tg: &TelegramClient, chat_id: i64, text: &str) {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() != 3 {
        let _ = tg.send_message(chat_id, "فرمت: `/add <دقیقه> <آیدی یا @یوزرنیم>`").await;
        return;
    }

    let minutes: i64 = match parts[1].parse() {
        Ok(m) => m,
        Err(_) => {
            let _ = tg.send_message(chat_id, "❌ مدت زمان باید عدد باشه (به دقیقه).").await;
            return;
        }
    };

    let target = match resolve_target_user_id(parts[2]) {
        Some(id) => id,
        None => {
            let _ = tg.send_message(chat_id, "❌ کاربر پیدا نشد.").await;
            return;
        }
    };

    let secs = minutes * 60;
    let until = db::now_ts() + secs;
    let _ = db::add_mute(target, until, "manual");
    antispam::clear_user(target);

    let _ = tg.send_message(chat_id, &format!("✅ کاربر `{}` به مدت {} دقیقه سکوت شد.", target, minutes)).await;

    let notify = format!(
        "🚫 شما به مدت {} دقیقه از استفاده از ربات محروم شدید.\n\nکمی استراحت کن، بعد دوباره برگرد. 🙏",
        minutes
    );
    let _ = tg.send_message(target, &notify).await;
}

async fn handle_remove_mute(tg: &TelegramClient, chat_id: i64, text: &str) {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() != 2 {
        let _ = tg.send_message(chat_id, "فرمت: `/remove <آیدی یا @یوزرنیم>`").await;
        return;
    }

    let target = match resolve_target_user_id(parts[1]) {
        Some(id) => id,
        None => {
            let _ = tg.send_message(chat_id, "❌ کاربر پیدا نشد.").await;
            return;
        }
    };

    let _ = db::remove_mute(target);
    let _ = tg.send_message(chat_id, &format!("✅ سکوت کاربر `{}` برداشته شد.", target)).await;

    let notify = "✅ سکوت شما برداشته شد. حالا می‌تونی از ربات استفاده کنی.";
    let _ = tg.send_message(target, notify).await;
}

// ============ /how ============

async fn handle_how_command(tg: &TelegramClient, chat_id: i64, text: &str) {
    let prompt = if text.trim() == "/how" {
        String::new()
    } else {
        text.trim_start_matches("/how").trim().to_string()
    };

    if prompt.is_empty() {
        let current = db::get_system_prompt().unwrap_or_default();
        let enabled = db::is_system_prompt_enabled().unwrap_or(false);
        let status = if enabled { "🟢 روشن" } else { "🔴 خاموش" };
        let msg = format!(
            "📝 *پرامپت سیستمی فعلی:*\n\n{}\n\nوضعیت: {}\n\nبرای تعریف پرامپت جدید:\n`/how متن پرامپت`",
            if current.is_empty() { "— تعریف نشده —" } else { &current },
            status
        );
        let _ = tg.send_message(chat_id, &msg).await;
        return;
    }

    let _ = db::set_system_prompt(&prompt);

    let msg = format!(
        "✅ *پرامپت سیستمی تعریف شد.*\n\n📝 متن:\n«{}»\n\nبرای فعال‌سازی: `/ison`",
        prompt
    );
    let _ = tg.send_message(chat_id, &msg).await;
}

// ============ گروه ============

fn is_directed_at_bot(message: &Value) -> bool {
    let is_reply_to_bot = message["reply_to_message"]["from"]["is_bot"]
        .as_bool()
        .unwrap_or(false)
        && message["reply_to_message"]["from"]["username"]
            .as_str()
            .map(|u| format!("@{}", u).to_lowercase() == bot_username().to_lowercase())
            .unwrap_or(false);

    let is_mentioned = message["text"]
        .as_str()
        .map(|t| t.to_lowercase().contains(&bot_username().to_lowercase()))
        .unwrap_or(false);

    is_reply_to_bot || is_mentioned
}

async fn handle_group_message(
    tg: &TelegramClient,
    http: &reqwest::Client,
    gemini_api_key: &str,
    orca_api_key: &str,
    groq_api_key: &str,
    chat_id: i64,
    user_id: i64,
    user_message_id: i64,
    message: &Value,
) {
    let text_raw = match message["text"].as_str() {
        Some(t) => t,
        None => return,
    };

    // ---------- سیستم مقام ----------
    let reply_target = message.get("reply_to_message");
    if let Some(target_msg) = reply_target {
        if let Some(rc) = protection::resolve_rank_command(text_raw) {
            let target_id = target_msg["from"]["id"].as_i64().unwrap_or(0);
            let target_is_bot = target_msg["from"]["is_bot"].as_bool().unwrap_or(false);
            if target_id != 0 && !target_is_bot {
                let target_name = target_msg["from"]["first_name"]
                    .as_str()
                    .unwrap_or("کاربر")
                    .to_string();
                match protection::apply_rank_command(chat_id, user_id, target_id, &target_name, rc) {
                    protection::RankActionResult::Ok(msg) => {
                        let _ = tg.reply_message(chat_id, user_message_id, &msg).await;
                    }
                    protection::RankActionResult::Denied(msg) => {
                        let _ = tg.reply_message(chat_id, user_message_id, &msg).await;
                    }
                    protection::RankActionResult::NoTarget => {}
                }
                return;
            }
        }
    }

    // ---------- قفل‌ها ----------
    if let Some((is_lock, lock_type)) = protection::parse_lock_command(text_raw) {
        if let Some(target_msg) = reply_target {
            let target_id = target_msg["from"]["id"].as_i64().unwrap_or(0);
            let target_is_bot = target_msg["from"]["is_bot"].as_bool().unwrap_or(false);
            if target_id != 0 && !target_is_bot {
                let target_name = target_msg["from"]["first_name"].as_str().unwrap_or("کاربر");

                if is_lock {
                    let minutes: Option<i64> = text_raw
                        .split_whitespace()
                        .last()
                        .and_then(|s| s.parse::<i64>().ok());
                    let until = minutes.map(|m| chrono_like_now() + m * 60);
                    let _ = db::set_user_restriction(chat_id, target_id, lock_type, until);
                    let label = lock_type_label(lock_type);
                    let msg = match minutes {
                        Some(m) => format!("{} has been restricted from sending {} for {} minutes.", target_name, label, m),
                        None => format!("{} has been restricted from sending {}.", target_name, label),
                    };
                    let _ = tg.reply_message(chat_id, user_message_id, &msg).await;
                } else {
                    let _ = db::clear_user_restriction(chat_id, target_id, lock_type);
                    let label = lock_type_label(lock_type);
                    let msg = format!("{} can send {} again.", target_name, label);
                    let _ = tg.reply_message(chat_id, user_message_id, &msg).await;
                }
                return;
            }
        } else {
            let _ = db::set_group_lock(chat_id, lock_type, is_lock);
            let label = lock_type_label(lock_type);
            let msg = if is_lock {
                format!("{} are now locked for everyone in this group.", label)
            } else {
                format!("{} are now unlocked for everyone in this group.", label)
            };
            let _ = tg.reply_message(chat_id, user_message_id, &msg).await;
            return;
        }
    }

    // ---------- بررسی محتوای ممنوعه ----------
    if let Some(_violated) = protection::should_delete(chat_id, user_id, message) {
        let _ = tg.delete_message(chat_id, user_message_id).await;
        return;
    }

    // ---------- "بات" -> "بله" ----------
    if protection::is_bare_bot_word(text_raw) {
        let _ = tg.reply_message(chat_id, user_message_id, "بله").await;
        return;
    }

    // ---------- /ban و /unban ----------
    let trimmed_text = text_raw.trim();

    if trimmed_text.starts_with("/ban") {
        let sender_rank = db::get_rank(chat_id, user_id).unwrap_or(db::Rank::Regular);
        if !matches!(sender_rank, db::Rank::FirstOwner | db::Rank::Owner | db::Rank::Admin) {
            return;
        }

        if let Some(target_msg) = reply_target {
            let target_id = target_msg["from"]["id"].as_i64().unwrap_or(0);
            let target_is_bot = target_msg["from"]["is_bot"].as_bool().unwrap_or(false);

            if target_id == 0 || target_is_bot {
                return;
            }

            let target_rank = db::get_rank(chat_id, target_id).unwrap_or(db::Rank::Regular);
            if matches!(target_rank, db::Rank::FirstOwner | db::Rank::Owner) {
                let _ = tg.reply_message(
                    chat_id, user_message_id,
                    "This user is an owner and cannot be banned."
                ).await;
                return;
            }

            if target_rank == db::Rank::Vip && !matches!(sender_rank, db::Rank::FirstOwner | db::Rank::Owner) {
                let _ = tg.reply_message(
                    chat_id, user_message_id,
                    "Only owners can ban a VIP member."
                ).await;
                return;
            }

            let url = format!("https://api.telegram.org/bot{}/banChatMember", tg.token);
            let body = json!({ "chat_id": chat_id, "user_id": target_id });

            match tg.http.post(&url).json(&body).send().await {
                Ok(resp) => {
                    let v: Value = resp.json().await.unwrap_or(Value::Null);
                    if v["ok"].as_bool() == Some(true) {
                        let name = target_msg["from"]["first_name"].as_str().unwrap_or("User");
                        let _ = tg.reply_message(
                            chat_id, user_message_id,
                            &format!("{} has been banned from this group.", name)
                        ).await;
                    } else {
                        let desc = v["description"].as_str().unwrap_or("Unknown error");
                        let _ = tg.reply_message(
                            chat_id, user_message_id,
                            &format!("Failed to ban: {}", desc)
                        ).await;
                    }
                }
                Err(e) => eprintln!("ban error: {}", e),
            }
        }
        return;
    }

    if trimmed_text.starts_with("/unban") {
        let sender_rank = db::get_rank(chat_id, user_id).unwrap_or(db::Rank::Regular);
        if !matches!(sender_rank, db::Rank::FirstOwner | db::Rank::Owner | db::Rank::Admin) {
            return;
        }

        let parts: Vec<&str> = trimmed_text.split_whitespace().collect();
        if parts.len() != 2 {
            return;
        }

        let target_id: i64 = match parts[1].parse() {
            Ok(id) => id,
            Err(_) => return,
        };

        let url = format!("https://api.telegram.org/bot{}/unbanChatMember", tg.token);
        let body = json!({ "chat_id": chat_id, "user_id": target_id, "only_if_banned": true });

        match tg.http.post(&url).json(&body).send().await {
            Ok(resp) => {
                let v: Value = resp.json().await.unwrap_or(Value::Null);
                if v["ok"].as_bool() == Some(true) {
                    let _ = tg.reply_message(
                        chat_id, user_message_id,
                        &format!("User {} has been unbanned.", target_id)
                    ).await;
                }
            }
            Err(e) => eprintln!("unban error: {}", e),
        }
        return;
    }

    // ---------- /model در گروه ----------
    if text_raw.trim() == "/model" {
        let model_menu = format!(
            "🎯 *انتخاب مدل زبانی*\n\nلطفاً مدل مورد نظرت رو انتخاب کن:\n\n{}\n\n{}\n\n{}",
            MODEL_DESC_GEMINI, MODEL_DESC_DEEPSEEK, MODEL_DESC_GROQ
        );

        let buttons = vec![
            (
                "🟢 Gemini 3.5 Flash Lite".to_string(),
                format!("model:gemini:{}", user_id),
                "success".to_string(),
            ),
            (
                "🔵 DeepSeek V4 Flash".to_string(),
                format!("model:deepseek:{}", user_id),
                "primary".to_string(),
            ),
            (
                "🔴 GPT-OSS-120B".to_string(),
                format!("model:groq:{}", user_id),
                "danger".to_string(),
            ),
        ];

        let _ = tg
            .send_inline_model_keyboard(chat_id, user_message_id, &model_menu, &buttons)
            .await;
        return;
    }

    if !is_directed_at_bot(message) {
        return;
    }

    let clean_text = text_raw.replace(bot_username(), "").trim().to_string();
    if clean_text.is_empty() {
        return;
    }

    let username = message["from"]["username"].as_str();
    let group_title = message["chat"]["title"].as_str().unwrap_or("گروه");

    match antispam::check_and_register(user_id) {
        antispam::CheckResult::Allowed => {}
        antispam::CheckResult::StillProcessing => {
            let _ = tg
                .send_reply_with_emojis(chat_id, user_message_id, &[EmojiText::plain("⏳ صبر کن تا جواب قبلی بیاد.")])
                .await;
            return;
        }
        antispam::CheckResult::TooSoon => {
            let _ = tg
                .send_reply_with_emojis(chat_id, user_message_id, &[EmojiText::plain("⏱ هر ۵ ثانیه فقط یک درخواست.")])
                .await;
            return;
        }
        antispam::CheckResult::SpamDetected => {
            let mins = mute_for_spam(tg, user_id, username, group_title, antispam::MUTE_DURATION_SECS).await;
            let msg = format!(
                "🚫 کاربر عزیز، به‌خاطر ارسال پیام‌های پی‌درپی، به مدت {} دقیقه از استفاده از ربات محروم شدی.\n\nلطفاً پس از پایان این مدت دوباره تلاش کن. 🙏",
                mins
            );
            let _ = tg
                .send_reply_with_emojis(chat_id, user_message_id, &[EmojiText::plain(msg)])
                .await;
            return;
        }
    }

    let _ = db::ensure_group_memory(user_id);
    let _ = db::add_group_message(user_id, "user", &clean_text);
    let new_user_msg_id = db::last_group_message_id(user_id).unwrap_or(0);

    let _ = tg.send_typing(chat_id).await;

    let placeholder_parts = vec![
        EmojiText::plain("در حال تفکر "),
        EmojiText::with_emoji(EMOJI_THINKING, ""),
    ];
    let placeholder = tg
        .send_reply_with_emojis(chat_id, user_message_id, &placeholder_parts)
        .await;
    let placeholder_message_id = match placeholder {
        Ok(v) => v["result"]["message_id"].as_i64().unwrap_or(0),
        Err(_) => 0,
    };

    if placeholder_message_id == 0 {
        antispam::mark_completed(user_id);
        return;
    }

    let user_model = db::get_user_group_model(user_id).unwrap_or(None);
    let group_model = db::get_group_model(DEFAULT_GROUP_MODEL).unwrap_or_else(|_| DEFAULT_GROUP_MODEL.to_string());
    let requested = user_model.unwrap_or(group_model);
    let model = if db::is_model_shown(&requested) {
        requested
    } else {
        pick_alternative_model(&requested).to_string()
    };

    let system_prompt = if db::is_system_prompt_enabled().unwrap_or(false) {
        let p = db::get_system_prompt().unwrap_or_default();
        if p.is_empty() { None } else { Some(p) }
    } else {
        None
    };

    let history_with_ids = db::get_group_history_with_ids(user_id).unwrap_or_default();
    let mut combined_history_ids = history_with_ids;
    combined_history_ids.push(("user".to_string(), clean_text.clone(), new_user_msg_id));

    let model_for_closure = model.clone();
    let system_prompt_for_closure = system_prompt.clone();
    let result = ai::run_with_auto_forget(user_id, true, combined_history_ids, |plain_history| {
        let model = model_for_closure.clone();
        let system_prompt = system_prompt_for_closure.clone();
        async move {
            if is_deepseek_model(&model) {
                let hist: Vec<(String, String)> = plain_history
                    .iter()
                    .map(|(r, c)| {
                        let role = if r == "assistant" { "model".to_string() } else { "user".to_string() };
                        (role, c.clone())
                    })
                    .collect();
                ai::deepseek::stream(
                    http, tg, orca_api_key, system_prompt.as_deref(), &hist, chat_id, placeholder_message_id,
                )
                .await
            } else if is_groq_model(&model) {
                let hist: Vec<(String, String)> = plain_history
                    .iter()
                    .map(|(r, c)| {
                        let role = if r == "assistant" { "model".to_string() } else { "user".to_string() };
                        (role, c.clone())
                    })
                    .collect();
                ai::groq::stream(
                    http, tg, groq_api_key, system_prompt.as_deref(), &hist, chat_id, placeholder_message_id,
                )
                .await
            } else {
                let hist: Vec<ai::HistoryItem> = plain_history
                    .iter()
                    .map(|(r, c)| ai::HistoryItem { role: r.clone(), content: c.clone() })
                    .collect();
                ai::gemini::stream_fixed(
                    http, tg, gemini_api_key, &model, system_prompt.as_deref(), &hist, chat_id, placeholder_message_id,
                )
                .await
            }
        }
    })
    .await;

    antispam::mark_completed(user_id);

    match result {
        Ok(sr) => {
            let _ = db::add_group_message(user_id, "assistant", &sr.full_text);
            ai::polish_and_send_rich(
                http,
                tg,
                gemini_api_key,
                chat_id,
                placeholder_message_id,
                &sr.full_text,
            )
            .await;
        }
        Err(e) => {
            eprintln!("خطا گروه: {}", e);
            let msg = if user_id == owner_id() {
                format!("🔴 *خطای واقعی (فقط مالک)*\n\nمدل: {}\n\n{}", model, e)
            } else {
                "⚠️ یه مشکل فنی پیش اومد و نتونستم جوابت رو بدم.\n\nلطفاً چند لحظه بعد دوباره تلاش کن. 🙏".to_string()
            };
            let _ = tg.edit_message(chat_id, placeholder_message_id, &msg).await;
        }
    }
}

fn lock_type_label(lock_type: &str) -> &'static str {
    protection::LOCK_TYPES
        .iter()
        .find(|(main, _, _)| *main == lock_type)
        .map(|(_, _, label)| *label)
        .unwrap_or("that content type")
}

fn chrono_like_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ============ کمکی ============

fn truncate_chars(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let truncated: String = chars[..max].iter().collect();
        format!("{}...", truncated)
    }
}

async fn send_long_message(tg: &TelegramClient, chat_id: i64, text: &str) {
    const LIMIT: usize = 3800;
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= LIMIT {
        let _ = tg.send_message(chat_id, text).await;
        return;
    }
    let mut start = 0;
    while start < chars.len() {
        let end = (start + LIMIT).min(chars.len());
        let chunk: String = chars[start..end].iter().collect();
        let _ = tg.send_message(chat_id, &chunk).await;
        start = end;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
}
