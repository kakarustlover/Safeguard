use rusqlite::{params, Connection, Result};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Chat {
    pub id: i64,
    pub title: String,
    pub model: Option<String>,
}

pub struct Message {
    pub role: String,
    pub content: String,
}

pub struct GroupInfo {
    pub chat_id: i64,
    pub title: String,
    pub username: Option<String>,
    pub join_link: Option<String>,
}

pub struct UserInfo {
    pub user_id: i64,
    pub username: Option<String>,
    pub joined_at: i64,
    pub message_count: i64,
}

pub struct GroupMember {
    pub user_id: i64,
    pub username: Option<String>,
    pub last_seen_at: i64,
}

pub struct FullMessage {
    pub role: String,
    pub content: String,
    pub created_at: i64,
    pub source: String,
}

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

static DB_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// باید یک‌بار در ابتدای main() صدا زده شود تا مسیر فایل دیتابیس (مثلاً از env یا Volume Railway) تنظیم شود.
pub fn set_db_path(path: String) {
    let _ = DB_PATH.set(path);
}

fn db_file_path() -> &'static str {
    DB_PATH.get_or_init(|| "bot.db".to_string())
}

pub fn get_conn() -> Result<Connection> {
    let conn = Connection::open(db_file_path())?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    Ok(conn)
}

