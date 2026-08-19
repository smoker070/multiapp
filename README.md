# multiapp — prototype CLI (v0.3.0)

Run multiple isolated profiles of installed apps. Implements the mechanism verified in Phase 0
(`../experiments/`): launch the app with `--user-data-dir=<profile>` (**equals form**) — no binary
modification, no env overrides, credential store never touched. The lever is Electron/Chromium, so it
is the same on every OS; only the plumbing (launch command, paths, shortcuts) differs per platform.

## Files

| File | Platform | Status |
|---|---|---|
| `multiapp` (bash) | **macOS** + **Linux** | macOS fully tested; Linux implemented per multigravity's verified patterns, **pending a real Linux test** |
| `multiapp.ps1` (PowerShell) | **Windows** | **profile commands only** (~half the CLI) and **never run on real Windows**. Read [WINDOWS.md](WINDOWS.md) first — it lists the known defects. The backup/session/export commands do not exist there yet |
| `rust/` (Rust workspace) | **macOS + Windows + Linux** | **profile commands only** (`new · launch · list · stop · where`) — the portable rewrite. Unlike the bash and PowerShell scripts, this one is **tested on real Windows and Linux by CI**, which launches Edge/Chrome into an isolated profile and stops it again. Builds `multiapp.exe`. Shares the same profile storage as the bash CLI |
| `app/Multiapp.swift` + `app/build.sh` | **macOS menu-bar app** (v0.3.0) | built with plain `swiftc` (no Xcode), ad-hoc signed; installs to `~/Applications/Multiapp.app`, DMG in `app/dist/`. Thin GUI over the CLI: profile list with running state, launch/stop, rename/clone/export/delete, New Profile dialog, **Back Up & Restore** submenu, **Export/Import App Data**, Claude session transfer, rescan. Reads `list --raw` / `migrate-list --raw` (cached) so the menu opens instantly |

## Move-proof install (the `multiapp` command)

Run once:
```bash
"<path>/prototype/multiapp" install-stub
```
This writes a tiny **self-locating stub** to `~/.local/bin/multiapp` (on PATH). The stub reads the real
script's path from `~/.config/multiapp/target`; the real script rewrites that path on every run, and if
the folder ever moves the stub self-heals instantly via the OS file index (`mdfind` on macOS,
`locate/plocate` on Linux — **not** a recursive `find`, which hangs for minutes on big cloud drives).

So: **moving or renaming the `Multiapp` folder no longer breaks anything.** Worst case, if the index is
cold, run `install-stub` once from the new location.

## Quick start

```bash
multiapp apps                      # what's supported here (honest verdicts)
multiapp new claude work           # create a profile
multiapp launch claude work        # run it (isolated login/session/settings)
multiapp wrapper claude work       # clickable launcher (macOS .app / Linux .desktop)
multiapp list                      # profiles + running state + size
multiapp stop claude work          # graceful quit
```

Example: a profile named `second` holding a second Claude account —
`multiapp launch claude second`, or click `~/Applications/Multiapp/Claude – second.app`.

## Commands

**Profiles:** `apps · scan · probe · new · launch · list · stop · clone · rename · delete · trash ·
wrapper · install-stub · doctor · help`  (aliases: `ls`=list, `mv`=rename, `rm`=delete)

**App data & logins:** `migrate-list · backup · restore · app-export · app-import ·
session-check · session-backup · session-restore · list-installed`

**Claude Code sessions:** `sessions · transfer · export · import`

Everything outside the profile group is macOS/Linux today — see the platform table above.

| Command | Notes |
|---|---|
| `scan` | discover new Electron/Chromium apps → `registry.local` as *untested* (macOS: `/Applications`; Linux: `.desktop` files; Windows: Programs dirs). Natives/sandboxed skipped |
| `probe <app>` | 10-second canary: does the app honor the flag? Persists verdict for scanned apps |
| `clone / rename / delete` | stopped profiles only; delete is type-name-confirmed → staged `Trash/` (recoverable) |
| `wrapper` | macOS: `osacompile` applet with the app's icon; Linux: `.desktop`; Windows: Start-Menu `.lnk` |
| `transfer claude <src> <dst> [sel]` | copy chosen Claude Code sessions between profiles on one machine — a **true copy**: new session ids + duplicated transcript, so the two profiles never share a live session (transcripts live in `~/.claude`, which `--user-data-dir` does not isolate) |
| `export/import claude` | bundle chosen sessions (index + transcripts) to move to another machine |
| `app-export <app> [dir]` / `app-import <archive>` | export/import **any** installed app's local data (work sessions, settings) — not just profile-capable ones. Looks in `~/Library` **and** home dotfolders (`~/.codex`, `~/.claude`) where CLI-style apps actually keep sessions. Import **merges**: it never deletes files the archive omitted |
| `session-check / session-backup / session-restore <app>` | save and put back just the **login session** (KB-sized). `session-check` says whether that login could survive a move to another Mac |
| `migrate-list` | apps whose **real** local data can be backed up on this machine, with a verdict per app |
| `backup <app> [out] [--include-cache]` | archive an app's real data (quit the app first). Caches excluded by default. **On the same Mac a restore is full — login included** — because the login lives in the Keychain, which stays on this machine |
| `restore <app> <archive>` | restore a backup: validates the archive, moves current data to `Trash/` first (recoverable), then extracts |
| `install-stub` | (re)install the move-proof launcher on PATH |

