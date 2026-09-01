# telepad

A **rofi quick-switcher for Telegram** — hit a hotkey from anywhere, fuzzy-type a
few letters, and your running [AyuGram Desktop](https://github.com/AyuGram/AyuGramDesktop)
jumps straight to that chat, group, channel, contact, or **forum topic** —
**across all your accounts**.

It's Discord's <kbd>Ctrl</kbd>+<kbd>K</kbd> quick-switcher, reimagined for the desktop
Telegram client — and arguably better. Because it's a global **rofi + i3** binding,
you trigger it from *any* window: your editor, terminal, browser, anything. No need to
first focus the app. Where Discord makes you *focus Discord → Ctrl+K → type → jump*,
telepad is just *hotkey → type → jump* from wherever you already are — fewer keystrokes,
zero context switch.

```
┌─ data layer (grammers / MTProto) ─┐        ┌─ front-end (rofi) ─────────────────┐
│ per account, once: log in         │        │ flat fuzzy list across accounts    │
│ sync: dialogs + contacts + forum  │ ─cache→ │  ↳ forum? → topic submenu          │
│       topics → JSON cache          │        │ → (xdotool: switch account)        │
└────────────────────────────────────┘        │ → D-Bus Open → AyuGram navigates  │
                                              └────────────────────────────────────┘
```

> **Status:** works day-to-day for the author on X11 + i3 + AyuGram. Rough edges
> remain — see [Caveats](#caveats) and [Roadmap](#roadmap). Notably it has **only
> been tested against AyuGram**, not vanilla Telegram Desktop yet.

## Features

- Fuzzy jump to any **chat, group, channel, or contact**, across all accounts
- **Forum topics**: selecting a forum opens a second menu of its topics
- **Archive**: a `🗄 Archived` entry per account — open the archive folder, or jump
  straight to any archived chat
- **Contacts included** — even blocked users or people you have no open chat with
- **Cross-account**: switches to the target account before opening
- Fullscreen rofi menu; simple `login` / `sync` / `menu` commands

## How it works

Two halves that talk through a small on-disk cache:

- **Reading your chats** uses [grammers](https://github.com/Lonami/grammers), a Rust
  MTProto client. This is a **separate** login from AyuGram (it can't read AyuGram's
  encrypted `tdata`), so each account authenticates once and its session is cached.
  `sync` pulls the dialog list, your contacts, and each forum's topics into a JSON
  cache the menu reads.
- **Navigating** hands a `tg://` link to the running client **in-process** over
  D-Bus (`org.freedesktop.Application.Open` on `com.ayugram.desktop`). Public peers
  resolve by `@username`; everything else (private groups/channels, username-less
  DMs) opens by internal peer id via the `tg://chat?id=` handler; forum topics use
  `resolve?domain=X&topic=` (public) or `privatepost?channel=<raw>&topic=` (private).

### Account switching (the ugly part)

Switching account is done by **injecting the keypress you bound in the client**
(e.g. `alt+2`) via `xdotool`, *not* by any `tg://` parameter. This is deliberate and
worth understanding:

- `tg://…&acc=N` **crashes** AyuGram. The deep-link account-switch path segfaults in
  the builds tested (on Nix it dies silently — no crash dump, truncated log).
- `xdg-open tg://…` spawns a **second** client process; its single-instance handoff
  races and can kill the running window. So navigation uses the in-process D-Bus
  `Open` instead, which never spawns anything.
- That leaves no safe URL-based way to switch accounts, so telepad drives the
  client's own account-switch shortcut with a real keypress — the same switch you'd
  do by hand, which is safe.

Consequently, cross-account jumping needs: **X11**, **xdotool**, a window-focus
command (default `i3-msg`), and the account-switch keys bound **inside** the client.
Leave `switch_key` empty to stay same-account only (rock-solid, no xdotool needed).

## Requirements

- **Rust** (to build)
- **rofi** — the menu
- **gdbus** (GLib) — delivers the URL to the running client
- **xdotool** + an **X11** session — only for cross-account switching
- a focuser — default `i3-msg` (override `focus_cmd` for other WMs)

## Install

```bash
cargo build --release
# put target/release/telepad on your PATH
```

## Setup

1. **API credentials** (free): https://my.telegram.org → API development tools → note
   your `api_id` and `api_hash`.
2. **Bind account-switch keys in the client** (for cross-account): AyuGram/Telegram
   Desktop → Settings → Advanced → keyboard shortcuts → bind `account1`, `account2`, …
   to keys (e.g. `alt+1`, `alt+2`).
3. **Configure telepad:**
   ```bash
   mkdir -p ~/.config/telepad
   cp config.example.toml ~/.config/telepad/config.toml
   $EDITOR ~/.config/telepad/config.toml
   ```
   Set `api_id`/`api_hash` and one `[[accounts]]` block per account, giving each its
   `switch_key`.
4. **Log in each account once:**
   ```bash
   telepad login personal
   ```
   > The login code arrives **inside your already-logged-in Telegram/AyuGram** (the
   > Telegram service chat), *not* by SMS. 2FA password is prompted if set.
5. **Build the cache:**
   ```bash
   telepad sync            # all accounts (dialogs + contacts + forum topics)
   ```
6. **Bind the menu to a key** (i3 example): `bindsym $mod+g exec --no-startup-id telepad`

## Usage

```bash
telepad                 # the quick-switcher (flat, all accounts)
telepad menu            # same thing
telepad sync [acct]     # refresh the cache (run periodically / via cron)
telepad login <acct>    # (re)authenticate an account
```

Rows are tagged `[Account]`, so type an account name to scope (`work signals`) or
just the chat name (`signals`). Forum rows show `forum ▸`.

## Config reference

| Field | Meaning |
|-------|---------|
| `api_id` / `api_hash` | One app from my.telegram.org, shared by all accounts |
| `window_class` | X11 class of the client, focused before the switch key |
| `focus_cmd` | Shell command to focus the client; `{class}` is substituted. Default `i3-msg [class="{class}"] focus` |
| `accounts[].acc` | 1-based slot in the client's account list (display/ordering) |
| `accounts[].label` | Name shown in rofi |
| `accounts[].session` | Session file name under `~/.local/share/telepad/` |
| `accounts[].phone` | International format; used only for `telepad login` |
| `accounts[].switch_key` | xdotool key that switches to this account (e.g. `alt+1`); empty = never switch |

## Caveats

- **Only tested with AyuGram**, not vanilla Telegram Desktop. The `tg://` handlers and
  the `com.ayugram.desktop` D-Bus name are AyuGram-specific; Telegram Desktop uses
  `org.telegram.desktop` and lacks AyuGram's `tg://chat?id=` handler. See the TODO.
- **Cross-account needs X11 + xdotool** and depends on window focus/timing. On Wayland
  you'd need a different injector (ydotool). Same-account is dependency-light.
- **Separate sessions**: grammers logs in independently, so each account shows as an
  extra device in your Telegram sessions list.
- **Stale cache**: the list is a snapshot; re-run `telepad sync` to pick up new chats.
- **"Recents" that aren't saved contacts** won't appear (only dialogs + saved
  contacts are indexed).

## Roadmap / TODO

- [ ] **Test against vanilla Telegram Desktop** — make the D-Bus name, `tg://` scheme,
      and id-based open configurable so it isn't AyuGram-only.
- [ ] **Reduce dependencies / decouple from rofi + i3** — ideally a single
      self-contained binary with its own minimal picker (or a pluggable menu backend)
      and a WM-agnostic focus/switch mechanism, so it isn't tied to rofi, i3, and
      xdotool. Goal: one package, few/no external CLI deps.
- [ ] Investigate a non-crashing account switch (upstream fix to the `tg://…&acc=`
      handler would remove the whole xdotool detour).
- [ ] Include recent/top peers (`contacts.getTopPeers`) for non-contact recents.
- [ ] Optional `sync`-on-open so the menu is always fresh.

## Prior art

Supersedes two earlier personal experiments (`tg-rofi`, `rofi-tg-switcher`).

## License

MIT OR Apache-2.0.
