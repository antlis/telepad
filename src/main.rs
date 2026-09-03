//! telepad — a rofi quick-switcher that jumps AyuGram/Telegram Desktop to any
//! chat, group or channel across all your accounts.
//!
//! Data (the chat list) comes from grammers over MTProto; navigation is handed
//! off to the running AyuGram instance via `tg://` deep links (`acc=` switches
//! account, no D-Bus or key simulation needed).

mod cache;
mod config;
mod frecency;
mod link;
mod rofi;
mod telegram;

use anyhow::{anyhow, Result};

use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = Config::load()?;

    match args.first().map(String::as_str) {
        Some("login") => {
            let key = args.get(1).ok_or_else(|| anyhow!("usage: telepad login <session|label|acc>"))?;
            let account = cfg
                .account(key)
                .ok_or_else(|| anyhow!("no account matching '{key}'"))?
                .clone();
            telegram::login(&cfg, &account).await?;
        }
        Some("sync") => {
            let rest = &args[1..];
            let avatars = rest.iter().any(|a| a == "--avatars");
            let target = rest
                .iter()
                .find(|a| !a.starts_with("--"))
                .map(String::as_str)
                .unwrap_or("all");
            sync(&cfg, target, avatars).await?;
        }
        Some("menu") | None => menu(&cfg).await?,
        Some(other) => {
            return Err(anyhow!(
                "unknown command '{other}'. Commands: login <acct>, sync [acct|all], menu"
            ));
        }
    }
    Ok(())
}

/// Refresh the dialog cache for one account or all of them.
async fn sync(cfg: &Config, target: &str, avatars: bool) -> Result<()> {
    let accounts: Vec<_> = if target == "all" {
        cfg.accounts.iter().collect()
    } else {
        vec![cfg
            .account(target)
            .ok_or_else(|| anyhow!("no account matching '{target}'"))?]
    };

    for account in accounts {
        match telegram::fetch_dialogs(cfg, account, avatars).await {
            Ok(cache) => {
                let count = cache.entries.len();
                let archived = cache.archived.len();
                let folders = cache.folders.len();
                cache::write(&account.session, &cache)?;
                println!(
                    "synced '{}' ({count} chats, {archived} archived, {folders} folders)",
                    account.label
                );
            }
            Err(e) => eprintln!("skipping '{}': {e}", account.label),
        }
    }
    Ok(())
}

/// A selectable row in the main menu.
enum Target {
    /// A chat/group/channel/contact (may itself expand into forum topics).
    Chat {
        switch_key: String,
        entry: cache::Entry,
    },
    /// A single forum topic, listed flat so it's searchable on first keystroke.
    Topic {
        switch_key: String,
        entry: cache::Entry,
        topic_id: i64,
    },
    /// This account's archive folder — expands into a submenu of archived chats.
    Archive {
        switch_key: String,
        label: String,
        entries: Vec<cache::Entry>,
    },
    /// A chat folder (dialog filter) — expands into a submenu of its chats.
    Folder {
        switch_key: String,
        title: String,
        entries: Vec<cache::Entry>,
    },
}

/// A trailing `  ·  @handle` fragment, so rows are searchable by public username
/// too. Empty when the peer has no `@username`.
fn handle(entry: &cache::Entry) -> String {
    match &entry.username {
        Some(u) => format!("  ·  @{u}"),
        None => String::new(),
    }
}

fn badge(entry: &cache::Entry) -> &'static str {
    if !entry.topics.is_empty() {
        return "forum ▸";
    }
    match entry.kind.as_str() {
        "user" => "dm",
        "channel" => "channel",
        _ => "group",
    }
}

/// One selectable row: its display line, what it jumps to, and the peer id used
/// for frecency scoring (`None` for the folder/archive header rows).
struct Row {
    line: String,
    target: Target,
    id: Option<i64>,
    /// Path to a cached avatar, shown as the rofi row icon if present.
    icon: Option<String>,
}

/// Path to a peer's cached avatar, if one was downloaded by `sync --avatars`.
fn avatar_for(id: i64) -> Option<String> {
    let path = config::avatar_path(id);
    path.exists().then(|| path.to_string_lossy().into_owned())
}

