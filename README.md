# telepad

A **rofi quick-switcher for Telegram** — hit a hotkey from anywhere, fuzzy-type a
few letters, and your running [AyuGram Desktop](https://github.com/AyuGram/AyuGramDesktop)
jumps straight to that chat, group, or channel — **across all your accounts**.

Think Discord's <kbd>Ctrl</kbd>+<kbd>K</kbd>, but as a global rofi menu that drives
the desktop app you already have open.

> **Status: early / works-for-me.** Same-account jumping is solid. Cross-account
> jumping works via injected key switches (see below). Forum *topics* aren't indexed
> yet (see [Roadmap](#roadmap)).

## How it works

telepad has two halves that talk through a small on-disk cache:

```
┌─ data layer (grammers / MTProto) ─┐      ┌─ front-end (rofi) ─────────────────┐
│ per account: log in once,          │      │ flat fuzzy list across accounts    │
│ fetch dialog list → JSON cache     │─────▶│ → (xdotool switch account)         │
└────────────────────────────────────┘ cache│ → D-Bus Open → AyuGram navigates  │
                                            └────────────────────────────────────┘
```

- **Reading your chats** uses [grammers](https://github.com/Lonami/grammers), a Rust
  MTProto client. This is a *separate* login from AyuGram (it can't read AyuGram's
  encrypted `tdata`), so each account authenticates once and its session is cached.
- **Navigating** hands a `tg://` link to the running AyuGram **in-process** over
  D-Bus (`org.freedesktop.Application.Open` on `com.ayugram.desktop`). Public chats
  resolve by `@username`; anything else (private groups/channels, username-less DMs)
  opens by internal peer id via AyuGram's `tg://chat?id=` handler.
- **Switching accounts** focuses the AyuGram window and injects the account-switch
  key you bound inside AyuGram (e.g. `alt+1`), via `xdotool`.

### Why not `xdg-open` / `tg://…&acc=`?

Two things that *seem* obvious don't work reliably:

- `xdg-open tg://…` spawns a **second** AyuGram process; its single-instance handoff
  can race and kill the running window. The in-process D-Bus `Open` avoids spawning
  anything.
- The `tg://…&acc=N` deep-link account switch **crashes** AyuGram (a bug in the
  account-switch-from-URL path; on Nix builds it dies silently with no crash dump).
  So account switching is done with a real keypress instead — the same switch you'd
  do by hand, which is safe.

## Requirements

- **Rust** (to build)
- **rofi** — the menu
- **gdbus** (glibc / GLib) — delivers the URL to AyuGram
- **xdotool** + an **X11** session — only needed for cross-account switching

## Install

```bash
cargo build --release
# copy target/release/telepad somewhere on your PATH
```

## Setup

1. **API credentials** (free): https://my.telegram.org → API development tools → note
   your `api_id` and `api_hash`.

2. **Bind account-switch keys in AyuGram** (needed for cross-account jumping):
   AyuGram Settings → Advanced → keyboard shortcuts, bind command `account1` to a key
   (e.g. `alt+1`), `account2` to `alt+2`, etc.

3. **Configure telepad:**
   ```bash
   mkdir -p ~/.config/telepad
   cp config.example.toml ~/.config/telepad/config.toml
   $EDITOR ~/.config/telepad/config.toml
   ```
   Set `api_id`/`api_hash` and one `[[accounts]]` block per account, giving each the
   `switch_key` you bound in AyuGram.

4. **Log in each account once:**
   ```bash
   telepad login personal
   ```
   > The login code arrives **inside your already-logged-in Telegram/AyuGram** (the
   > Telegram service chat), *not* by SMS. If the account has 2FA you'll be prompted
   > for the password too.

5. **Build the chat cache:**
   ```bash
   telepad sync            # all accounts
   telepad sync work       # just one
   ```

6. **Bind the menu to a key** (e.g. i3):
   ```
   bindsym $mod+k exec --no-startup-id telepad
   ```

## Usage

```bash
telepad                 # show the quick-switcher (flat, all accounts)
telepad menu            # same thing
telepad sync [acct]     # refresh the chat cache (run periodically / via cron)
telepad login <acct>    # (re)authenticate an account
```

Each row is tagged `[Account]`, so you can scope a search by typing the account name
(`work signals`) or just jump by chat name (`signals`).

## Config reference

| Field | Meaning |
|-------|---------|
| `api_id` / `api_hash` | One app from my.telegram.org, shared by all accounts |
| `window_class` | X11 class of AyuGram, focused before injecting the switch key |
| `accounts[].acc` | 1-based slot in AyuGram's account list (display/ordering) |
| `accounts[].label` | Name shown in rofi |
| `accounts[].session` | Session file name under `~/.local/share/telepad/` |
| `accounts[].phone` | International format; used only for `telepad login` |
| `accounts[].switch_key` | xdotool key that switches to this account (e.g. `alt+1`); empty = never switch |

## Limitations

- **Cross-account needs X11 + xdotool.** Switching is a synthetic keypress, so it
  needs the key bound in AyuGram and depends on window focus/timing. On Wayland
  you'd need a different injector (ydotool). Leave `switch_key` empty to stay
  same-account only (rock-solid).
- **Separate sessions.** grammers logs in independently of AyuGram, so each account
  shows up as an extra device in your Telegram sessions list.
- **Stale cache.** The list is a snapshot; re-run `telepad sync` (a cron job or a
  `sync && menu` wrapper works well) to pick up new chats.

## Roadmap

- **Forum topics** as first-class jump targets (AyuGram already accepts `&topic=<id>`;
  needs a `channels.getForumTopics` fetch in the sync step).
- Optional `sync`-on-open so the menu is always fresh.

## Prior art

Supersedes two earlier personal experiments (`tg-rofi`, `rofi-tg-switcher`).

## License

MIT OR Apache-2.0.
