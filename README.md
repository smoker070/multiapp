# multiapp — prototype CLI (v0.2.0)

Run multiple isolated profiles of installed apps. Implements the mechanism verified in Phase 0
(`../experiments/`): launch the app with `--user-data-dir=<profile>` (**equals form**) — no binary
modification, no env overrides, credential store never touched. The lever is Electron/Chromium, so it
is the same on every OS; only the plumbing (launch command, paths, shortcuts) differs per platform.

## Files

| File | Platform | Status |
|---|---|---|
| `multiapp` (bash) | **macOS** + **Linux** | macOS fully tested; Linux implemented per multigravity's verified patterns, **pending a real Linux test** |
| `multiapp.ps1` (PowerShell) | **Windows** | implemented per the verified mechanism + multigravity.ps1 conventions, **pending a real Windows test** (not even syntax-checkable on this Mac) |
| `app/Multiapp.swift` + `app/build.sh` | **macOS menu-bar app** (v0.1.0) | built with plain `swiftc` (no Xcode), ad-hoc signed; installs to `~/Applications/Multiapp.app`, DMG in `app/dist/`. Thin GUI over the CLI: profile list with running state, launch/stop, New Profile dialog, rescan, Dock-launcher creation, reveal-in-Finder. Uses `multiapp list --raw` (fast path, no `du`) |

## Move-proof install (the `multiapp` command)

Run once (already done on this Mac):
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

`apps · scan · new · launch · list · stop · clone · rename · delete · trash · wrapper · probe ·
install-stub · doctor · help` — plus Claude-only session ops on macOS/Linux:
`sessions · transfer · export · import`.

| Command | Notes |
|---|---|
| `scan` | discover new Electron/Chromium apps → `registry.local` as *untested* (macOS: `/Applications`; Linux: `.desktop` files; Windows: Programs dirs). Natives/sandboxed skipped |
| `probe <app>` | 10-second canary: does the app honor the flag? Persists verdict for scanned apps |
| `clone / rename / delete` | stopped profiles only; delete is type-name-confirmed → staged `Trash/` (recoverable) |
| `wrapper` | macOS: `osacompile` applet with the app's icon; Linux: `.desktop`; Windows: Start-Menu `.lnk` |
| `transfer claude <src> <dst> [sel]` | copy chosen Claude Code sessions between profiles on one machine |
| `export/import claude` | bundle chosen sessions (index + transcripts) to move to another machine |
| `install-stub` | (re)install the move-proof launcher on PATH |

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