pub fn init_db(owner_id: i64, default_group_model: &str) -> Result<()> {
    let conn = get_conn()?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (
            user_id INTEGER PRIMARY KEY,
            username TEXT,
            is_banned INTEGER DEFAULT 0,
            joined_at INTEGER NOT NULL
        )",
        [],
    )?;
    let _ = conn.execute("ALTER TABLE users ADD COLUMN username TEXT", []);

    conn.execute(
        "CREATE TABLE IF NOT EXISTS admins (
            user_id INTEGER PRIMARY KEY
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS chats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            telegram_chat_id INTEGER NOT NULL,
            title TEXT NOT NULL,
            model TEXT,
            created_at INTEGER NOT NULL,
            is_active INTEGER DEFAULT 0
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS group_memory (
            user_id INTEGER PRIMARY KEY,
            created_at INTEGER NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS group_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (user_id) REFERENCES group_memory(user_id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            chat_id INTEGER NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (chat_id) REFERENCES chats(id)
        )",
        [],
    )?;

    // پیام‌های فراموش‌شده حذف نمی‌شوند، فقط علامت می‌خورند (برای /see و بازبینی بعدی)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS forgotten_markers (
            scope_id INTEGER NOT NULL,
            is_group INTEGER NOT NULL,
            last_forgotten_id INTEGER NOT NULL,
            PRIMARY KEY (scope_id, is_group)
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS usage (
            user_id INTEGER PRIMARY KEY,
            message_count INTEGER DEFAULT 0
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS ui_state (
            telegram_chat_id INTEGER PRIMARY KEY,
            stage TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS bot_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS groups (
            telegram_chat_id INTEGER PRIMARY KEY,
            title TEXT,
            last_seen_at INTEGER NOT NULL
        )",
        [],
    )?;
    let _ = conn.execute("ALTER TABLE groups ADD COLUMN username TEXT", []);
    let _ = conn.execute("ALTER TABLE groups ADD COLUMN join_link TEXT", []);

    conn.execute(
        "CREATE TABLE IF NOT EXISTS group_members (
            group_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            username TEXT,
            last_seen_at INTEGER NOT NULL,
            PRIMARY KEY (group_id, user_id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS user_group_models (
            user_id INTEGER PRIMARY KEY,
            model TEXT NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS mutes (
            user_id INTEGER PRIMARY KEY,
            until INTEGER NOT NULL,
            reason TEXT,
            created_at INTEGER NOT NULL
        )",
        [],
    )?;

    // ---------- Protection: rank system (per group) ----------
    conn.execute(
        "CREATE TABLE IF NOT EXISTS group_owners (
            group_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            is_first INTEGER NOT NULL DEFAULT 0,
            added_at INTEGER NOT NULL,
            PRIMARY KEY (group_id, user_id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS group_admins (
            group_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            added_at INTEGER NOT NULL,
            PRIMARY KEY (group_id, user_id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS group_vips (
            group_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            added_at INTEGER NOT NULL,
            PRIMARY KEY (group_id, user_id)
        )",
        [],
    )?;

    // ---------- Protection: group-wide locks ----------
    conn.execute(
        "CREATE TABLE IF NOT EXISTS group_locks (
            group_id INTEGER NOT NULL,
            lock_type TEXT NOT NULL,
            PRIMARY KEY (group_id, lock_type)
        )",
        [],
    )?;

    // ---------- Protection: per-user restrictions (optionally timed) ----------
    conn.execute(
        "CREATE TABLE IF NOT EXISTS user_restrictions (
            group_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            lock_type TEXT NOT NULL,
            until INTEGER,
            created_at INTEGER NOT NULL,
            PRIMARY KEY (group_id, user_id, lock_type)
        )",
        [],
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO bot_settings (key, value) VALUES ('bot_enabled', '1')",
        [],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO bot_settings (key, value) VALUES ('group_model', ?1)",
        params![default_group_model],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO bot_settings (key, value) VALUES ('system_prompt', '')",
        [],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO bot_settings (key, value) VALUES ('system_prompt_enabled', '0')",
        [],
    )?;
    // نمایش/عدم نمایش مدل‌ها (پیش‌فرض: همه روشن)
    conn.execute(
        "INSERT OR IGNORE INTO bot_settings (key, value) VALUES ('show_gemini', '1')",
        [],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO bot_settings (key, value) VALUES ('show_deepseek', '1')",
        [],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO bot_settings (key, value) VALUES ('show_groq', '1')",
        [],
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO admins (user_id) VALUES (?1)",
        params![owner_id],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO users (user_id, is_banned, joined_at) VALUES (?1, 0, ?2)",
        params![owner_id, now_ts()],
    )?;

    Ok(())
}

// ---------- کاربران ----------

pub fn ensure_user(user_id: i64, username: Option<&str>) -> Result<bool> {
    let conn = get_conn()?;
    let existed: bool = {
        let mut stmt = conn.prepare("SELECT 1 FROM users WHERE user_id = ?1")?;
        stmt.exists(params![user_id])?
    };

    if !existed {
        conn.execute(
            "INSERT INTO users (user_id, username, is_banned, joined_at) VALUES (?1, ?2, 0, ?3)",
            params![user_id, username, now_ts()],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO usage (user_id, message_count) VALUES (?1, 0)",
            params![user_id],
        )?;
    } else {
        conn.execute(
            "UPDATE users SET username = ?2 WHERE user_id = ?1",
            params![user_id, username],
        )?;
    }

    Ok(!existed)
}

pub fn is_banned(user_id: i64) -> Result<bool> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare("SELECT is_banned FROM users WHERE user_id = ?1")?;
    let banned: Option<i64> = stmt.query_row(params![user_id], |row| row.get(0)).ok();
    Ok(banned.unwrap_or(0) == 1)
}

pub fn set_banned(user_id: i64, banned: bool) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "UPDATE users SET is_banned = ?1 WHERE user_id = ?2",
        params![banned as i64, user_id],
    )?;
    Ok(())
}

pub fn total_users() -> Result<i64> {
    let conn = get_conn()?;
    conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
}

pub fn find_user_id_by_username(username: &str) -> Result<Option<i64>> {
    let clean = username.trim_start_matches('@');
    let conn = get_conn()?;
    let mut stmt = conn.prepare("SELECT user_id FROM users WHERE username = ?1 COLLATE NOCASE")?;
    let id: Option<i64> = stmt.query_row(params![clean], |row| row.get(0)).ok();
    Ok(id)
}

pub fn get_user_info(user_id: i64) -> Result<Option<UserInfo>> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare(
        "SELECT u.user_id, u.username, u.joined_at, COALESCE(us.message_count, 0)
         FROM users u LEFT JOIN usage us ON u.user_id = us.user_id
         WHERE u.user_id = ?1",
    )?;
    let info: Option<UserInfo> = stmt
        .query_row(params![user_id], |row| {
            Ok(UserInfo {
                user_id: row.get(0)?,
                username: row.get(1)?,
                joined_at: row.get(2)?,
                message_count: row.get(3)?,
            })
        })
        .ok();
    Ok(info)
}

