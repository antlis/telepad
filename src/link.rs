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
/// because the deep-link `acc=` switch crashes this AyuGram build. A peer with a
/// public username resolves by name; a username-less peer (private group/channel
/// or a DM with no @username) opens by internal id via AyuGram's `chat?id=`.
pub fn build(entry: &Entry) -> String {
    if let Some(username) = &entry.username {
        format!("tg://resolve?domain={username}")
    } else {
        format!("tg://chat?id={}", tdesktop_peer_id(entry.id))
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
    // Focus AyuGram first. xdotool's own `windowactivate` is unreliable on i3
    // (it blocks/warns on `_NET_WM_DESKTOP`), so focusing is delegated to a
    // WM-native command; the key is then sent to whatever is now focused.
    let focus = focus_cmd.replace("{class}", window_class);
    let status = Command::new("sh").arg("-c").arg(&focus).status()?;
    if !status.success() {
        return Err(anyhow!("focus command failed: {focus}"));
    }
    // Let focus settle before typing.
    std::thread::sleep(std::time::Duration::from_millis(150));

    let status = Command::new("xdotool")
        .args(["key", "--clearmodifiers", key])
        .status()?;
    if !status.success() {
        return Err(anyhow!(
            "xdotool failed to send '{key}' (is xdotool installed?)"
        ));
    }
    // Let AyuGram finish switching before we open the chat, so the URL lands in
    // the newly-active account.
    std::thread::sleep(std::time::Duration::from_millis(250));
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
