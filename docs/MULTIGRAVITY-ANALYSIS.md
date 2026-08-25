# Multigravity-CLI — Full Source Analysis (Evidence Appendix)

> Analysis of https://github.com/sujitagarwal/multigravity-cli performed 2026-07-15 by reading every
> tracked text file via the GitHub API (`git/trees?recursive=1` confirmed `"truncated": false`,
> i.e. the complete file list was seen). This document is the evidence base for `REPORT.md`.
> Everything here is **Verified from source code** unless explicitly marked otherwise.

---

## 0. Verification status up front

- The repo contains **10 tracked files**, no submodules, no compiled components.
- `test.sh` is git-ignored (`.gitignore` contains only `test.sh`) and absent from the repo — **no test suite could be inspected**.
- The code **does not reference `~/.gravity` anywhere**. It uses `~/.antigravity/extensions` and
  `…/Application Support/Antigravity`.

## 1. File tree with responsibilities

```
multigravity-cli/
├── .gitignore              # single line: "test.sh"
├── README.md               # user docs; some claims diverge from code (see §7)
├── multigravity            # MAIN CLI — bash, macOS+Linux, ~800 lines, mode 100755
├── multigravity.ps1        # MAIN CLI — PowerShell port for Windows
├── install.sh              # curl|bash installer for macOS/Linux
├── install.ps1             # PowerShell installer for Windows
├── uninstall.sh            # macOS/Linux uninstaller
├── uninstall.ps1           # Windows uninstaller
├── icon.icns               # macOS app-bundle icon (binary, not inspected)
└── assets/multigravity-logo.jpg  # README logo (binary, not inspected)
```

No `package.json`, `setup.py`, Makefile, or binary — a pure shell-script tool.

## 2. Language / runtime / install

- Bash (`#!/usr/bin/env bash`, `set -euo pipefail`, `shopt -s nullglob`) for macOS/Linux; PowerShell for Windows.
- Entry points: `/usr/local/bin/multigravity` (fallback `~/.local/bin`); Windows gets a `multigravity.cmd` shim:
  ```bat
  @echo off
  powershell.exe -ExecutionPolicy Bypass -File "%~dp0multigravity.ps1" %*
  ```
- Install: remote `curl | bash` / `irm | iex` downloading from `raw.githubusercontent.com/.../main/`;
  auto-appends PATH export to `~/.zshrc` / `~/.bashrc` / fish config when using the `~/.local/bin` fallback.
- Self-update: `multigravity update` downloads to `"$target.tmp"` then `mv`s over itself on success (atomic-ish, rollback on failure).

## 3. The exact isolation mechanism (traced through code)

**The Antigravity binary/bundle is NEVER copied.** `find_app()` locates the existing system install and
launches that same binary. Isolation = three combined techniques:

1. **`HOME` / `USERPROFILE` + XDG/APPDATA env-var override** to a per-profile directory
2. **`--user-data-dir` + `--extensions-dir` CLI flags** (VS Code lineage flags)
3. **Per-profile directory trees** under a base dir

Base directory:
```bash
BASE="${MULTIGRAVITY_HOME:-$HOME/AntigravityProfiles}"
```

Profile layout creation — `create_profile_layout()`:
```bash
mkdir -p "$profile_path"
mkdir -p "$(extensions_dir "$profile_path")"     # $profile_path/.antigravity/extensions
# darwin:
mkdir -p "$profile_path/Library/Application Support"
if [ ! -e "$profile_path/Library/Keychains" ] && [ -d "$HOME/Library/Keychains" ]; then
  ln -s "$HOME/Library/Keychains" "$profile_path/Library/Keychains"   # Keychain SHARED via symlink
fi
# linux:
mkdir -p "$profile_path/.config/Antigravity" "$profile_path/.cache" \
         "$profile_path/.local/share" "$profile_path/.local/state"
```

Launch — `launch_profile()`:
```bash
# macOS:
HOME="$profile_path" open -n "$app" --args \
  --user-data-dir "$data_dir" \
  --extensions-dir "$ext_dir" \
  "$@"

# Linux:
HOME="$profile_path" \
XDG_CONFIG_HOME="$profile_path/.config" \
XDG_CACHE_HOME="$profile_path/.cache" \
XDG_DATA_HOME="$profile_path/.local/share" \
XDG_STATE_HOME="$profile_path/.local/state" \
"$app" --user-data-dir "$data_dir" --extensions-dir "$ext_dir" "$@"
```

Key observations:
- `open -n` forces a brand-new macOS process instance instead of activating the running one.
- macOS Keychains directory is symlinked → **credential store is deliberately shared**.
- **[Reasoned inference]** That Antigravity honors `--user-data-dir`/`--extensions-dir` is inferred from
  its VS Code lineage; not provable from this repo.

Windows launch (`Invoke-LaunchProfile`) is different — **env redirection only, no CLI flags**:
```powershell
$env:USERPROFILE  = $PROFILE_DIR
$env:APPDATA      = "$PROFILE_DIR\AppData\Roaming"
$env:LOCALAPPDATA = "$PROFILE_DIR\AppData\Local"
Start-Process -FilePath $APP -ArgumentList $ArgsToForward
```

## 4. Isolated vs shared, per profile type

**Full profile (default):**
- Isolated: user-data dir (settings, keybindings, workspace/global storage, accounts, state),
  extensions dir, everything written relative to the redirected `HOME`/`APPDATA`.
- Shared always: the application bundle itself; on macOS the login **Keychains** (symlink).