// ---------- ادمین‌ها ----------

pub fn is_admin(user_id: i64) -> Result<bool> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare("SELECT 1 FROM admins WHERE user_id = ?1")?;
    Ok(stmt.exists(params![user_id])?)
}

pub fn add_admin(user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute("INSERT OR IGNORE INTO admins (user_id) VALUES (?1)", params![user_id])?;
    Ok(())
}

pub fn remove_admin(user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute("DELETE FROM admins WHERE user_id = ?1", params![user_id])?;
    Ok(())
}

// ---------- چت‌ها ----------

pub fn create_chat(telegram_chat_id: i64, title: &str) -> Result<i64> {
    let conn = get_conn()?;
    conn.execute(
        "UPDATE chats SET is_active = 0 WHERE telegram_chat_id = ?1",
        params![telegram_chat_id],
    )?;
    conn.execute(
        "INSERT INTO chats (telegram_chat_id, title, model, created_at, is_active)
         VALUES (?1, ?2, NULL, ?3, 1)",
        params![telegram_chat_id, title, now_ts()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_active_chat(telegram_chat_id: i64) -> Result<Option<Chat>> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare(
        "SELECT id, title, model FROM chats WHERE telegram_chat_id = ?1 AND is_active = 1",
    )?;
    let mut rows = stmt.query(params![telegram_chat_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(Chat {
            id: row.get(0)?,
            title: row.get(1)?,
            model: row.get(2)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn set_chat_model(chat_id: i64, model: &str) -> Result<()> {
    let conn = get_conn()?;
    conn.execute("UPDATE chats SET model = ?1 WHERE id = ?2", params![model, chat_id])?;
    Ok(())
}

pub fn set_chat_title(chat_id: i64, title: &str) -> Result<()> {
    let conn = get_conn()?;
    conn.execute("UPDATE chats SET title = ?1 WHERE id = ?2", params![title, chat_id])?;
    Ok(())
}

// ---------- پیام‌ها ----------

pub fn add_message(chat_id: i64, role: &str, content: &str) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT INTO messages (chat_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![chat_id, role, content, now_ts()],
    )?;
    Ok(())
}

/// id آخرین پیام ثبت‌شده در این چت را برمی‌گرداند (برای استفاده در فراموشی خودکار).
pub fn last_message_id(chat_id: i64) -> Result<i64> {
    let conn = get_conn()?;
    let id: i64 = conn.query_row(
        "SELECT id FROM messages WHERE chat_id = ?1 ORDER BY id DESC LIMIT 1",
        params![chat_id],
        |row| row.get(0),
    )?;
    Ok(id)
}

pub fn get_history(chat_id: i64) -> Result<Vec<Message>> {
    let conn = get_conn()?;
    let cutoff = get_forgotten_cutoff(chat_id, false)?;
    let mut stmt = conn.prepare(
        "SELECT role, content FROM messages WHERE chat_id = ?1 AND id > ?2 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![chat_id, cutoff], |row| {
        Ok(Message { role: row.get(0)?, content: row.get(1)? })
    })?;
    rows.collect()
}

/// مثل get_history ولی id هر ردیف را هم برمی‌گرداند (لازم برای فراموشی خودکار).
pub fn get_history_with_ids(chat_id: i64) -> Result<Vec<(String, String, i64)>> {
    let conn = get_conn()?;
    let cutoff = get_forgotten_cutoff(chat_id, false)?;
    let mut stmt = conn.prepare(
        "SELECT role, content, id FROM messages WHERE chat_id = ?1 AND id > ?2 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![chat_id, cutoff], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
    })?;
    rows.collect()
}

// ---------- حافظه گروهی ----------

pub fn ensure_group_memory(user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT OR IGNORE INTO group_memory (user_id, created_at) VALUES (?1, ?2)",
        params![user_id, now_ts()],
    )?;
    Ok(())
}

pub fn add_group_message(user_id: i64, role: &str, content: &str) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT INTO group_messages (user_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![user_id, role, content, now_ts()],
    )?;
    Ok(())
}

/// id آخرین پیام گروهی ثبت‌شده برای این کاربر را برمی‌گرداند.
pub fn last_group_message_id(user_id: i64) -> Result<i64> {
    let conn = get_conn()?;
    let id: i64 = conn.query_row(
        "SELECT id FROM group_messages WHERE user_id = ?1 ORDER BY id DESC LIMIT 1",
        params![user_id],
        |row| row.get(0),
    )?;
    Ok(id)
}

pub fn get_group_history(user_id: i64) -> Result<Vec<Message>> {
    let conn = get_conn()?;
    let cutoff = get_forgotten_cutoff(user_id, true)?;
    let mut stmt = conn.prepare(
        "SELECT role, content FROM group_messages WHERE user_id = ?1 AND id > ?2 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![user_id, cutoff], |row| {
        Ok(Message { role: row.get(0)?, content: row.get(1)? })
    })?;
    rows.collect()
}

/// مثل get_group_history ولی id هر ردیف را هم برمی‌گرداند.
pub fn get_group_history_with_ids(user_id: i64) -> Result<Vec<(String, String, i64)>> {
    let conn = get_conn()?;
    let cutoff = get_forgotten_cutoff(user_id, true)?;
    let mut stmt = conn.prepare(
        "SELECT role, content, id FROM group_messages WHERE user_id = ?1 AND id > ?2 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![user_id, cutoff], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
    })?;
    rows.collect()
}

