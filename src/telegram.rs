//! grammers (MTProto) integration: connect, interactive login, dialog fetch.

use anyhow::{anyhow, Context, Result};
use std::io::Write;
use std::sync::Arc;

use grammers_client::client::LoginToken;
use grammers_client::peer::Peer;
use grammers_client::session::storages::SqliteSession;
use grammers_client::{Client, SenderPool, SignInError};

use crate::cache::{AccountCache, Entry};
use crate::config::{self, Account, Config};

/// A live client plus the background tasks driving its network I/O.
pub struct Session {
    pub client: Client,
    _runner: tokio::task::JoinHandle<()>,
    _updates: tokio::task::JoinHandle<()>,
}

/// Open (or create) a session file and connect a client for it.
pub async fn connect(cfg: &Config, session: &str) -> Result<Session> {
    let path = config::session_path(session);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let storage = SqliteSession::open(&path)
        .await
        .with_context(|| format!("opening session {}", path.display()))?;

    let pool = SenderPool::new(Arc::new(storage), cfg.api_id);
    let SenderPool {
        runner,
        handle,
        mut updates,
    } = pool;

    let client = Client::new(handle);
    let runner = tokio::spawn(runner.run());
    // We never consume updates, but the receiver must be drained so the pool
    // doesn't warn about a full buffer while we page through dialogs.
    let updates = tokio::spawn(async move { while updates.recv().await.is_some() {} });

    Ok(Session {
        client,
        _runner: runner,
        _updates: updates,
    })
}

/// Interactive login for a single account: sends a code, reads it from stdin,
/// and handles a 2FA password if the account has one.
pub async fn login(cfg: &Config, account: &Account) -> Result<()> {
    if account.phone.is_empty() {
        return Err(anyhow!(
            "account '{}' has no `phone` in config; add it before logging in",
            account.label
        ));
    }
    let session = connect(cfg, &account.session).await?;
    let client = &session.client;

    if client.is_authorized().await? {
        println!("'{}' is already logged in.", account.label);
        return Ok(());
    }

    println!(
        "Logging in '{}' ({}). The code is delivered inside your already \
         logged-in Telegram/AyuGram app (Telegram service chat), not by SMS.",
        account.label, account.phone
    );
    let token: LoginToken = client
        .request_login_code(&account.phone, &cfg.api_hash)
        .await
        .context("requesting login code")?;

    let code = prompt("Login code: ")?;
    match client.sign_in(&token, code.trim()).await {
        Ok(user) => {
            println!("Logged in as {}.", user.first_name().unwrap_or("(unknown)"));
        }
        Err(SignInError::PasswordRequired(password_token)) => {
            let password = prompt("2FA password: ")?;
            client
                .check_password(password_token, password.trim())
                .await
                .map_err(|e| anyhow!("2FA password rejected: {e:?}"))?;
            println!("Logged in (with 2FA).");
        }
        Err(e) => return Err(anyhow!("sign-in failed: {e:?}")),
    }
    Ok(())
}

/// Fetch all dialogs for an account into a fresh cache.
pub async fn fetch_dialogs(cfg: &Config, account: &Account) -> Result<AccountCache> {
    let session = connect(cfg, &account.session).await?;
    let client = &session.client;

    if !client.is_authorized().await? {
        return Err(anyhow!(
            "'{}' is not logged in; run `telepad login {}` first",
            account.label,
            account.session
        ));
    }

    let mut entries = Vec::new();
    let mut dialogs = client.iter_dialogs();
    while let Some(dialog) = dialogs.next().await? {
        let peer = &dialog.peer;
        let kind = match peer {
            Peer::User(_) => "user",
            Peer::Group(_) => "group",
            Peer::Channel(_) => "channel",
        };
        let name = peer.name().unwrap_or("(no name)").to_string();
        entries.push(Entry {
            name,
            username: peer.username().map(str::to_string),
            id: peer.id().to_string().parse().unwrap_or(0),
            kind: kind.to_string(),
        });
    }

    Ok(AccountCache {
        acc: account.acc,
        label: account.label.clone(),
        entries,
    })
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line)
}
