First public release. Run several independent profiles of the same desktop app — separate logins, cookies and settings — without modifying, copying or re-signing the app.

Two different programs ship here, and it matters which you download:

- **`Multiapp-0.3.0.dmg`** — the macOS app and the full 27-command CLI. This is the one to get on a Mac.
- **`multiapp-*`** — the portable Rust CLI, which currently has only `new · launch · list · stop · where`. It is the Windows and Linux story; it is **not** the full tool yet.

The DMG is ad-hoc signed, so macOS warns on first open: right-click → **Open** → **Open**. Notarisation needs a paid Apple Developer ID ([#1](../../issues)).

### Profiles

- feat: create, launch, list, stop, clone, rename and delete isolated profiles of an installed app
- feat: `scan` discovers Electron/Chromium apps automatically, so apps installed later work without a code change
- feat: `probe` is a 10-second canary that checks whether an app actually honoured the flag, and records the verdict
- feat: clickable per-profile launchers — a macOS `.app` applet carrying the real app's icon, a Linux `.desktop` entry, a Windows Start-Menu shortcut
- feat: profile names may contain spaces and underscores
- fix: a profile whose name is a prefix of another was reported as running when the other was launched — `work` matched `work2`. Both implementations now compare the whole `--user-data-dir` value

### macOS app

- feat(macos): menu-bar app with live running state, launch/stop, rename/clone/delete, and a designed DMG
- feat(macos): **Back Up & Restore** and **Export/Import App Data** in the menu
- feat(macos): each app in the backup list now shows what is actually saved for it, plus a **What's Saved?…** report
- fix(macos): the menu took 5.3 s to open and now takes 0.16 s — the app list is cached instead of rescanned on every click
- fix(macos): `launch` no longer waits on `open` forever. It gives up after 30 s and reports a stuck LaunchServices, which is what a wedged window server actually deserves

### Backup, restore and login sessions

- feat: `backup` / `restore` archive an app's real local data and put it back
- feat: `session-backup` / `session-restore` save just the **login** — kilobytes, not gigabytes — for any installed app, including ones that cannot be profiled at all
- feat: `session-check` reports what session data an app holds, which **sites** its cookies belong to, and whether any of it would survive a move to another Mac
- feat: `app-export` / `app-import` move an app's local data between machines, including home dotfolders like `~/.codex` and `~/.claude` that a `~/Library`-only sweep misses
- feat: Telegram Desktop is handled as its own app, separate from Telegram for macOS — different login, different store
- **fix: restores now merge instead of replacing.** An archive made without caches is an *incomplete* copy, and the old behaviour deleted everything it had skipped. Found on Telegram: 18,918 files down to 521. Anything displaced is staged to a recoverable Trash first
- fix: `session-backup` missed Chromium **partitioned** cookie stores. Notion keeps its real session in `Partitions/notion/Cookies` while its top-level store sits empty, so the archive captured nothing and restored no login
- fix: cookie classification read only `encrypted_value`, so an app storing cookies in the clear looked like an empty store
- fix: the backup list called everything a "login". It now reports what was found — `cookies (encrypted)`, `account file`, `login lives in the Keychain` — and does not claim to know whether you have an account there, because that needs the cookie values and those are never read

### Claude Code sessions

- feat: `sessions` / `transfer` / `export` / `import` for Claude Code sessions
- fix: `transfer` now makes a **true copy** with new session ids and a duplicated transcript. Copying only the index left two accounts reading and writing one live transcript, because transcripts live in `~/.claude`, outside the profile

### Portable CLI (Windows and Linux)

- feat(cli): a Rust `multiapp-core` + `multiapp-cli` — one codebase for macOS, Windows and Linux
- feat(cli): Windows uses **LocalAppData**, not Roaming. Profiles run to gigabytes and Roaming is synced by enterprise roaming-profile policy
- feat(cli): graceful stop repeats the quit request rather than signalling once — an Electron app still starting up drops the first signal silently
- fix(windows): app lookup now finds Chromium browsers. Chrome lives at `Google\Chrome\Application\chrome.exe`, which the `<Name>\<Name>.exe` pattern never matched
- fix(windows): a lint error made the Windows build fail outright; the platform-specific code is now compiled and type-checked on every OS, so this cannot be discovered only on Windows again

### Security and safety

- security: Multiapp never reads, writes, exports or decrypts anything in the macOS Keychain or Windows Credential Manager. Logins therefore do not travel to another machine, and the tool says so instead of pretending
- security: cookie **values** are never read. `session-check` shows host names only
- security: session and backup archives can contain a live login. The tool warns that they should be treated like a password
- security: nothing here modifies, re-signs or patches an application, and nothing circumvents licensing, account limits, authentication or integrity checks — a documented Chromium flag is passed to an unmodified binary
- feat: type-to-confirm deletes, a staged Trash instead of `rm`, and a containment guard that refuses any path outside Multiapp's own root

### Verified

- The Rust CLI is exercised on every push against a **real Chromium app**: Edge on `windows-latest`, headless Chrome on `ubuntu-latest`. Each run asserts the app wrote into the isolated profile, that a prefix-named sibling is *not* reported as running, and that graceful stop works
- The macOS live test runs on a real desktop rather than in CI — GitHub's macOS runners have no usable window server, so `open -n` cannot launch anything there
- 9 Rust tests. The bash CLI has no automated suite; its verdicts come from the experiment logs in [`docs/experiments/`](docs/experiments/)

### Known limitations

- `multiapp.ps1`, the PowerShell port of the full tool, has **never been executed on Windows**. Run `scripts/test-windows.ps1` if you are willing to be the first
- The bash CLI's Linux support is implemented from verified patterns but has never been run on Linux
- Profiles are convenience isolation, **not a security boundary** — same user, same Keychain, same permission grants
- ChatGPT isolates its data but shares its account across profiles; do not sign out inside a profile
- Gemini, ChatGPT Classic, Telegram and HDRezka-Client cannot be profiled at all. Their backup and session commands still work