// ---------- فراموشی خودکار حافظه ----------

fn get_forgotten_cutoff(scope_id: i64, is_group: bool) -> Result<i64> {
    let conn = get_conn()?;
    let is_group_i = if is_group { 1 } else { 0 };
    let val: Option<i64> = conn
        .query_row(
            "SELECT last_forgotten_id FROM forgotten_markers WHERE scope_id = ?1 AND is_group = ?2",
            params![scope_id, is_group_i],
            |row| row.get(0),
        )
        .ok();
    Ok(val.unwrap_or(0))
}

/// همه پیام‌ها تا این id (شامل خودش) را "فراموش‌شده" علامت می‌زند (بدون حذف واقعی از دیتابیس).
pub fn mark_forgotten_before(scope_id: i64, is_group: bool, last_forgotten_id: i64) -> Result<()> {
    let conn = get_conn()?;
    let is_group_i = if is_group { 1 } else { 0 };
    conn.execute(
        "INSERT INTO forgotten_markers (scope_id, is_group, last_forgotten_id) VALUES (?1, ?2, ?3)
         ON CONFLICT(scope_id, is_group) DO UPDATE SET last_forgotten_id = MAX(last_forgotten_id, excluded.last_forgotten_id)",
        params![scope_id, is_group_i, last_forgotten_id],
    )?;
    Ok(())
}

// ---------- مصرف ----------

pub fn increment_usage(user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT INTO usage (user_id, message_count) VALUES (?1, 1)
         ON CONFLICT(user_id) DO UPDATE SET message_count = message_count + 1",
        params![user_id],
    )?;
    Ok(())
}

pub fn get_usage(user_id: i64) -> Result<i64> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare("SELECT message_count FROM usage WHERE user_id = ?1")?;
    let count: Option<i64> = stmt.query_row(params![user_id], |row| row.get(0)).ok();
    Ok(count.unwrap_or(0))
}

// ---------- UI State ----------

pub fn get_stage(telegram_chat_id: i64) -> Result<String> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare("SELECT stage FROM ui_state WHERE telegram_chat_id = ?1")?;
    let stage: Option<String> = stmt.query_row(params![telegram_chat_id], |row| row.get(0)).ok();
    Ok(stage.unwrap_or_else(|| "root".to_string()))
}

pub fn set_stage(telegram_chat_id: i64, stage: &str) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT INTO ui_state (telegram_chat_id, stage, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(telegram_chat_id) DO UPDATE SET stage = ?2, updated_at = ?3",
        params![telegram_chat_id, stage, now_ts()],
    )?;
    Ok(())
}

