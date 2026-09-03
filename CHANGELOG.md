# Changelog

All notable changes to telepad are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.0] - 2026-09-03

### Added
- **Configurable client.** A new `client` setting (`"telegram"` or `"ayugram"`)
  selects the D-Bus service name and X11 window class telepad drives, so it's no
  longer hardwired to AyuGram. Vanilla Telegram Desktop is now the default. Other
  TDesktop forks work by overriding `dbus_service` (the object path is derived
  from it) and `window_class` directly.

### Changed
- **Default client is now vanilla Telegram Desktop.** The D-Bus name defaults to
  `org.telegram.desktop` and the window class to `TelegramDesktop`. **AyuGram
  users must add `client = "ayugram"` to their config** (previously the AyuGram
  values were the built-in defaults).

### Notes
- Only the client's D-Bus name and window class are switched by `client`; the
  `tg://` handlers themselves still differ between builds. AyuGram's `tg://chat?id=`
  open (used for username-less users/legacy groups) may not exist on vanilla
  Telegram Desktop — public `@username` peers and private channels (`privatepost`)
  should work on both.

## [0.8.2] - 2026-09-02

### Fixed
- **Jumping to username-less channels/supergroups.** These now open via
  `tg://privatepost?channel=<raw>` instead of `tg://chat?id=<packed>`. AyuGram's
  `chat?id=` fallback is broken for channels that aren't currently loaded (it
  re-prepends `-100` to the already-packed PeerId), so jumps to less-active
  private channels silently failed. `privatepost` routes through `showPeerByLink`,
  which resolves the channel via the API whether or not it's loaded.

## [0.8.1] - 2026-09-02

### Added
- **Placeholder avatar.** Rows without a cached photo get a neutral silhouette
  icon, so the list looks even once avatars are present. Only applied when at
  least one avatar exists — a menu that never ran `sync --avatars` stays a clean
  icon-less text list.

## [0.8.0] - 2026-09-02

### Added
- **Avatars (opt-in).** `sync --avatars` downloads each main dialog's profile
  photo into the avatar cache, and the menu shows it as the rofi row icon (rofi
  now runs with `-show-icons`). Downloads are best-effort — peers without a photo
  are skipped and a failed download warns without aborting the sync. Plain `sync`
  is unchanged and pays no download cost.

### Notes
- Only main dialogs get avatars (that's where full peer objects are available);
  contacts without a chat and archived chats don't. Icons appear in the main flat
  list, not the folder/archive submenus. This is the one rofi-specific feature.

## [0.7.0] - 2026-09-02

### Added
- **Saved Messages row.** A guaranteed `⭐ Saved Messages` entry per account.
  `sync` now captures each account's own user id and `@username` (`get_me`), and
  the menu renders the self-chat as a normal jump target — so it switches to the
  right account and participates in frecency ranking.

### Changed
- The self-chat is de-duplicated out of the normal dialog list (no more a
  `YourName · dm` row alongside `⭐ Saved Messages`).

### Notes
- Requires a re-sync (`telepad sync`) to populate the new self identity; caches
  from older versions simply omit the row until then.

## [0.6.0] - 2026-09-02

### Added
- **Frecency ranking.** Every jump is recorded to `frecency.json` (peer id →
  visit count + last-used, with a 30-day recency half-life), and the flat list is
  stable-sorted by that score so your most-used chats float to the top. Recording
  is best-effort and never blocks or fails an open; never-jumped rows keep their
  prior order.

## [0.5.0] - 2026-09-02

### Added
- **`@handle` in rows.** Peers with a public username show their `@handle` as a
  searchable segment, so rofi's fuzzy match finds them by display name *or*
  username. Rendered in the flat list and the archive/folder submenus. No re-sync
  needed — the username was already cached.

### Documentation
- Caveat that fancy Unicode "font" names (styled math/pseudo-font codepoints)
  won't match a normal-text fuzzy search.

## [0.4.0] - 2026-09-02

### Added
- **Chat folders.** Each Telegram folder (dialog filter) shows as a
  `📁 <Folder> ▸` row that expands into a submenu of that folder's chats.

## [0.3.0] - 2026-09-02

### Added
- **Flat forum topics.** Every forum topic is surfaced as its own searchable
  `Forum ▸ Topic` row in the main menu, so you can jump straight to a topic on the
  first keystroke (selecting the forum itself still opens its topic submenu).

## [0.2.0] - 2026-09-02

### Added
- **Archive browsing.** A `🗄 Archived` entry per account: open the archive folder
  view, or jump straight to a specific archived chat.

### Chores
- Ignore real config, session, and cache files.

## [0.1.0] - 2026-09-01

Initial release: a rofi quick-switcher that jumps AyuGram to any chat across all
your accounts.

### Added
- Fuzzy jump to any chat, group, channel, or contact — across all accounts in one
  flat list, including contacts you have no open chat with (e.g. blocked ones).
- Forum topics as two-level jump targets (forum → topic submenu).
- Cross-account switching: focuses the client and injects the account's bound
  switch key via `xdotool` before opening (the safe path; the deep-link `acc=`
  switch crashes AyuGram).
- In-process navigation over D-Bus (`org.freedesktop.Application.Open` on
  `com.ayugram.desktop`), avoiding the second-process race that kills the window.
- `login` / `sync` / `menu` commands, with the dialog list cached to JSON.

[0.9.0]: https://github.com/antlis/telepad/releases/tag/v0.9.0
[0.8.2]: https://github.com/antlis/telepad/releases/tag/v0.8.2
[0.8.1]: https://github.com/antlis/telepad/releases/tag/v0.8.1
[0.8.0]: https://github.com/antlis/telepad/releases/tag/v0.8.0
[0.7.0]: https://github.com/antlis/telepad/releases/tag/v0.7.0
[0.6.0]: https://github.com/antlis/telepad/releases/tag/v0.6.0
[0.5.0]: https://github.com/antlis/telepad/releases/tag/v0.5.0
[0.4.0]: https://github.com/antlis/telepad/releases/tag/v0.4.0
[0.3.0]: https://github.com/antlis/telepad/releases/tag/v0.3.0
[0.2.0]: https://github.com/antlis/telepad/releases/tag/v0.2.0
[0.1.0]: https://github.com/antlis/telepad/releases/tag/v0.1.0
