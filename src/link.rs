//! Build `tg://` deep links and deliver them to the running AyuGram over D-Bus.

use anyhow::{anyhow, Result};
use std::process::Command;

use crate::cache::Entry;

// tdesktop packs the peer type into bits 48..56 of the internal `PeerId`
// (see data/data_peer_id.h: `bare | (Shift << 48)`). Shift 0 = user,
// 1 = basic chat, 2 = channel/supergroup.
const CHAT_SHIFT: u64 = 1 << 48;
const CHANNEL_SHIFT: u64 = 2 << 48;

/// Build the `tg://` URL to open `entry` in the *currently active* account.
///
/// No `acc=` param — account switching is done separately via [`switch_account`]
/// because the deep-link `acc=` switch crashes this AyuGram build.
///
/// - Public peer → `resolve?domain=`.
/// - Username-less **channel/supergroup** → `privatepost?channel=<raw>`. This
///   goes through AyuGram's `showPeerByLink`, which resolves the channel via the
///   API even when it isn't loaded in the client. The `chat?id=` handler's
///   fallback is broken for unloaded channels (it re-prepends `-100` to the
///   already-packed PeerId), so jumps to less-active private channels fail.
/// - Username-less user / legacy basic group → `chat?id=<packed PeerId>`.
pub fn build(entry: &Entry) -> String {
    if let Some(username) = &entry.username {
        format!("tg://resolve?domain={username}")
    } else if entry.id <= -1_000_000_000_000 {
        // Channel/supergroup: bot id is -(1e12 + bare); recover the bare id.
        let raw = -entry.id - 1_000_000_000_000;
        format!("tg://privatepost?channel={raw}")
    } else {
        format!("tg://chat?id={}", tdesktop_peer_id(entry.id))
    }
}

/// Build the URL to open a specific forum `topic` within `entry`.
///
/// Public forum: `resolve?domain=X&topic=<id>`. Private forum: the `chat?id=`
/// handler doesn't take a topic, so use `privatepost?channel=<raw>&topic=<id>`
/// (`raw` is the bare channel id, i.e. the Bot-API id minus the -100… prefix).
pub fn build_topic(entry: &Entry, topic_id: i64) -> String {
    if let Some(username) = &entry.username {
        format!("tg://resolve?domain={username}&topic={topic_id}")
    } else {
        let raw = -entry.id - 1_000_000_000_000;
        format!("tg://privatepost?channel={raw}&topic={topic_id}")
    }
}

/// Switch AyuGram to an account by focusing its window (via `focus_cmd`) and
/// injecting the user-configured key (e.g. "alt+1"). This drives AyuGram's own
/// account-switch shortcut — the safe path — instead of the deep-link `acc=`
/// switch that crashes. Requires the focuser (default i3-msg) and `xdotool`
/// (X11). No-op if `key` is empty.
pub fn switch_account(key: &str, focus_cmd: &str, window_class: &str) -> Result<()> {
    if key.is_empty() {
        return Ok(());
    }
    focus_and_keys(focus_cmd, window_class, &[key])
}

/// Open AyuGram's archive folder view: switch to the account (if a key is set),
/// then inject Ctrl+9 (AyuGram's `ShowArchive` shortcut). There's no `tg://` for
/// the archive folder, so this is the only way to focus it.
pub fn open_archive(switch_key: &str, focus_cmd: &str, window_class: &str) -> Result<()> {
    let mut keys: Vec<&str> = Vec::new();
    if !switch_key.is_empty() {
        keys.push(switch_key);
    }
    keys.push("ctrl+9");
    focus_and_keys(focus_cmd, window_class, &keys)
}

/// Focus AyuGram (via `focus_cmd`), then inject each key in order with a small
/// settle delay between them.
///
/// xdotool's own `windowactivate` is unreliable on i3 (it blocks/warns on
/// `_NET_WM_DESKTOP`), so focusing is delegated to a WM-native command; keys are
/// then sent to whatever is now focused.
fn focus_and_keys(focus_cmd: &str, window_class: &str, keys: &[&str]) -> Result<()> {
    let focus = focus_cmd.replace("{class}", window_class);
    let status = Command::new("sh").arg("-c").arg(&focus).status()?;
    if !status.success() {
        return Err(anyhow!("focus command failed: {focus}"));
    }
    std::thread::sleep(std::time::Duration::from_millis(150));

    for key in keys {
        let status = Command::new("xdotool")
            .args(["key", "--clearmodifiers", key])
            .status()?;
        if !status.success() {
            return Err(anyhow!(
                "xdotool failed to send '{key}' (is xdotool installed?)"
            ));
        }
        // Let the action (account switch / folder open) settle before the next.
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    Ok(())
}

/// Convert a Bot-API style id (as stored in the cache) to tdesktop's internal
/// `PeerId` value expected by the `tg://chat?id=` handler.
fn tdesktop_peer_id(bot_id: i64) -> u64 {
    if bot_id >= 0 {
        // User: bare id, no type shift.
        bot_id as u64
    } else if bot_id <= -1_000_000_000_000 {
        // Channel / supergroup: bot id is -(1e12 + bare).
        let bare = (-bot_id - 1_000_000_000_000) as u64;
        bare | CHANNEL_SHIFT
    } else {
        // Basic (legacy) group: bot id is -bare.
        let bare = (-bot_id) as u64;
        bare | CHAT_SHIFT
    }
}

/// Deliver each URL in order, in-process, to the running AyuGram over D-Bus.
///
/// AyuGram owns `com.ayugram.desktop` and implements
/// `org.freedesktop.Application.Open`, which navigates the *existing* window
/// without spawning anything. This is the only route that doesn't create a
/// second `AyuGram` process — on this platform spawning a competing process
/// (via `xdg-open` or a direct `AyuGram -- <url>` exec) reliably kills the
/// running primary. When AyuGram isn't running, the same call D-Bus-activates a
/// fresh instance and opens the chat. Both are safe.
pub fn open(url: &str) -> Result<()> {
    let status = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "com.ayugram.desktop",
            "--object-path",
            "/com/ayugram/desktop",
            "--method",
            "org.freedesktop.Application.Open",
            &format!("['{url}']"),
            "{}",
        ])
        .status()?;
    if !status.success() {
        return Err(anyhow!("gdbus call failed for {url}"));
    }
    Ok(())
}
