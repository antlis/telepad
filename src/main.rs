//! telepad — a rofi quick-switcher that jumps AyuGram/Telegram Desktop to any
//! chat, group or channel across all your accounts.
//!
//! Data (the chat list) comes from grammers over MTProto; navigation is handed
//! off to the running AyuGram instance via `tg://` deep links (`acc=` switches
//! account, no D-Bus or key simulation needed).

mod cache;
mod config;
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
            let target = args.get(1).map(String::as_str).unwrap_or("all");
            sync(&cfg, target).await?;
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
async fn sync(cfg: &Config, target: &str) -> Result<()> {
    let accounts: Vec<_> = if target == "all" {
        cfg.accounts.iter().collect()
    } else {
        vec![cfg
            .account(target)
            .ok_or_else(|| anyhow!("no account matching '{target}'"))?]
    };

    for account in accounts {
        match telegram::fetch_dialogs(cfg, account).await {
            Ok(cache) => {
                let count = cache.entries.len();
                cache::write(&account.session, &cache)?;
                println!("synced '{}' ({count} chats)", account.label);
            }
            Err(e) => eprintln!("skipping '{}': {e}", account.label),
        }
    }
    Ok(())
}

/// Show the flat, cross-account quick-switcher and jump to the selection.
async fn menu(cfg: &Config) -> Result<()> {
    // Flatten every account's cache into one list, tagged with the account.
    let mut lines = Vec::new();
    // (switch_key, entry) for each row.
    let mut targets: Vec<(String, cache::Entry)> = Vec::new();

    for account in &cfg.accounts {
        if !cache::exists(&account.session) {
            continue;
        }
        let account_cache = cache::read(&account.session)?;
        for entry in account_cache.entries {
            let badge = if !entry.topics.is_empty() {
                "forum ▸"
            } else {
                match entry.kind.as_str() {
                    "user" => "dm",
                    "group" => "group",
                    "channel" => "channel",
                    other => other,
                }
            };
            lines.push(format!("[{}] {}  ·  {}", account.label, entry.name, badge));
            targets.push((account.switch_key.clone(), entry));
        }
    }

    if targets.is_empty() {
        return Err(anyhow!("cache is empty — run `telepad sync` first"));
    }

    let Some(index) = rofi::pick("Jump", &lines)? else {
        return Ok(()); // cancelled
    };
    let (switch_key, entry) = &targets[index];

    // For a forum, offer its topics in a second menu first (while rofi still
    // has focus — before we steal focus to AyuGram for the account switch).
    let url = if entry.topics.is_empty() {
        link::build(entry)
    } else {
        let mut lines = Vec::with_capacity(entry.topics.len() + 1);
        lines.push("↩  open group (no topic)".to_string());
        lines.extend(entry.topics.iter().map(|t| t.title.clone()));
        match rofi::pick(&format!("{} ▸ topic", entry.name), &lines)? {
            None => return Ok(()),                 // cancelled
            Some(0) => link::build(entry),         // whole group
            Some(i) => link::build_topic(entry, entry.topics[i - 1].id),
        }
    };

    // Switch to the target account (safe key path), then open in it.
    // A failed switch is non-fatal: we still try to open in the active account.
    if let Err(e) = link::switch_account(switch_key, &cfg.focus_cmd, &cfg.window_class) {
        eprintln!("warning: account switch failed: {e}");
    }
    link::open(&url)?;
    Ok(())
}
