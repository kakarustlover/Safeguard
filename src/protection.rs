use crate::client::TelegramClient;
use crate::db;
use serde_json::Value;

/// نوع قفل‌هایی که روی گروه یا یک کاربر خاص قابل‌اعمال است.
/// هر ورودی: (کلمه اصلی، لیست مترادف/مخفف‌ها، برچسب نمایشی انگلیسی)
pub const LOCK_TYPES: &[(&str, &[&str], &str)] = &[
    ("link", &["lnk"], "links"),
    ("sticker", &["stk"], "stickers"),
    ("gif", &["gf"], "GIFs"),
    ("video", &["vid"], "videos"),
    ("photo", &["pho"], "photos"),
    ("voice", &["vc"], "voice messages"),
    ("audio", &["au"], "audio files"),
    ("file", &["fl"], "files"),
    ("location", &["loc"], "locations"),
    ("contact", &["ct"], "contacts"),
    ("forward", &["fwd"], "forwarded messages"),
    ("mention", &["mnt"], "mentions"),
    ("hashtag", &["hsh"], "hashtags"),
    ("emoji", &["emj"], "excess emoji"),
    ("profanity", &["prf"], "profanity"),
    ("bot", &["bt"], "other bots"),
    ("username", &["usr"], "username ads"),
    ("anon", &["an"], "anonymous admin messages"),
    ("game", &["gm"], "games"),
];

/// یک کلمه ورودی کاربر را (بعد از پایین‌آوردن حروف) به نوع قفل نگاشت می‌کند، اگه دقیقاً مطابقت داشت.
pub fn resolve_lock_type(word: &str) -> Option<&'static str> {
    let w = word.to_lowercase();
    for (main, synonyms, _) in LOCK_TYPES {
        if w == *main || synonyms.contains(&w.as_str()) {
            return Some(main);
        }
    }
    None
}

/// دستور lock/unlock را پارس می‌کند: "lock link", "llink", "unlock link", "ullink", ...
/// خروجی: (is_lock, lock_type)
pub fn parse_lock_command(text: &str) -> Option<(bool, &'static str)> {
    let t = text.trim();
    let t = t.strip_prefix('/').unwrap_or(t);
    let lower = t.to_lowercase();

    // حالت با فاصله: "lock link" / "l link" / "unlock link" / "ul link"
    if let Some(rest) = lower.strip_prefix("unlock ") {
        return resolve_lock_type(rest.trim()).map(|lt| (false, lt));
    }
    if let Some(rest) = lower.strip_prefix("ul ") {
        return resolve_lock_type(rest.trim()).map(|lt| (false, lt));
    }
    if let Some(rest) = lower.strip_prefix("lock ") {
        return resolve_lock_type(rest.trim()).map(|lt| (true, lt));
    }
    if let Some(rest) = lower.strip_prefix("l ") {
        return resolve_lock_type(rest.trim()).map(|lt| (true, lt));
    }

    // حالت چسبیده: "ullink" / "unlocklink" / "llink"
    if let Some(rest) = lower.strip_prefix("unlock") {
        if let Some(lt) = resolve_lock_type(rest) {
            return Some((false, lt));
        }
    }
    if let Some(rest) = lower.strip_prefix("ul") {
        if let Some(lt) = resolve_lock_type(rest) {
            return Some((false, lt));
        }
    }
    if let Some(rest) = lower.strip_prefix("lock") {
        if let Some(lt) = resolve_lock_type(rest) {
            return Some((true, lt));
        }
    }
    if let Some(rest) = lower.strip_prefix('l') {
        if let Some(lt) = resolve_lock_type(rest) {
            return Some((true, lt));
        }
    }

    None
}

// ================= سیستم مقام =================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankCommand {
    MakeAdmin,      // ad / admin
    MakeOwner,      // owner / own
    RemoveAny,      // rem / remove  -> کامل به عادی
    RemoveAdmin,    // rad           -> فقط از ادمینی
    DemoteOwner,    // rowner / rown -> از مالک به ادمین
    Vip,            // vip
    RemoveVip,      // rvip
}

