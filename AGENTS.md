# AGENTS.md - Developer Guide

Guidelines for agentic coding agents working in this repository.

## Project Overview

`telepad` is a rofi quick-switcher that jumps AyuGram / Telegram Desktop to any chat,
group, or channel across all of the user's accounts. It reads the chat list with
[grammers](https://github.com/Lonami/grammers) (MTProto) into a JSON cache, and
navigates by handing `tg://` deep links to the running AyuGram instance via `xdg-open`.
Account switching uses the `acc=` deep-link parameter (no D-Bus).

## Build Commands

```bash
cargo build            # debug
cargo build --release  # production
cargo run -- sync      # run a subcommand
```

## Lint / Format

```bash
cargo fmt
cargo fmt --check
cargo clippy           # requires the clippy component
cargo check
```

## Architecture

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry + subcommand dispatch (`login`, `sync`, `menu`) |
| `src/config.rs` | TOML config + path helpers (config / data / session / cache) |
| `src/telegram.rs` | grammers connect, interactive login, dialog fetch |
| `src/cache.rs` | Per-account JSON dialog cache (read/write) |
| `src/rofi.rs` | rofi `-dmenu -format i` wrapper → selected index |
| `src/link.rs` | Build `tg://` deep links + `xdg-open` |

Data flow: `telegram::fetch_dialogs` → `cache::write` → (later) `menu` reads all
caches, flattens to a rofi list, maps the picked index to a `tg://` URL.

## grammers 0.10 notes

The 0.10 API differs from older tutorials. Key facts (verified against the crate
source), so don't "fix" them back to the old shapes:

- Connect: `SqliteSession::open(path).await` → `SenderPool::new(Arc::new(session), api_id)`,
  then destructure `{ runner, handle, updates }`, `Client::new(handle)`, and
  `tokio::spawn(runner.run())`. The `updates` receiver **must** be drained.
- Auth: `client.request_login_code(phone, api_hash)` → `client.sign_in(&token, code)`;
  handle `SignInError::PasswordRequired(token)` with `client.check_password(token, pw)`.
- Dialogs: `client.iter_dialogs()` yields `Dialog { peer: Peer, .. }`. `Peer` is an
  enum `User | Group | Channel` with `.name()`, `.username()`, `.id()` (a `PeerId`
  whose `Display` is the Bot-API id — positive for users).

## Code Style

- `anyhow::Result<T>` + `?` for errors; `anyhow!` / `.context()` for messages.
- `snake_case` fns/vars, `PascalCase` types, `SCREAMING_SNAKE_CASE` consts.
- Group imports: external crates, then std, then local; alphabetical within a group.
- Keep dependencies minimal (no clap — args parsed by hand in `main.rs`).
- `#[tokio::main]` async entry; grammers calls are async.

## Conventions specific to this project

- **Navigation is in-process D-Bus**, not `xdg-open`. `link::open` calls
  `org.freedesktop.Application.Open` on `com.ayugram.desktop`. Do not switch to
  `xdg-open` or a direct `AyuGram -- <url>` exec: both spawn a second process whose
  single-instance handoff can kill the running primary on some builds.
- **Account switching is a synthetic keypress**, not the `tg://…&acc=` param. The
  deep-link `acc=` switch crashes AyuGram (silent, no dump on Nix). `link::switch_account`
  focuses the window and injects `accounts[].switch_key` via `xdotool`. Never
  reintroduce `acc=` into built URLs.
- **URLs carry no account** (`link::build` → one URL): public peers use
  `tg://resolve?domain=`, everything else uses `tg://chat?id=<PeerId>`. The PeerId is
  tdesktop's internal packed id (`bare | (shift<<48)`; shift 0/1/2 = user/chat/channel),
  computed from the Bot-API id in `tdesktop_peer_id`.
- **`acc` is 1-based** and mirrors AyuGram's account order (display/ordering only).
- Sessions and caches live under `~/.local/share/telepad/`, never in the repo.
