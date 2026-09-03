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
    /// Which Telegram Desktop client to drive: "telegram" (default) or
    /// "ayugram". Selects the D-Bus service name and X11 window class. For any
    /// other fork, leave this and set `dbus_service` / `window_class` yourself.
    #[serde(default = "default_client")]
    pub client: String,
    /// Override the X11 window class (else derived from `client`), focused
    /// before injecting the switch key.
    #[serde(default)]
    window_class: Option<String>,
    /// Override the D-Bus service name the client owns (else derived from
    /// `client`). Handy for TDesktop forks without a built-in `client` preset.
    #[serde(default)]
    dbus_service: Option<String>,
    /// Shell command that focuses the client window; `{class}` is replaced with
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

fn default_client() -> String {
    "telegram".to_string()
}

fn default_focus_cmd() -> String {
    r#"i3-msg [class="{class}"] focus"#.to_string()
}

/// Built-in D-Bus service name and X11 window class for a known client.
struct ClientProfile {
    dbus_service: &'static str,
    window_class: &'static str,
}

/// The clients telepad ships presets for. Any other TDesktop fork works by
/// setting `dbus_service` / `window_class` in the config instead.
fn client_profile(client: &str) -> Option<ClientProfile> {
    match client {
        "telegram" => Some(ClientProfile {
            dbus_service: "org.telegram.desktop",
            window_class: "TelegramDesktop",
        }),
        "ayugram" => Some(ClientProfile {
            dbus_service: "com.ayugram.desktop",
            window_class: "AyuGramDesktop",
        }),
        _ => None,
    }
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
        // An unknown `client` is only OK if the D-Bus name is given explicitly.
        if client_profile(&config.client).is_none() && config.dbus_service.is_none() {
            return Err(anyhow!(
                "unknown client '{}' — use client = \"telegram\" or \"ayugram\", \
                 or set `dbus_service` (and `window_class`) explicitly",
                config.client
            ));
        }
        Ok(config)
    }

    pub fn account(&self, key: &str) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.session == key || a.label == key || a.acc.to_string() == key)
    }

    /// X11 window class of the client, focused before injecting the switch key.
    /// The explicit `window_class` override wins; otherwise it's the `client`
    /// preset's class.
    pub fn window_class(&self) -> String {
        self.window_class
            .clone()
            .or_else(|| client_profile(&self.client).map(|p| p.window_class.to_string()))
            .unwrap_or_default()
    }

    /// D-Bus well-known name the client owns (it implements
    /// `org.freedesktop.Application`). Override wins over the `client` preset.
    pub fn dbus_service(&self) -> String {
        self.dbus_service
            .clone()
            .or_else(|| client_profile(&self.client).map(|p| p.dbus_service.to_string()))
            .unwrap_or_default()
    }

    /// Object path for the D-Bus service: the well-known name with dots turned
    /// into slashes and a leading slash (the `org.freedesktop.Application`
    /// convention, e.g. `org.telegram.desktop` → `/org/telegram/desktop`).
    pub fn dbus_object_path(&self) -> String {
        format!("/{}", self.dbus_service().replace('.', "/"))
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

pub fn frecency_path() -> PathBuf {
    data_dir().join("frecency.json")
}

/// Directory holding cached profile photos (populated by `sync --avatars`).
pub fn avatars_dir() -> PathBuf {
    data_dir().join("avatars")
}

/// Cached profile photo for a peer (downloaded only by `sync --avatars`).
pub fn avatar_path(id: i64) -> PathBuf {
    avatars_dir().join(format!("{id}.jpg"))
}

/// Generic fallback icon shown for rows without a cached avatar (written on
/// demand the first time it's needed).
pub fn placeholder_avatar_path() -> PathBuf {
    data_dir().join("placeholder-avatar.svg")
}
