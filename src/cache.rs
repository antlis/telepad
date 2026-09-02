//! On-disk dialog cache: one JSON file per account, read by the rofi menu.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Display name (user first name, or group/channel title).
    pub name: String,
    /// Public @username without the "@", if any. Preferred for deep links.
    pub username: Option<String>,
    /// Bot-API style id (positive for users). Used for username-less private chats.
    pub id: i64,
    /// "user" | "group" | "channel".
    pub kind: String,
    /// Forum topics, if this is a forum supergroup (empty otherwise).
    #[serde(default)]
    pub topics: Vec<Topic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topic {
    pub id: i64,
    pub title: String,
}

/// A Telegram chat folder (dialog filter), surfaced as a submenu of its chats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub title: String,
    pub entries: Vec<Entry>,
}

/// Full cache for a single account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCache {
    /// 1-based AyuGram account index (mirrors config).
    pub acc: i32,
    /// Label shown in rofi.
    pub label: String,
    /// This account's own user id, used for the Saved Messages row. 0 if a stale
    /// cache predates this field (the row is simply omitted until the next sync).
    #[serde(default)]
    pub me_id: i64,
    /// This account's own @username, if any (for the Saved Messages deep link).
    #[serde(default)]
    pub me_username: Option<String>,
    pub entries: Vec<Entry>,
    /// Archived chats (folder 1), surfaced behind a separate menu item.
    #[serde(default)]
    pub archived: Vec<Entry>,
    /// User-defined chat folders (dialog filters), each a submenu of chats.
    #[serde(default)]
    pub folders: Vec<Folder>,
}

pub fn write(session: &str, cache: &AccountCache) -> Result<()> {
    let path = config::cache_path(session);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let json = serde_json::to_string_pretty(cache)?;
    std::fs::write(&path, json).with_context(|| format!("writing cache {}", path.display()))?;
    Ok(())
}

pub fn read(session: &str) -> Result<AccountCache> {
    let path = config::cache_path(session);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading cache {} (run `telepad sync` first)", path.display()))?;
    let cache: AccountCache = serde_json::from_str(&text)?;
    Ok(cache)
}

pub fn exists(session: &str) -> bool {
    Path::new(&config::cache_path(session)).exists()
}