/// A neutral silhouette used for rows that have no cached avatar, so the list
/// stays visually even once some avatars are present.
const PLACEHOLDER_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <circle cx="50" cy="50" r="50" fill="#4c566a"/>
  <circle cx="50" cy="40" r="17" fill="#d8dee9"/>
  <path d="M22 84a28 28 0 0 1 56 0z" fill="#d8dee9"/>
</svg>
"##;

/// True if any avatars have been cached — i.e. `sync --avatars` has been run.
/// When false, the menu shows no icons at all (clean text list).
fn avatars_in_use() -> bool {
    std::fs::read_dir(config::avatars_dir())
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

/// Ensure the placeholder icon exists on disk and return its path.
fn ensure_placeholder() -> Option<String> {
    let path = config::placeholder_avatar_path();
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&path, PLACEHOLDER_SVG).ok()?;
    }
    Some(path.to_string_lossy().into_owned())
}

/// Show the flat, cross-account quick-switcher and jump to the selection.
async fn menu(cfg: &Config) -> Result<()> {
    let mut frec = frecency::Store::load();
    // Fall back to a placeholder icon only once real avatars exist, so a menu
    // that never synced avatars stays a clean, icon-less text list.
    let placeholder = if avatars_in_use() {
        ensure_placeholder()
    } else {
        None
    };

    // Flatten every account's cache into one list, tagged with the account.
    let mut rows: Vec<Row> = Vec::new();

    for account in &cfg.accounts {
        if !cache::exists(&account.session) {
            continue;
        }
        let account_cache = cache::read(&account.session)?;
        // A guaranteed Saved Messages row per account (self-chat). Omitted for
        // stale caches synced before `me_id` was captured.
        if account_cache.me_id != 0 {
            let entry = cache::Entry {
                name: "Saved Messages".to_string(),
                username: account_cache.me_username.clone(),
                id: account_cache.me_id,
                kind: "user".to_string(),
                topics: Vec::new(),
            };
            rows.push(Row {
                line: format!("[{}] ⭐ Saved Messages", account.label),
                id: Some(entry.id),
                icon: avatar_for(entry.id).or_else(|| placeholder.clone()),
                target: Target::Chat {
                    switch_key: account.switch_key.clone(),
                    entry,
                },
            });
        }
        for entry in account_cache.entries {
            // Skip the self-chat here; it's already the Saved Messages row above.
            if entry.id == account_cache.me_id {
                continue;
            }
            rows.push(Row {
                line: format!(
                    "[{}] {}{}  ·  {}",
                    account.label,
                    entry.name,
                    handle(&entry),
                    badge(&entry)
                ),
                id: Some(entry.id),
                icon: avatar_for(entry.id).or_else(|| placeholder.clone()),
                target: Target::Chat {
                    switch_key: account.switch_key.clone(),
                    entry: entry.clone(),
                },
            });
            // Also surface each forum topic as its own flat, searchable row.
            for topic in &entry.topics {
                rows.push(Row {
                    line: format!(
                        "[{}] {} ▸ {}  ·  topic",
                        account.label, entry.name, topic.title
                    ),
                    id: Some(entry.id),
                    icon: avatar_for(entry.id).or_else(|| placeholder.clone()),
                    target: Target::Topic {
                        switch_key: account.switch_key.clone(),
                        entry: entry.clone(),
                        topic_id: topic.id,
                    },
                });
            }
        }
        for folder in account_cache.folders {
            rows.push(Row {
                line: format!(
                    "[{}] 📁 {}  ·  {} ▸",
                    account.label,
                    folder.title,
                    folder.entries.len()
                ),
                id: None,
                icon: None,
                target: Target::Folder {
                    switch_key: account.switch_key.clone(),
                    title: folder.title,
                    entries: folder.entries,
                },
            });
        }
        if !account_cache.archived.is_empty() {
            rows.push(Row {
                line: format!(
                    "[{}] 🗄 Archived  ·  {} ▸",
                    account.label,
                    account_cache.archived.len()
                ),
                id: None,
                icon: None,
                target: Target::Archive {
                    switch_key: account.switch_key.clone(),
                    label: account.label.clone(),
                    entries: account_cache.archived,
                },
            });
        }
    }

    if rows.is_empty() {
        return Err(anyhow!("cache is empty — run `telepad sync` first"));
    }

    // Float your most-used chats to the top. Stable, so never-jumped rows keep
    // their original (per-account) order below the frecency-ranked ones.
    rows.sort_by(|a, b| {
        let sa = a.id.map(|i| frec.score(i)).unwrap_or(0.0);
        let sb = b.id.map(|i| frec.score(i)).unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    // rofi row-option syntax: `text\0icon\x1f<path>` attaches a per-row icon.
    let lines: Vec<String> = rows
        .iter()
        .map(|r| match &r.icon {
            Some(path) => format!("{}\0icon\x1f{}", r.line, path),
            None => r.line.clone(),
        })
        .collect();
    let Some(index) = rofi::pick("Jump", &lines)? else {
        return Ok(()); // cancelled
    };

    match &rows[index].target {
        Target::Chat { switch_key, entry } => open_entry(cfg, switch_key, entry, &mut frec),
        Target::Topic {
            switch_key,
            entry,
            topic_id,
        } => {
            frec.bump(entry.id);
            switch_and_open(cfg, switch_key, &link::build_topic(entry, *topic_id))
        }
        Target::Archive {
            switch_key,
            label,
            entries,
        } => {
            let mut lines = Vec::with_capacity(entries.len() + 1);
            lines.push("🗄 Open Archive folder in AyuGram".to_string());
            lines.extend(
                entries
                    .iter()
                    .map(|e| format!("{}{}  ·  {}", e.name, handle(e), badge(e))),
            );
            match rofi::pick(&format!("{label} ▸ archived"), &lines)? {
                None => Ok(()), // cancelled
                Some(0) => {
                    // Open the archive folder view (switch account + Ctrl+9).
                    if let Err(e) = link::open_archive(
                        switch_key,
                        &cfg.focus_cmd,
                        &cfg.inject_cmd,
                        &cfg.window_class(),
                    ) {
                        eprintln!("warning: could not open archive folder: {e}");
                    }
                    Ok(())
                }
                Some(i) => open_entry(cfg, switch_key, &entries[i - 1], &mut frec),
            }
        }
        Target::Folder {
            switch_key,
            title,
            entries,
        } => {
            let lines: Vec<String> = entries
                .iter()
                .map(|e| format!("{}{}  ·  {}", e.name, handle(e), badge(e)))
                .collect();
            match rofi::pick(&format!("📁 {title}"), &lines)? {
                None => Ok(()), // cancelled
                Some(i) => open_entry(cfg, switch_key, &entries[i], &mut frec),
            }
        }
    }
}

/// Switch to the entry's account, then open it — offering a topic submenu first
/// if it's a forum. The topic menu is shown before the account-switch keypress,
/// so rofi keeps focus during selection.
fn open_entry(
    cfg: &Config,
    switch_key: &str,
    entry: &cache::Entry,
    frec: &mut frecency::Store,
) -> Result<()> {
    let url = if entry.topics.is_empty() {
        link::build(entry)
    } else {
        let mut lines = Vec::with_capacity(entry.topics.len() + 1);
        lines.push("↩  open group (no topic)".to_string());
        lines.extend(entry.topics.iter().map(|t| t.title.clone()));
        match rofi::pick(&format!("{} ▸ topic", entry.name), &lines)? {
            None => return Ok(()),         // cancelled
            Some(0) => link::build(entry), // whole group
            Some(i) => link::build_topic(entry, entry.topics[i - 1].id),
        }
    };

    frec.bump(entry.id);
    switch_and_open(cfg, switch_key, &url)
}

/// Switch to the target account (safe key path), then open the URL in it.
/// A failed switch is non-fatal: we still try to open in the active account.
fn switch_and_open(cfg: &Config, switch_key: &str, url: &str) -> Result<()> {
    if let Err(e) =
        link::switch_account(switch_key, &cfg.focus_cmd, &cfg.inject_cmd, &cfg.window_class())
    {
        eprintln!("warning: account switch failed: {e}");
    }
    link::open(&cfg.dbus_service(), &cfg.dbus_object_path(), url)
}
