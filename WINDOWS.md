# Multiapp on Windows

> **It has now been run, and it does not work.** On 2026-09-01 `multiapp.ps1` was executed on a real
> Windows 11 machine for the first time. It does not even parse: PowerShell fails at line 186 with
> *"The '<' operator is reserved for future use"* on `Die "usage: multiapp rename <app> <old> <new>"`,
> and reports further errors after it. **No command in this file has ever run.** Use the Rust
> `multiapp.exe` instead — that one is tested. The rest of this document describes what was intended,
> not what works.

## There is no build step

Nothing to compile or install. The Windows CLI is one file — `multiapp.ps1` — that you run with the
PowerShell already in Windows. No `.exe`, no installer, no dependencies.

The macOS menu-bar app (`app/Multiapp.swift`) is **AppKit, so it is macOS-only**. There is no Windows
GUI in any form, and porting it would be a rewrite, not a port.

## Running it

```powershell
# from the folder containing multiapp.ps1
powershell -ExecutionPolicy Bypass -File .\multiapp.ps1 doctor
powershell -ExecutionPolicy Bypass -File .\multiapp.ps1 apps
```

`-ExecutionPolicy Bypass` applies to that one run only — it changes nothing permanently. To avoid
typing it every time, allow local scripts for your user once:

```powershell
Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
```

If the file came from the internet, unblock it first: `Unblock-File .\multiapp.ps1`.

## What exists on Windows today

**Profiles only:** `apps · scan · probe · new · launch · list · stop · clone · rename · delete ·
trash · wrapper · doctor`

Profiles use the same lever as macOS — `--user-data-dir=<folder>` — which is Chromium's, not Apple's,
so it is expected to work. Windows is in one way *easier*: it has no "one instance per app" rule to
defeat, so a second instance just starts.

Profiles live in `%APPDATA%\Multiapp\Profiles\<app>\<profile>\data`.

## What does NOT exist on Windows

Everything else: `backup`, `restore`, `migrate-list`, `session-check`, `session-backup`,
`session-restore`, `app-export`, `app-import`, `sessions`, `transfer`, `export`, `import`,
`list-installed`, `install-stub`. Use the macOS/Linux `multiapp` for those.

A port of them exists in draft but is **not shipped**: an adversarial review found a defect that would
sweep the Windows Credential Manager vault and DPAPI master keys into a backup archive. It will not be
released until that is fixed and actually verified.

## Known defects in the current script

These are real and unfixed — read before trusting it:

| Defect | Consequence |
|---|---|
| `stop` uses `Stop-Process` | A hard kill. Electron/Chromium never flushes its databases, so a profile can be corrupted. Close the app normally instead. |
| `trash purge` has no containment check | It runs `Remove-Item -Recurse -Force` without verifying the path is inside Multiapp's own folder. The bash version has that guard; this does not. |
| Profile names reject spaces and `_` | `"Work Account"` is valid on macOS and refused here. |
| `registry.local` uses `;` | macOS/Linux writes `|`. The two files are mutually unreadable — do not copy one between machines. |
| The `chatgpt` entry is wrong | The Windows ChatGPT ships as an **MSIX Store app**, which redirects its writes into `%LOCALAPPDATA%\Packages\…`. `--user-data-dir` cannot escape that, so profiles cannot work for it. |
| Built-in app paths are guesses | Written from documentation, never checked against a real install. `vscode` assumes the *user* installer, not the system one. |
| `scan` picks the first `.exe` alphabetically | For Squirrel-packaged apps that is usually `Update.exe`, not the app. It also does not look inside `app-<version>\`, where Electron actually lives. |

## Helping test it

Run these and report the exact output:

```powershell
.\multiapp.ps1 doctor          # environment + paths it resolved
.\multiapp.ps1 apps            # which built-in apps it actually found installed
.\multiapp.ps1 scan            # does discovery find your Electron apps?
.\multiapp.ps1 probe notion    # does the app honour --user-data-dir=?
```

`probe` is the important one: it launches the app into a throwaway folder for ~10 s and reports
whether the app wrote there. That single result decides whether profiles work at all for a given app.

## Windows-specific caveats

- **Paths with spaces** are everywhere on Windows (`%APPDATA%\Telegram Desktop`). Quote them.
- **`%APPDATA%` vs `%LOCALAPPDATA%`** — apps use both; the script checks the ones it knows about.
- **SmartScreen / Defender** may warn about a downloaded script; `Unblock-File` clears it.
- **Shortcuts** (`wrapper`) embed `-ExecutionPolicy Bypass`, which AppLocker, Constrained Language
  Mode or an MDM policy may block on managed machines.
- **MSIX / Microsoft Store apps** cannot be profiled at all — their writes are redirected into a
  per-package container that `--user-data-dir` cannot escape.
