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
}

/// Full cache for a single account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCache {
    /// 1-based AyuGram account index (mirrors config).
    pub acc: i32,
    /// Label shown in rofi.
    pub label: String,
    pub entries: Vec<Entry>,
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