/// متن پیام را (که باید دقیقاً و فقط این کلمه باشد) به یک دستور مقام نگاشت می‌کند.
pub fn resolve_rank_command(text: &str) -> Option<RankCommand> {
    let t = text.trim();
    let t = t.strip_prefix('/').unwrap_or(t);
    let lower = t.to_lowercase();

    match lower.as_str() {
        "ad" | "admin" => Some(RankCommand::MakeAdmin),
        "owner" | "own" => Some(RankCommand::MakeOwner),
        "rem" | "remove" => Some(RankCommand::RemoveAny),
        "rad" => Some(RankCommand::RemoveAdmin),
        "rowner" | "rown" => Some(RankCommand::DemoteOwner),
        "vip" => Some(RankCommand::Vip),
        "rvip" => Some(RankCommand::RemoveVip),
        _ => None,
    }
}

pub enum RankActionResult {
    Ok(String),
    Denied(String),
    NoTarget,
}

/// یک دستور مقام را با توجه به رتبه فرستنده اجرا می‌کند. target_name فقط برای پیام خروجی است.
pub fn apply_rank_command(
    group_id: i64,
    sender_id: i64,
    target_id: i64,
    target_name: &str,
    cmd: RankCommand,
) -> RankActionResult {
    let sender_rank = match db::get_rank(group_id, sender_id) {
        Ok(r) => r,
        Err(_) => return RankActionResult::Denied("خطای داخلی در بررسی رتبه.".to_string()),
    };
    let target_rank = match db::get_rank(group_id, target_id) {
        Ok(r) => r,
        Err(_) => return RankActionResult::Denied("خطای داخلی در بررسی رتبه.".to_string()),
    };

    use db::Rank::*;

    match cmd {
        RankCommand::MakeAdmin => {
            if !matches!(sender_rank, FirstOwner | Owner) {
                return RankActionResult::Denied("Only owners can promote members to bot admin.".to_string());
            }
            let _ = db::add_admin(group_id, target_id);
            RankActionResult::Ok(format!("{} has been added to the bot admins list.", target_name))
        }
        RankCommand::MakeOwner => {
            if sender_rank != FirstOwner {
                return RankActionResult::Denied("Only the first owner can promote members to bot owner.".to_string());
            }
            let _ = db::add_owner(group_id, target_id);
            RankActionResult::Ok(format!("{} has been added to the bot owners list.", target_name))
        }
        RankCommand::RemoveAny => {
            match sender_rank {
                FirstOwner => {
                    if target_rank == FirstOwner {
                        return RankActionResult::Denied("The first owner cannot be removed.".to_string());
                    }
                    let _ = db::demote_to_regular(group_id, target_id);
                    RankActionResult::Ok(format!("{} is now a regular member.", target_name))
                }
                Owner => {
                    if target_rank != Admin {
                        return RankActionResult::Denied("You can only use this on bot admins.".to_string());
                    }
                    let _ = db::demote_to_regular(group_id, target_id);
                    RankActionResult::Ok(format!("{} is now a regular member.", target_name))
                }
                _ => RankActionResult::Denied("You don't have permission to use this command.".to_string()),
            }
        }
        RankCommand::RemoveAdmin => {
            match sender_rank {
                FirstOwner => {
                    let _ = db::remove_admin(group_id, target_id);
                    RankActionResult::Ok(format!("{} is no longer a bot admin.", target_name))
                }
                Owner => {
                    if target_rank != Admin {
                        return RankActionResult::Denied("You can only use this on bot admins.".to_string());
                    }
                    let _ = db::remove_admin(group_id, target_id);
                    RankActionResult::Ok(format!("{} is no longer a bot admin.", target_name))
                }
                _ => RankActionResult::Denied("You don't have permission to use this command.".to_string()),
            }
        }
        RankCommand::DemoteOwner => {
            if sender_rank != FirstOwner {
                return RankActionResult::Denied("Only the first owner can demote an owner.".to_string());
            }
            if target_rank == FirstOwner {
                return RankActionResult::Denied("The first owner cannot be demoted.".to_string());
            }
            let _ = db::remove_owner(group_id, target_id);
            let _ = db::add_admin(group_id, target_id);
            RankActionResult::Ok(format!("{} has been demoted from owner to admin.", target_name))
        }
        RankCommand::Vip => {
            if !matches!(sender_rank, FirstOwner | Owner) {
                return RankActionResult::Denied("Only owners can grant VIP status.".to_string());
            }
            let _ = db::add_vip(group_id, target_id);
            RankActionResult::Ok(format!("{} is now a VIP member.", target_name))
        }
        RankCommand::RemoveVip => {
            if !matches!(sender_rank, FirstOwner | Owner) {
                return RankActionResult::Denied("Only owners can remove VIP status.".to_string());
            }
            let _ = db::remove_vip(group_id, target_id);
            RankActionResult::Ok(format!("{} is no longer a VIP member.", target_name))
        }
    }
}