**Shared profile (`new <name> --shared`)** — `create_shared_layout()`:
```bash
touch "$profile_path/.shared"          # marker file
mkdir -p "$data_dir/User"
for f in settings.json keybindings.json snippets; do
  [ -e "$sys_data/User/$f" ] && [ ! -e "$data_dir/User/$f" ] && ln -s "$sys_data/User/$f" "$data_dir/User/$f"
done
rm -rf "$ext_dir"; ln -s "$sys_ext" "$ext_dir"   # share SYSTEM extensions dir
```
A shared profile shares extensions + settings/keybindings/snippets with the main install (symlinks — the
code comments say "read-only intent" but the links are read/write), isolating only the account/auth layer.
Windows uses `New-Item -ItemType SymbolicLink` which needs Developer Mode/admin; failures are swallowed
with `-ErrorAction SilentlyContinue` (a shared profile can silently end up unlinked).

## 5. Why concurrent instances work

- The single-instance lock of VS Code-family apps lives **inside the user-data dir**; a distinct
  `--user-data-dir` (or redirected `%APPDATA%`) per profile means each instance takes its own lock.
  **[Reasoned inference from VS Code/Electron/Chromium behavior — no lock file is named in the code.]**
- macOS additionally requires `open -n` to bypass Launch Services' activate-existing-instance behavior.
  **[Verified from `man open`: "-n Open a new instance of the application(s) even if one is already running."]**
- There is **no lock-file manipulation** anywhere in the code.

## 6. Platform specifics

| Aspect | macOS | Linux | Windows |
|---|---|---|---|
| App discovery | `/Applications/Antigravity.app`, `~/Applications/…` | PATH, `/usr/share/antigravity/antigravity`, `/usr/bin`, `/usr/local/bin`, `~/.local/bin`, `~/Applications/Antigravity.AppImage` | `%LOCALAPPDATA%\Programs\Antigravity\Antigravity.exe`, `%PROGRAMFILES%\…`, PATH |
| User data | `$profile/Library/Application Support/Antigravity` | `$profile/.config/Antigravity` | `$profile\AppData\Roaming\Antigravity` |
| Isolation | `HOME` + flags + `open -n` | `HOME` + `XDG_*` + flags | `USERPROFILE`/`APPDATA`/`LOCALAPPDATA` only |
| Shortcut | generated `.app` at `~/Applications/Multigravity <name>.app`, `CFBundleIdentifier=com.multigravity.profile.<name>`, bash `run` wrapper exporting `MULTIGRAVITY_HOME` | launcher script + `.desktop` file (`StartupWMClass=Antigravity`) | Start Menu `.lnk` via WScript.Shell COM |

Platform detection: `platform()` normalizes `uname -s`, overridable via `MULTIGRAVITY_PLATFORM` (not in README).

## 7. Lifecycle features — code vs README

All implemented in both bash and PowerShell: `new` (full/`--shared`/`--from <tpl>`), launch (default case),
`list [--raw]`, `status` (running/type/last-used/size), `rename`, `clone` (`cp -R`), `delete` (y/N confirm),
`template save|list|delete`, `export` (tar.gz / zip), `import`, `stats` (`du -sh`), `doctor`, `update`,
`completion`, `help`.

Discrepancies:
- `MULTIGRAVITY_PLATFORM` override is undocumented.
- README does not mention that shared-profile symlinks write back to the system install.
- **No `cleanup`/`prune`/`gc` command exists** — cleanup only happens via `delete`, `template delete`, uninstall.

## 8. Safety, destructive ops, weaknesses

Safety: `set -euo pipefail`; name regex `^[a-zA-Z0-9][a-zA-Z0-9-]*$` (blocks `../` traversal);
existence pre-checks on create/clone/rename/import; y/N confirm on `delete` and uninstall;
atomic self-update with rollback.

Destructive ops:
- `delete_profile`: `rm -rf "$profile_path"` (confirmed)
- `template delete`: `rm -rf "$tpl_path"` — **no confirmation**
- `create_shared_layout`: `rm -rf "$ext_dir"` on a computed path before symlinking
- uninstallers: `rm -rf "$PROFILE_BASE"` (confirmed) + unconditional removal of binary/shortcuts

Weaknesses to avoid in our product:
- `import` extracts archives with `tar -xzf … -C "$BASE"` with **no path-traversal sanitization** of archive contents.
- Windows shared-profile symlink failures are silently swallowed.
- "Running" detection greps process arg lists (`ps -eo args | grep -qF "$data_dir"`); Windows matches on bare
  profile name (`-like "*$name*"`) — loose, can false-positive.

## 9. What the code reveals about Antigravity

- Paths: `~/Library/Application Support/Antigravity` (mac), `~/.config/Antigravity` (linux),
  `%APPDATA%\Antigravity` (win); extensions at `~/.antigravity/extensions` (all platforms).
- Flags used: `--user-data-dir <dir>`, `--extensions-dir <dir>`, plus passthrough args.
- **Antigravity's own bundle identifier is never referenced or modified.** No re-signing anywhere.

## 10. Could-NOT-verify list

- Whether Antigravity honors the two flags (inferred from VS Code lineage).
- The exact single-instance-lock mechanism (inferred: per-user-data-dir singleton lock).
- Windows extension isolation (no `--extensions-dir` passed there; depends on `USERPROFILE` redirection).
- `test.sh` (absent), binary assets (not inspected).
- README contributor credits (not checked against commit history).