// ---------- تنظیمات ----------

fn get_setting(key: &str) -> Result<Option<String>> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare("SELECT value FROM bot_settings WHERE key = ?1")?;
    let value: Option<String> = stmt.query_row(params![key], |row| row.get(0)).ok();
    Ok(value)
}

fn set_setting(key: &str, value: &str) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT INTO bot_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = ?2",
        params![key, value],
    )?;
    Ok(())
}

pub fn is_bot_enabled() -> Result<bool> {
    let value = get_setting("bot_enabled")?;
    Ok(value.map(|v| v == "1").unwrap_or(true))
}

pub fn set_bot_enabled(enabled: bool) -> Result<()> {
    set_setting("bot_enabled", if enabled { "1" } else { "0" })
}

pub fn get_group_model(fallback: &str) -> Result<String> {
    let value = get_setting("group_model")?;
    Ok(value.unwrap_or_else(|| fallback.to_string()))
}

pub fn set_group_model(model: &str) -> Result<()> {
    set_setting("group_model", model)
}

// ---------- پرامپت سیستمی ----------

pub fn get_system_prompt() -> Result<String> {
    let value = get_setting("system_prompt")?;
    Ok(value.unwrap_or_default())
}

pub fn set_system_prompt(prompt: &str) -> Result<()> {
    set_setting("system_prompt", prompt)
}

pub fn is_system_prompt_enabled() -> Result<bool> {
    let value = get_setting("system_prompt_enabled")?;
    Ok(value.map(|v| v == "1").unwrap_or(false))
}

pub fn set_system_prompt_enabled(enabled: bool) -> Result<()> {
    set_setting("system_prompt_enabled", if enabled { "1" } else { "0" })
}

// ---------- نمایش/عدم نمایش مدل‌ها ----------

pub fn is_model_shown(model: &str) -> bool {
    let key = if model == "gemini-3.5-flash-lite" {
        "show_gemini"
    } else if model.contains("deepseek") {
        "show_deepseek"
    } else if model.contains("groq") || model.contains("gpt-oss") {
        "show_groq"
    } else {
        return true;
    };

    match get_setting(key) {
        Ok(Some(v)) => v == "1",
        _ => true,
    }
}

pub fn set_model_shown(model: &str, shown: bool) -> Result<()> {
    let key = if model == "gemini-3.5-flash-lite" || model == "gemini" {
        "show_gemini"
    } else if model == "deepseek" {
        "show_deepseek"
    } else if model == "groq" || model == "gpt" {
        "show_groq"
    } else {
        return Ok(());
    };
    set_setting(key, if shown { "1" } else { "0" })
}

// ---------- مدل هر کاربر در گروه ----------

pub fn get_user_group_model(user_id: i64) -> Result<Option<String>> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare("SELECT model FROM user_group_models WHERE user_id = ?1")?;
    let model: Option<String> = stmt.query_row(params![user_id], |row| row.get(0)).ok();
    Ok(model)
}

pub fn set_user_group_model(user_id: i64, model: &str) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT INTO user_group_models (user_id, model) VALUES (?1, ?2)
         ON CONFLICT(user_id) DO UPDATE SET model = ?2",
        params![user_id, model],
    )?;
    Ok(())
}

// ---------- سکوت (Mute) ----------

pub fn add_mute(user_id: i64, until: i64, reason: &str) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT INTO mutes (user_id, until, reason, created_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_id) DO UPDATE SET until = ?2, reason = ?3, created_at = ?4",
        params![user_id, until, reason, now_ts()],
    )?;
    Ok(())
}

pub fn remove_mute(user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute("DELETE FROM mutes WHERE user_id = ?1", params![user_id])?;
    Ok(())
}

pub fn is_muted(user_id: i64) -> Result<Option<i64>> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare("SELECT until FROM mutes WHERE user_id = ?1")?;
    let until: Option<i64> = stmt.query_row(params![user_id], |row| row.get(0)).ok();
    if let Some(u) = until {
        if u > now_ts() {
            return Ok(Some(u));
        } else {
            let _ = remove_mute(user_id);
            return Ok(None);
        }
    }
    Ok(None)
}

