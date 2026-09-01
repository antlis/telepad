//! Configuration loading from `~/.config/telepad/config.toml`.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Telegram API id from https://my.telegram.org (one app is fine for all accounts).
    pub api_id: i32,
    /// Telegram API hash that pairs with `api_id`.
    pub api_hash: String,
    /// X11 window class used to focus AyuGram before injecting the switch key.
    #[serde(default = "default_window_class")]
    pub window_class: String,
    /// Shell command that focuses the AyuGram window; `{class}` is replaced with
    /// `window_class`. Default targets i3. For other setups override it, e.g.
    /// `wmctrl -xa {class}`.
    #[serde(default = "default_focus_cmd")]
    pub focus_cmd: String,
    /// Accounts to index, in the same order as AyuGram's account switcher.
    #[serde(default)]
    pub accounts: Vec<Account>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    /// 1-based position in AyuGram's account list (kept for display/ordering).
    pub acc: i32,
    /// Human label shown in rofi (e.g. "Personal", "Work").
    pub label: String,
    /// Session file name (stored under the data dir as `<session>.session`).
    pub session: String,
    /// Phone number in international format, used only for the initial `login`.
    #[serde(default)]
    pub phone: String,
    /// xdotool key sequence that switches to this account in AyuGram
    /// (whatever you bound in AyuGram's shortcuts, e.g. "alt+1"). If empty,
    /// telepad won't switch accounts and will open in whatever account is
    /// active — the deep-link `acc=` switch crashes this build, so a key is
    /// the only safe way to switch.
    #[serde(default)]
    pub switch_key: String,
}

fn default_window_class() -> String {
    "AyuGramDesktop".to_string()
}

fn default_focus_cmd() -> String {
    r#"i3-msg [class="{class}"] focus"#.to_string()
}

impl Config {
    pub fn load() -> Result<Config> {
        let path = config_path();
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading config at {}", path.display()))?;
        let config: Config = toml::from_str(&text).context("parsing config.toml")?;
        if config.accounts.is_empty() {
            return Err(anyhow!("no [[accounts]] configured in {}", path.display()));
        }
        Ok(config)
    }

    pub fn account(&self, key: &str) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.session == key || a.label == key || a.acc.to_string() == key)
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("telepad")
        .join("config.toml")
}

/// Directory holding session files and the dialog cache.
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("telepad")
}

pub fn session_path(session: &str) -> PathBuf {
    data_dir().join(format!("{session}.session"))
}

pub fn cache_path(session: &str) -> PathBuf {
    data_dir().join("cache").join(format!("{session}.json"))
}
