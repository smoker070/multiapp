# Phase 0 — Experiment E1 Results (+ E2 core, run early)

**Date:** 2026-07-15 · **Machine:** the author's Mac, Darwin 25.5.0 (macOS 26), Apple Silicon
**Status:** E1 **PASS with a critical twist** — env propagates, but `HOME` is the wrong lever on modern macOS; the right lever is `--user-data-dir=` (equals form), which modern Electron honors at core level.

---

## E1a — Does `open -n` propagate custom env to the launched app?

**Method:** built a throwaway probe `.app` (bundle id `com.multiapp.e1probe`) whose executable writes
`$HOME`, a custom var, and its pid/ppid to a file. Launched twice: directly, and via
`HOME=<fake> MG_TEST=via-open-n open -n E1Probe.app`.

**Result: ✅ YES — verified.** The `open -n`-launched process (ppid = 1, i.e. genuinely spawned via
launchd/Launch Services, not as our child) received both the overridden `HOME` and the custom variable.

## E1b — Does the `HOME` override actually redirect app data paths?

**Method:** compiled a Swift probe printing `getenv("HOME")`, `NSHomeDirectory()`,
`FileManager.homeDirectoryForCurrentUser`, `NSSearchPathForDirectoriesInDomains(.applicationSupport…)`,
and `getpwuid()->pw_dir`, run with and without `HOME` override. Cross-checked with Node
`os.homedir()` and a real Electron runtime (v43.1.1 via npm).

**Result: ❌ NO for the layers that matter — verified, and it overturns a report assumption.**

| Layer | Under `HOME=<fake>` | Consequence |
|---|---|---|
| POSIX `getenv("HOME")` | fake ✅ follows | dotfiles written via shell/libc paths move |
| Node `os.homedir()` (libuv) | fake ✅ follows | Node-layer paths in Electron apps move |
| `NSHomeDirectory()` / `homeDirectoryForCurrentUser` | **real ❌ ignores** | Cocoa apps don't move |
| `NSSearchPathForDirectoriesInDomains` (Application Support) | **real ❌ ignores** | the big one: `~/Library/Application Support` never moves |
| Electron `app.getPath('appData'/'userData')` (Chromium PathService) | **real ❌ ignores** | **`HOME` override does NOT isolate Electron user data on macOS 26** |
| `getpwuid()->pw_dir` | real ❌ (by definition) | — |

(`CFCopyHomeDirectoryURL` is now marked *unavailable on macOS* in the current SDK — the historical
"CF checks `$HOME`" behavior is gone at this API level.)

**Implication for multigravity:** its macOS `HOME` override is mostly vestigial on macOS 26 — the real
isolation work is done by the `--user-data-dir`/`--extensions-dir` flags that VS Code-family apps parse.

## E1c — Does Electron honor `--user-data-dir` as a core switch?

**Method:** minimal Electron app printing `app.getPath('userData')`, launched with
`--user-data-dir=<dir>` (equals form).

**Result: ✅ YES — verified on Electron 43.1.1.** `userData` and `sessionData` both followed the flag.
The official command-line-switches docs don't list it — a docs gap, now empirically settled.

**⚠️ Syntax matters:** Chromium-style switch parsing requires **`--user-data-dir=/path`** (equals).
The space-separated form `--user-data-dir /path` (multigravity's style — fine for VS Code's own CLI
parser) is **ignored** by raw Electron/Chromium parsing. Verified both ways against Claude.app.

## E2 core (run early) — Claude.app profile isolation + concurrency

**Method:** Claude.app (Electron **42.5.1** bundled — read from the framework's binary) launched via
`open -n /Applications/Claude.app --args --user-data-dir=<scratch dir>` twice with two different dirs;
real `~/Library/Application Support/Claude` watched for writes via marker-file `find -newer` diffs.

**Results:**
- ✅ **Flag honored:** each test dir got a complete fresh Chromium profile (Cookies, IndexedDB,
  Local Storage, Local State, Partitions…).
- ✅ **Two concurrent instances stable**, each pinned to its own data dir (visible in `ps` args).
- ✅ **No leakage attributable to the test instances.** Writes seen in the real profile came from the
  *already-running host* Claude instance (this session's own app — started 2h before the test).
- ⏳ **Not yet tested:** actual login in a probe profile, session persistence across restart
  (safeStorage/Keychain interaction = E3), long-run stability.
- 🧹 Cleanup: test instances killed by exact `--user-data-dir=<scratch>` arg match; test dirs are in
  the session scratchpad (auto-cleaned). Real profile untouched.

## Version floor — old Electron does NOT honor the flag

HDRezka-Client (the author's own app, Electron **22.3.25**): launched with the flag → test dir stayed
**empty** (flag ignored; single run 🟠). Notion bundles Electron 42.3.3 (untested, expected OK).
Core `--user-data-dir` support landed in modern Electron (~v36 era 🟡); adapters must read the bundled
Electron version from `Electron Framework` and gate the verdict.

---

## Consequences for the architecture (report updated accordingly)

1. **Generic-Electron adapter mechanism changes:** `--user-data-dir=<profile>` flag (equals form),
   NOT `HOME` override. `HOME` remains only a supplementary measure for Node-layer dotfiles.
2. **Native non-sandboxed apps (ChatGPT, Gemini): redirection story collapses** on macOS 26 —
   Foundation path APIs ignore `$HOME`, so there is no generic env lever for their data at all.
   They move to "unsupported by redirection" unless an app-specific flag exists. This raises the value
   of the long-term hard-isolation tier (separate user / VM).
3. **Claude adapter: upgraded to *verified supported* (mechanism level)** — pending E3 (login
   persistence) before calling it fully supported.
4. **Detection engine must check bundled Electron version** as a static-probe input.
5. Process-management lesson (recorded): only kill processes whose argv provably contains our
   profile path — an unrelated flagless host instance of the same app may legitimately be running.

## Checklist deltas

- [x] **E1 — PASS** (env propagates; correct lever identified)
- [x] **E2 — core PASS** (isolation + concurrency verified; login/session persistence still open → E3)
- [ ] E3 — safeStorage/session persistence across restarts (next)
- Matrix updates: Claude ✅ (mechanism verified) · generic Electron ✅ if Electron ≥ ~36 · ChatGPT/Gemini ⬇ unsupported-by-redirection · HDRezka-class old Electron ❌