// ---------- گروه‌ها ----------

pub fn touch_group_full(
    telegram_chat_id: i64,
    title: &str,
    username: Option<&str>,
    join_link: Option<&str>,
) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT INTO groups (telegram_chat_id, title, last_seen_at, username, join_link)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(telegram_chat_id) DO UPDATE SET
            title = ?2,
            last_seen_at = ?3,
            username = COALESCE(?4, username),
            join_link = COALESCE(?5, join_link)",
        params![telegram_chat_id, title, now_ts(), username, join_link],
    )?;
    Ok(())
}

pub fn list_groups_full() -> Result<Vec<GroupInfo>> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare(
        "SELECT telegram_chat_id, title, username, join_link
         FROM groups ORDER BY last_seen_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(GroupInfo {
            chat_id: row.get(0)?,
            title: row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "بدون عنوان".to_string()),
            username: row.get(2)?,
            join_link: row.get(3)?,
        })
    })?;
    rows.collect()
}

pub fn find_group_by_ref(input: &str) -> Result<Option<i64>> {
    let clean = input.trim().trim_start_matches('@');
    let conn = get_conn()?;

    if let Ok(id) = input.trim().parse::<i64>() {
        return Ok(Some(id));
    }

    let mut stmt = conn.prepare(
        "SELECT telegram_chat_id FROM groups
         WHERE username = ?1 COLLATE NOCASE OR title = ?1 COLLATE NOCASE
         LIMIT 1",
    )?;
    let id: Option<i64> = stmt.query_row(params![clean], |row| row.get(0)).ok();
    Ok(id)
}

// ---------- ممبرای گروه ----------

pub fn touch_group_member(
    group_id: i64,
    user_id: i64,
    username: Option<&str>,
) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT INTO group_members (group_id, user_id, username, last_seen_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(group_id, user_id) DO UPDATE SET
            username = COALESCE(?3, username),
            last_seen_at = ?4",
        params![group_id, user_id, username, now_ts()],
    )?;
    Ok(())
}

pub fn list_group_members(group_id: i64, limit: usize) -> Result<Vec<GroupMember>> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare(
        "SELECT user_id, username, last_seen_at FROM group_members
         WHERE group_id = ?1
         ORDER BY last_seen_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![group_id, limit as i64], |row| {
        Ok(GroupMember {
            user_id: row.get(0)?,
            username: row.get(1)?,
            last_seen_at: row.get(2)?,
        })
    })?;
    rows.collect()
}

pub fn count_group_members(group_id: i64) -> Result<i64> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM group_members WHERE group_id = ?1")?;
    let count: i64 = stmt.query_row(params![group_id], |row| row.get(0))?;
    Ok(count)
}

// ---------- تاریخچه کامل یک کاربر ----------

pub fn get_user_all_messages(user_id: i64, limit: usize, offset: usize) -> Result<Vec<FullMessage>> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare(
        "SELECT role, content, created_at, 'private' AS source FROM messages
         WHERE chat_id IN (SELECT id FROM chats WHERE telegram_chat_id = ?1)
         UNION ALL
         SELECT role, content, created_at, 'group' AS source FROM group_messages
         WHERE user_id = ?1
         ORDER BY created_at ASC
         LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt.query_map(params![user_id, limit as i64, offset as i64], |row| {
        Ok(FullMessage {
            role: row.get(0)?,
            content: row.get(1)?,
            created_at: row.get(2)?,
            source: row.get(3)?,
        })
    })?;
    rows.collect()
}

pub fn count_user_all_messages(user_id: i64) -> Result<i64> {
    let conn = get_conn()?;
    let count: i64 = conn.query_row(
        "SELECT
            (SELECT COUNT(*) FROM messages WHERE chat_id IN
                (SELECT id FROM chats WHERE telegram_chat_id = ?1))
            +
            (SELECT COUNT(*) FROM group_messages WHERE user_id = ?1)",
        params![user_id],
        |row| row.get(0),
    )?;
    Ok(count)
}

