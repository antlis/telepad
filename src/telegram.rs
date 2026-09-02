//! grammers (MTProto) integration: connect, interactive login, dialog fetch.

use anyhow::{anyhow, Context, Result};
use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;

use grammers_client::client::LoginToken;
use grammers_client::peer::Peer;
use grammers_client::session::storages::SqliteSession;
use grammers_client::{tl, Client, SenderPool, SignInError};

use crate::cache::{AccountCache, Entry, Folder, Topic};
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

/// Fetch all dialogs for an account into a fresh cache. When `avatars` is set,
/// also download each dialog's profile photo into the avatar cache (slower, and
/// opt-in via `sync --avatars`).
pub async fn fetch_dialogs(cfg: &Config, account: &Account, avatars: bool) -> Result<AccountCache> {
    let session = connect(cfg, &account.session).await?;
    let client = &session.client;

    if !client.is_authorized().await? {
        return Err(anyhow!(
            "'{}' is not logged in; run `telepad login {}` first",
            account.label,
            account.session
        ));
    }

    // The account's own identity, for the Saved Messages row.
    let me = client.get_me().await.context("fetching self user")?;
    let me_id = me.id().to_string().parse().unwrap_or(0);
    let me_username = me.username().map(str::to_string);

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

        // If this is a forum supergroup, pull its topic list too.
        let topics = match forum_input_peer(peer) {
            Some(input) => fetch_topics(client, input).await,
            None => Vec::new(),
        };

        let id = peer.id().to_string().parse().unwrap_or(0);
        if avatars {
            download_avatar(client, peer, id, &name).await;
        }

        entries.push(Entry {
            name,
            username: peer.username().map(str::to_string),
            id,
            kind: kind.to_string(),
            topics,
        });
    }

    // iter_dialogs only returns open conversations, so contacts you have no
    // active chat with (e.g. blocked ones) are missing. Add them from the
    // contact list, skipping any already present as a dialog.
    let mut seen: HashSet<i64> = entries.iter().map(|e| e.id).collect();
    for contact in fetch_contacts(client).await {
        if seen.insert(contact.id) {
            entries.push(contact);
        }
    }

    let archived = fetch_archived(client).await;

    // Folders reference chats by peer id; resolve them against everything we've
    // already fetched (main dialogs, contacts, and archived chats).
    let folders = fetch_folders(client, &entries, &archived).await;

    Ok(AccountCache {
        acc: account.acc,
        label: account.label.clone(),
        me_id,
        me_username,
        entries,
        archived,
        folders,
    })
}

