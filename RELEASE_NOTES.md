# Multiapp v0.2.0

Run multiple **isolated profiles of the same macOS app** at the same time — separate logins,
cookies, settings and caches — without modifying, copying or re-signing the app itself.

## Install

1. Download `Multiapp-0.2.0.dmg` below, open it, drag **Multiapp** to Applications.
2. **First launch:** right-click the app → **Open** → **Open**.
   macOS shows an "unidentified developer" warning because this build is *ad-hoc signed*
   (no paid Apple Developer ID). This is expected — the warning appears only once.
3. Optional CLI: `ln -s "/path/to/multiapp" ~/.local/bin/multiapp`

## What it does

- **Menu-bar app** — profiles per app with running state; launch / stop / rename / clone /
  export / delete; import profiles; app rescan.
- **CLI** (`multiapp`) — same engine: `apps · scan · new · launch · list · stop · clone ·
  rename · delete · trash · wrapper · probe · doctor`.
- **Claude Code sessions** — list and transfer sessions between any two profiles, or export /
  import them to move work between Macs.

## How it works

`open -n <App.app> --args --user-data-dir=<profile dir>` — the **equals form is required**;
the space-separated form is silently ignored by Chromium's switch parsing. Nothing inside the
target app is touched, so its code signature, notarization and auto-updater keep working.

## Compatibility

| Status | Apps |
|---|---|
| **Supported** (verified) | Claude, Notion, Notion Calendar, GitHub Desktop, OpenMTP, VS Code, Antigravity, Google Chrome |
| **Partial** | ChatGPT — data isolates, but the *account* is shared (identity lives in Keychain). Don't sign out inside a profile. |
| **Unsupported** | Gemini, ChatGPT Classic (native apps, no isolation lever) · Telegram (sandboxed container is bundle-ID-keyed) · apps that override their own `userData` path |

Verified across Electron 18–43 — there is no version floor, but an app can override the switch
in its own code, so `multiapp probe <app>` canary-tests any newly scanned app before you trust it.

## Honest limitations

- Profiles are **convenience isolation, not a security boundary** — same user account, same TCC
  permission grants, same Keychain.
- Apps whose login lives in the **Keychain** share that login across profiles.
- **Sandboxed** (Mac App Store) apps cannot be profiled this way at all.
- Profile folders contain live session cookies — treat exports as sensitive.
- Windows (`multiapp.ps1`) and Linux paths are implemented but **not yet tested on real hardware**.