pub fn list_private_users() -> Result<Vec<(i64, Option<String>)>> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare("SELECT user_id, username FROM users ORDER BY joined_at ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    rows.collect()
}

pub fn list_group_users() -> Result<Vec<i64>> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare("SELECT user_id FROM group_memory ORDER BY created_at ASC")?;
    let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    rows.collect()
}

// ================= Protection: Rank System =================

/// نقش یک کاربر در گروه
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rank {
    FirstOwner,
    Owner,
    Admin,
    Vip,
    Regular,
}

/// اولین مالک گروه را ثبت می‌کند (وقتی ربات به گروه اضافه می‌شود). اگر از قبل یک first_owner
/// برای این گروه ثبت شده باشد، کاری انجام نمی‌دهد (idempotent).
pub fn ensure_first_owner(group_id: i64, user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    let exists: bool = {
        let mut stmt = conn.prepare(
            "SELECT 1 FROM group_owners WHERE group_id = ?1 AND is_first = 1",
        )?;
        stmt.exists(params![group_id])?
    };
    if !exists {
        conn.execute(
            "INSERT OR IGNORE INTO group_owners (group_id, user_id, is_first, added_at) VALUES (?1, ?2, 1, ?3)",
            params![group_id, user_id, now_ts()],
        )?;
    }
    Ok(())
}

pub fn is_first_owner(group_id: i64, user_id: i64) -> Result<bool> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare(
        "SELECT 1 FROM group_owners WHERE group_id = ?1 AND user_id = ?2 AND is_first = 1",
    )?;
    stmt.exists(params![group_id, user_id])
}

pub fn is_owner(group_id: i64, user_id: i64) -> Result<bool> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare(
        "SELECT 1 FROM group_owners WHERE group_id = ?1 AND user_id = ?2",
    )?;
    stmt.exists(params![group_id, user_id])
}

pub fn is_group_admin(group_id: i64, user_id: i64) -> Result<bool> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare(
        "SELECT 1 FROM group_admins WHERE group_id = ?1 AND user_id = ?2",
    )?;
    stmt.exists(params![group_id, user_id])
}

pub fn is_vip(group_id: i64, user_id: i64) -> Result<bool> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare(
        "SELECT 1 FROM group_vips WHERE group_id = ?1 AND user_id = ?2",
    )?;
    stmt.exists(params![group_id, user_id])
}

/// رتبه فعلی یک کاربر در گروه را برمی‌گرداند (بالاترین رتبه‌ای که دارد)
pub fn get_rank(group_id: i64, user_id: i64) -> Result<Rank> {
    if is_first_owner(group_id, user_id)? {
        return Ok(Rank::FirstOwner);
    }
    if is_owner(group_id, user_id)? {
        return Ok(Rank::Owner);
    }
    if is_group_admin(group_id, user_id)? {
        return Ok(Rank::Admin);
    }
    if is_vip(group_id, user_id)? {
        return Ok(Rank::Vip);
    }
    Ok(Rank::Regular)
}

pub fn add_owner(group_id: i64, user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT OR IGNORE INTO group_owners (group_id, user_id, is_first, added_at) VALUES (?1, ?2, 0, ?3)",
        params![group_id, user_id, now_ts()],
    )?;
    Ok(())
}

/// مالک را کامل حذف می‌کند (اولین مالک هرگز نباید با این تابع حذف شود؛ فراخوان باید قبلش چک کند)
pub fn remove_owner(group_id: i64, user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "DELETE FROM group_owners WHERE group_id = ?1 AND user_id = ?2 AND is_first = 0",
        params![group_id, user_id],
    )?;
    Ok(())
}

pub fn add_admin(group_id: i64, user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT OR IGNORE INTO group_admins (group_id, user_id, added_at) VALUES (?1, ?2, ?3)",
        params![group_id, user_id, now_ts()],
    )?;
    Ok(())
}

pub fn remove_admin(group_id: i64, user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "DELETE FROM group_admins WHERE group_id = ?1 AND user_id = ?2",
        params![group_id, user_id],
    )?;
    Ok(())
}