### Backup / restore — what actually moves

**Local content** (history, drafts, settings) is plain files → copies perfectly.

**Login** is different, and the detail matters:

| App type | What the archive holds | Verified |
|---|---|---|
| Electron/Chromium (Claude, Notion, ChatGPT, Chrome, VS Code) | the session files (`Cookies`, `Local State`, `Local Storage`, `IndexedDB`) — but the cookies are **encrypted** (`v10` prefix) with a key that lives in the **Keychain**, which is *not* in the archive | ✅ inspected |
| Native, Keychain-token (ChatGPT Classic, Gemini, **Telegram for macOS**) | **no usable session files** — the token lives only in the Keychain. Verified by experiment: deleting every session file left Gemini still signed in | ✅ verified |
| **Telegram Desktop** (`com.tdesktop.Telegram`) | different from Telegram for macOS: its login lives in `tdata`, **its own encrypted key store, not the Keychain** — so this one is the best cross-machine candidate | ✅ inspected |

Either way the login is only usable where its **Keychain** is:

- **Restore on this Mac with the Keychain intact** (e.g. you reinstalled *the app*) → history **and**
  login work — not because the archive carries the login, but because the Keychain never left.
- **Fresh macOS install, another Mac, or another user** → the Keychain is gone, so encrypted cookies
  can't be decrypted and tokens are absent → history and settings restore, **you sign in once**.

⚠️ "Same machine" is not the same as "same Keychain". Erasing the Mac and reinstalling macOS wipes the
login keychain too, so that case behaves like a new machine unless you also restore the Keychain
(Migration Assistant / Time Machine do that; Multiapp deliberately never touches Keychain items).

`migrate-list` only lists apps that actually hold a **login** (so Sublime Text, Numbers, Xcode and
friends are filtered out) and marks each `full-samemac`. Multiapp never touches Keychain or Credential
Manager items — that is precisely why logins don't migrate to another machine.

## Storage (per platform)

```
macOS:   ~/Library/Application Support/Multiapp/Profiles/<app>/<profile>/data/
Linux:   ${XDG_DATA_HOME:-~/.local/share}/multiapp/Profiles/<app>/<profile>/data/
Windows: %APPDATA%\Multiapp\Profiles\<app>\<profile>\data\
         + registry.local (scanned apps) · Trash/ (staged deletes) · Probes/ (canary)
wrappers: ~/Applications/Multiapp (mac) · ~/.local/share/applications (linux) · Start Menu (win)
stub cfg: ~/.config/multiapp/target
```

## Known limitations (prototype)

- Profiles are convenience isolation, not a security boundary (same user, same Keychain/TCC).
- ChatGPT: data isolates but identity is shared → one account everywhere (E4).
- Claude downloads its ~11 GB Cowork `vm_bundles` **per profile**; safe to delete inside a profile if
  unused (Claude re-downloads on demand).
- **Linux & Windows are unproven on real hardware** — run `multiapp probe <app>` there first; treat
  every non-macOS verdict as provisional.
- Session transfer/export is Claude-Code-specific and macOS/Linux-only for now (needs Python 3).

## Fixed-the-hard-way notes

- `--user-data-dir` MUST use `=`; the space form is silently ignored by Chromium parsing.
- macOS bash 3.2: empty arrays + `set -u` need the `${arr[@]+"${arr[@]}"}` idiom.
- Launch Services silently ignores `.app` bundles whose executable is a plain shell script → wrappers
  must be Mach-O (`osacompile` applets).
- `install-stub` must `rm -f` the target before writing, or `cat >` follows an existing symlink and
  overwrites the real script (hit this once — now guarded).
- Self-heal must use the OS file index, never a recursive `find` (minutes-long hang on cloud drives).
