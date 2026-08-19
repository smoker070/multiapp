# Multiapp v0.3.0

Run **multiple isolated profiles of the same app** (separate logins, cookies, settings), and **back up
or move an app's data and login sessions** — without modifying, copying or re-signing any app.

## Install (macOS)

1. Download `Multiapp-0.3.0.dmg` below, open it, drag **Multiapp** to Applications.
2. **First launch:** right-click the app → **Open** → **Open**.
   macOS shows an "unidentified developer" warning because this build is *ad-hoc signed*
   (no paid Apple Developer ID). Expected — it appears once.
3. Optional CLI: run `./multiapp install-stub` once; it puts a self-healing `multiapp` on your PATH.

## What's new since v0.2.0

**App backup & restore**
- `migrate-list` · `backup` · `restore` — archive an app's real local data and put it back.
- Restores **merge** rather than replace. This matters: an archive made without caches is an
  *incomplete* copy, and the previous behaviour deleted everything the archive had skipped
  (found on Telegram: 18,918 files → 521). Anything replaced is staged to Trash first.
- Works for **any installed app**, not just a curated list — apps you install later show up
  automatically.

**Login sessions**
- `session-check` · `session-backup` · `session-restore` — save just the login (KB-sized) so you
  don't have to sign in again, for **any** app.
- `session-check` reports honestly whether an app's login is even *in* those files. Verified by
  experiment: deleting every session file left Gemini still signed in, because its token lives in
  the Keychain. The tool says so instead of pretending.

**Export / import any app**
- `app-export` · `app-import` and menu-bar **Export/Import App Data** — move an app's work sessions
  to another machine. Finds home dotfolders too (`~/.codex` holds 625 MB of Codex sessions,
  `~/.claude` about 1 GB) which a `~/Library`-only sweep misses entirely.

**Claude Code sessions**
- `sessions` · `transfer` · `export` · `import`. `transfer` makes a **true copy** — new session ids and
  a duplicated transcript — because transcripts live in `~/.claude`, outside the profile. Copying only
  the index left two accounts reading and writing one live transcript.

**Also**
- Telegram Desktop added as its own app, separate from Telegram for macOS (separate logins).
- The backup list now shows only apps that actually hold a login.
- Menu opens in 0.16 s instead of 5.3 s (the list is cached).
- Profile names may contain spaces and underscores.

## Compatibility

| Status | Apps |
|---|---|
| **Profiles supported** (verified) | Claude, Notion, Notion Calendar, GitHub Desktop, OpenMTP, VS Code, Antigravity, Google Chrome |
| **Profiles partial** | ChatGPT — data isolates, but the *account* is shared (identity is in the Keychain). Don't sign out inside a profile. |
| **No profiles** | Gemini, ChatGPT Classic (native, no isolation lever) · Telegram (sandboxed) · apps that override their own `userData` path |
| **Backup / sessions** | any installed app that holds a login — including the "no profiles" ones |

## Honest limitations

- Profiles are **convenience isolation, not a security boundary** — same user, same Keychain, same
  permission grants.
- A restored login only works **where its Keychain is**. Same Mac with the Keychain intact → login
  comes back. New Mac, new user, or a fresh macOS install (which wipes the Keychain) → your history
  and settings restore and **you sign in once**. Multiapp never touches Keychain items; that is
  exactly why logins don't travel.
- Backup archives contain live session data — treat them like passwords.
- **Windows and Linux are not verified on real hardware.** See `WINDOWS.md` before relying on the
  Windows build.
- The macOS app is **ad-hoc signed**, so Gatekeeper warns on first open.