pub fn add_vip(group_id: i64, user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT OR IGNORE INTO group_vips (group_id, user_id, added_at) VALUES (?1, ?2, ?3)",
        params![group_id, user_id, now_ts()],
    )?;
    Ok(())
}

pub fn remove_vip(group_id: i64, user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "DELETE FROM group_vips WHERE group_id = ?1 AND user_id = ?2",
        params![group_id, user_id],
    )?;
    Ok(())
}

/// یک کاربر را کامل به "عادی" برمی‌گرداند: هم از ادمین هم از مالک (اگر مالک اول نباشد) خارج می‌کند
pub fn demote_to_regular(group_id: i64, user_id: i64) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "DELETE FROM group_admins WHERE group_id = ?1 AND user_id = ?2",
        params![group_id, user_id],
    )?;
    conn.execute(
        "DELETE FROM group_owners WHERE group_id = ?1 AND user_id = ?2 AND is_first = 0",
        params![group_id, user_id],
    )?;
    Ok(())
}

// ================= Protection: Locks =================

pub fn set_group_lock(group_id: i64, lock_type: &str, enabled: bool) -> Result<()> {
    let conn = get_conn()?;
    if enabled {
        conn.execute(
            "INSERT OR IGNORE INTO group_locks (group_id, lock_type) VALUES (?1, ?2)",
            params![group_id, lock_type],
        )?;
    } else {
        conn.execute(
            "DELETE FROM group_locks WHERE group_id = ?1 AND lock_type = ?2",
            params![group_id, lock_type],
        )?;
    }
    Ok(())
}

pub fn is_group_lock_enabled(group_id: i64, lock_type: &str) -> Result<bool> {
    let conn = get_conn()?;
    let mut stmt = conn.prepare(
        "SELECT 1 FROM group_locks WHERE group_id = ?1 AND lock_type = ?2",
    )?;
    stmt.exists(params![group_id, lock_type])
}

/// یک محدودیت فردی ثبت می‌کند. until = None یعنی دائمی.
pub fn set_user_restriction(
    group_id: i64,
    user_id: i64,
    lock_type: &str,
    until: Option<i64>,
) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "INSERT OR REPLACE INTO user_restrictions (group_id, user_id, lock_type, until, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![group_id, user_id, lock_type, until, now_ts()],
    )?;
    Ok(())
}

pub fn clear_user_restriction(group_id: i64, user_id: i64, lock_type: &str) -> Result<()> {
    let conn = get_conn()?;
    conn.execute(
        "DELETE FROM user_restrictions WHERE group_id = ?1 AND user_id = ?2 AND lock_type = ?3",
        params![group_id, user_id, lock_type],
    )?;
    Ok(())
}

/// چک می‌کند که آیا کاربر الان برای این نوع قفل محدود است (چه با قانون کلی گروه چه با محدودیت فردی).
/// محدودیت‌های منقضی‌شده به‌طور خودکار نادیده گرفته می‌شوند (پاک‌سازی lazy).
pub fn is_user_restricted(group_id: i64, user_id: i64, lock_type: &str) -> Result<bool> {
    // VIP از همه قفل‌ها معاف است
    if is_vip(group_id, user_id)? {
        return Ok(false);
    }

    // قانون کلی گروه
    if is_group_lock_enabled(group_id, lock_type)? {
        return Ok(true);
    }

    // محدودیت فردی
    let conn = get_conn()?;
    let row: Option<(Option<i64>,)> = conn
        .query_row(
            "SELECT until FROM user_restrictions WHERE group_id = ?1 AND user_id = ?2 AND lock_type = ?3",
            params![group_id, user_id, lock_type],
            |row| Ok((row.get(0)?,)),
        )
        .ok();

    match row {
        None => Ok(false),
        Some((None,)) => Ok(true), // دائمی
        Some((Some(until),)) => {
            if until > now_ts() {
                Ok(true)
            } else {
                // منقضی شده، پاکش می‌کنیم
                let _ = clear_user_restriction(group_id, user_id, lock_type);
                Ok(false)
            }
        }
    }
}