/// آیا این کاربر اجازه دارد فرد ویژه را بن کند؟ (طبق تصمیم: فقط مالکین)
pub fn can_ban_vip(group_id: i64, actor_id: i64) -> bool {
    matches!(
        db::get_rank(group_id, actor_id),
        db::Rank::FirstOwner | db::Rank::Owner
    )
}

// ================= پیام "بات" -> "بله" =================

/// چک می‌کند که آیا متن پیام (بعد از trim) دقیقاً کلمه "بات" است.
pub fn is_bare_bot_word(text: &str) -> bool {
    text.trim() == "بات"
}

// ================= اجرای قفل‌ها روی پیام‌های گروه =================

/// نوع محتوای یک پیام تلگرام را (برای مقایسه با قفل‌ها) تشخیص می‌دهد.
pub fn detect_message_content_types(message: &Value) -> Vec<&'static str> {
    let mut types = Vec::new();

    if message.get("sticker").is_some() {
        types.push("sticker");
    }
    if message.get("animation").is_some() {
        types.push("gif");
    }
    if message.get("video").is_some() {
        types.push("video");
    }
    if message.get("photo").is_some() {
        types.push("photo");
    }
    if message.get("voice").is_some() {
        types.push("voice");
    }
    if message.get("audio").is_some() {
        types.push("audio");
    }
    if message.get("document").is_some() {
        types.push("file");
    }
    if message.get("location").is_some() || message.get("venue").is_some() {
        types.push("location");
    }
    if message.get("contact").is_some() {
        types.push("contact");
    }
    if message.get("game").is_some() {
        types.push("game");
    }
    if message.get("forward_origin").is_some() || message.get("forward_from").is_some() {
        types.push("forward");
    }
    if message.get("via_bot").is_some() {
        types.push("bot");
    }
    if message["from"]["is_bot"].as_bool() == Some(true) {
        types.push("bot");
    }

    // لینک/منشن/هشتگ از روی entities تشخیص داده می‌شوند
    if let Some(entities) = message.get("entities").and_then(|e| e.as_array()) {
        for ent in entities {
            match ent["type"].as_str() {
                Some("url") | Some("text_link") => types.push("link"),
                Some("mention") | Some("text_mention") => types.push("mention"),
                Some("hashtag") => types.push("hashtag"),
                _ => {}
            }
        }
    }

    // پیام از طرف ادمین ناشناس گروه
    if message.get("sender_chat").is_some() {
        types.push("anon");
    }

    types
}

/// برای یک پیام گروه، چک می‌کند آیا باید حذف شود (چون نوع محتوایش برای این کاربر قفل است).
/// اولین نوع قفل‌شده‌ای که پیدا شود را برمی‌گرداند (برای پیام لاگ/اطلاع‌رسانی، اختیاری).
pub fn should_delete(group_id: i64, user_id: i64, message: &Value) -> Option<&'static str> {
    let content_types = detect_message_content_types(message);
    for ct in content_types {
        if let Ok(true) = db::is_user_restricted(group_id, user_id, ct) {
            return Some(ct);
        }
    }
    None
}