/// Download a peer's small profile photo into the avatar cache (best-effort).
/// Silently does nothing if the peer has no photo; warns but never fails on a
/// download error, so one bad avatar can't abort the whole sync.
async fn download_avatar(client: &Client, peer: &Peer, id: i64, name: &str) {
    let photo = match peer.photo(false).await {
        Ok(Some(photo)) => photo,
        Ok(None) => return, // no profile photo
        Err(e) => {
            eprintln!("warning: could not resolve avatar for {name}: {e:?}");
            return;
        }
    };
    let path = config::avatar_path(id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if let Err(e) = client.download_media(&photo, &path).await {
        eprintln!("warning: avatar download failed for {name}: {e:?}");
    }
}

/// Fetch the account's chat folders (dialog filters) as submenus of chats.
///
/// A folder is resolved from its explicitly listed peers (`pinned_peers` +
/// `include_peers`), matched against the chats we've already fetched. Folders
/// defined purely by category flags (e.g. "all groups") with no explicit
/// members won't populate — a known limitation. Best-effort; empty on error.
async fn fetch_folders(client: &Client, main: &[Entry], archived: &[Entry]) -> Vec<Folder> {
    use tl::enums::{DialogFilter, TextWithEntities};

    let by_id: std::collections::HashMap<i64, &Entry> = main
        .iter()
        .chain(archived.iter())
        .map(|e| (e.id, e))
        .collect();

    let filters = match client.invoke(&tl::functions::messages::GetDialogFilters {}).await {
        Ok(tl::enums::messages::DialogFilters::Filters(f)) => f.filters,
        Err(e) => {
            eprintln!("warning: could not fetch folders: {e:?}");
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    for filter in filters {
        let (title, pinned, include) = match filter {
            DialogFilter::Filter(f) => (f.title, f.pinned_peers, f.include_peers),
            DialogFilter::Chatlist(f) => (f.title, f.pinned_peers, f.include_peers),
            DialogFilter::Default => continue, // the built-in "All chats"
        };
        let TextWithEntities::Entities(title) = title;

        let mut entries = Vec::new();
        let mut seen: HashSet<i64> = HashSet::new();
        for ip in pinned.into_iter().chain(include) {
            let Some(id) = input_peer_bot_id(&ip) else { continue };
            if seen.insert(id) {
                if let Some(entry) = by_id.get(&id) {
                    entries.push((*entry).clone());
                }
            }
        }

        if !entries.is_empty() {
            out.push(Folder {
                title: title.text,
                entries,
            });
        }
    }
    out
}

/// Bot-API id for an `InputPeer`, or `None` for kinds we can't map to a chat.
fn input_peer_bot_id(peer: &tl::enums::InputPeer) -> Option<i64> {
    use tl::enums::InputPeer;
    match peer {
        InputPeer::User(u) => Some(u.user_id),
        InputPeer::Chat(c) => Some(-c.chat_id),
        InputPeer::Channel(c) => Some(-(1_000_000_000_000 + c.channel_id)),
        _ => None,
    }
}

/// Fetch the archived folder (folder_id 1) as entries. `iter_dialogs` only
/// covers the main folder and its request builder is private, so we call
/// `messages.getDialogs` directly and reconstruct entries from the raw peers.
async fn fetch_archived(client: &Client) -> Vec<Entry> {
    use tl::enums::messages::Dialogs;
    use tl::enums::{Dialog as TlDialog, InputPeer, Message as TlMessage};

    let mut out: Vec<Entry> = Vec::new();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut offset_date = 0;
    let mut offset_id = 0;
    let mut offset_peer = InputPeer::Empty;

    // Page through the archive (folder 1). A single request is capped, so we
    // keep going, advancing the offset by the last dialog, until a short page.
    for _ in 0..50 {
        let request = tl::functions::messages::GetDialogs {
            exclude_pinned: false,
            folder_id: Some(1),
            offset_date,
            offset_id,
            offset_peer: offset_peer.clone(),
            limit: 100,
            hash: 0,
        };
        let (dialogs, messages, chats, users, maybe_more) = match client.invoke(&request).await {
            Ok(Dialogs::Dialogs(d)) => (d.dialogs, d.messages, d.chats, d.users, false),
            Ok(Dialogs::Slice(d)) => {
                let more = d.dialogs.len() >= 100;
                (d.dialogs, d.messages, d.chats, d.users, more)
            }
            _ => break,
        };
        if dialogs.is_empty() {
            break;
        }

        let before = out.len();
        for d in &dialogs {
            let TlDialog::Dialog(d) = d else { continue };
            if d.folder_id != Some(1) {
                continue; // folder_id=1 request isn't always a hard filter
            }
            let bot_id = peer_bot_id(&d.peer);
            if seen.insert(bot_id) {
                if let Some(entry) = build_peer_entry(&d.peer, &users, &chats) {
                    out.push(entry);
                }
            }
        }

        // Advance the offset from the last dialog on the page.
        let last = dialogs.iter().rev().find_map(|d| match d {
            TlDialog::Dialog(d) => Some(d),
            _ => None,
        });
        let Some(last) = last else { break };
        offset_id = last.top_message;
        offset_date = messages
            .iter()
            .find_map(|m| match m {
                TlMessage::Message(m) if m.id == last.top_message => Some(m.date),
                TlMessage::Service(m) if m.id == last.top_message => Some(m.date),
                _ => None,
            })
            .unwrap_or(offset_date);
        offset_peer = input_peer_for(&last.peer, &users, &chats);

        // Stop when the page was short or made no progress.
        if !maybe_more || out.len() == before {
            break;
        }
    }
    out
}

fn peer_bot_id(peer: &tl::enums::Peer) -> i64 {
    use tl::enums::Peer;
    match peer {
        Peer::User(p) => p.user_id,
        Peer::Chat(p) => -p.chat_id,
        Peer::Channel(p) => -(1_000_000_000_000 + p.channel_id),
    }
}

/// Resolve a raw `Peer` to an `Entry` using the page's user/chat lists.
fn build_peer_entry(
    peer: &tl::enums::Peer,
    users: &[tl::enums::User],
    chats: &[tl::enums::Chat],
) -> Option<Entry> {
    use tl::enums::{Chat as TlChat, Peer, User as TlUser};
    match peer {
        Peer::User(p) => {
            let u = users.iter().find_map(|u| match u {
                TlUser::User(u) if u.id == p.user_id => Some(u),
                _ => None,
            })?;
            if u.deleted {
                return None;
            }
            let name = [u.first_name.as_deref(), u.last_name.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            Some(Entry {
                name: if name.is_empty() {
                    u.username.clone().unwrap_or_else(|| "(no name)".into())
                } else {
                    name
                },
                username: u.username.clone(),
                id: u.id,
                kind: "user".into(),
                topics: Vec::new(),
            })
        }
        Peer::Chat(p) => {
            let c = chats.iter().find_map(|c| match c {
                TlChat::Chat(c) if c.id == p.chat_id => Some(c),
                _ => None,
            })?;
            Some(Entry {
                name: c.title.clone(),
                username: None,
                id: -c.id,
                kind: "group".into(),
                topics: Vec::new(),
            })
        }
        Peer::Channel(p) => {
            let c = chats.iter().find_map(|c| match c {
                TlChat::Channel(c) if c.id == p.channel_id => Some(c),
                _ => None,
            })?;
            Some(Entry {
                name: c.title.clone(),
                username: c.username.clone(),
                id: -(1_000_000_000_000 + c.id),
                kind: if c.broadcast { "channel" } else { "group" }.into(),
                topics: Vec::new(),
            })
        }
    }
}

/// Build an `InputPeer` (with access hash) for use as a paging offset.
fn input_peer_for(
    peer: &tl::enums::Peer,
    users: &[tl::enums::User],
    chats: &[tl::enums::Chat],
) -> tl::enums::InputPeer {
    use tl::enums::{Chat as TlChat, InputPeer, Peer, User as TlUser};
    match peer {
        Peer::User(p) => {
            let access_hash = users
                .iter()
                .find_map(|u| match u {
                    TlUser::User(u) if u.id == p.user_id => u.access_hash,
                    _ => None,
                })
                .unwrap_or(0);
            InputPeer::User(tl::types::InputPeerUser {
                user_id: p.user_id,
                access_hash,
            })
        }
        Peer::Chat(p) => InputPeer::Chat(tl::types::InputPeerChat { chat_id: p.chat_id }),
        Peer::Channel(p) => {
            let access_hash = chats
                .iter()
                .find_map(|c| match c {
                    TlChat::Channel(c) if c.id == p.channel_id => c.access_hash,
                    _ => None,
                })
                .unwrap_or(0);
            InputPeer::Channel(tl::types::InputPeerChannel {
                channel_id: p.channel_id,
                access_hash,
            })
        }
    }
}

/// Fetch the account's contacts as user entries (best-effort; empty on error).
async fn fetch_contacts(client: &Client) -> Vec<Entry> {
    let request = tl::functions::contacts::GetContacts { hash: 0 };
    let users = match client.invoke(&request).await {
        Ok(tl::enums::contacts::Contacts::Contacts(c)) => c.users,
        Ok(tl::enums::contacts::Contacts::NotModified) => return Vec::new(),
        Err(e) => {
            eprintln!("warning: could not fetch contacts: {e:?}");
            return Vec::new();
        }
    };
    users
        .into_iter()
        .filter_map(|u| {
            let tl::enums::User::User(user) = u else {
                return None;
            };
            if user.deleted {
                return None;
            }
            let name = [user.first_name.as_deref(), user.last_name.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            let name = if name.is_empty() {
                user.username.clone().unwrap_or_else(|| "(no name)".to_string())
            } else {
                name
            };
            Some(Entry {
                name,
                username: user.username,
                id: user.id,
                kind: "user".to_string(),
                topics: Vec::new(),
            })
        })
        .collect()
}

/// If `peer` is a forum supergroup, return an `InputPeer` for topic queries.
fn forum_input_peer(peer: &Peer) -> Option<tl::enums::InputPeer> {
    let Peer::Group(group) = peer else {
        return None;
    };
    let tl::enums::Chat::Channel(channel) = &group.raw else {
        return None;
    };
    if !channel.forum {
        return None;
    }
    Some(tl::enums::InputPeer::Channel(tl::types::InputPeerChannel {
        channel_id: channel.id,
        access_hash: channel.access_hash.unwrap_or(0),
    }))
}

/// Fetch a forum's topics (best-effort; returns empty on error).
async fn fetch_topics(client: &Client, peer: tl::enums::InputPeer) -> Vec<Topic> {
    let request = tl::functions::messages::GetForumTopics {
        peer,
        q: None,
        offset_date: 0,
        offset_id: 0,
        offset_topic: 0,
        limit: 100,
    };
    match client.invoke(&request).await {
        Ok(tl::enums::messages::ForumTopics::Topics(result)) => result
            .topics
            .into_iter()
            .filter_map(|t| match t {
                tl::enums::ForumTopic::Topic(topic) => Some(Topic {
                    id: topic.id as i64,
                    title: topic.title,
                }),
                tl::enums::ForumTopic::Deleted(_) => None,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line)
}
