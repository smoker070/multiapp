# Phase 0 — Experiments E4 (ChatGPT) + E6 (Gemini) Results

**Date:** 2026-07-15 · **Machine:** the author's Mac, Darwin 25.5.0 (macOS 26), Apple Silicon
**Verdicts:** ChatGPT = **PARTIAL** (data isolates, account shared) · Gemini = **UNSUPPORTED** (concurrency yes, isolation no)

---

## E4 — ChatGPT.app (`com.openai.codex`)

### Finding 0: the app changed under us — it's Chromium now

The report's "native Swift" classification was correct for the *old* ChatGPT app but is stale:
the bundle on this Mac was **updated 2026-07-14** and is now a **full Chromium 150 embed**
(Atlas-browser lineage). Evidence (🔵 verified locally):

- `NSPrincipalClass = BrowserCrApplication` (Chromium's NSApplication subclass)
- `Codex Framework.framework/Versions/150.0.7871.115/` — a Chromium version string
  (`Chrome/150.0.7871.115` inside the binary)
- Chromium-style helpers: `Codex (GPU/Renderer/Alerts/Service).app`, `browser_crashpad_handler`
- `user-data-dir` switch string present in the framework binary
- Default Chromium profile root: `~/Library/Application Support/Codex` (Local State, Default/, Cookies)
- Legacy native-era data still at `~/Library/Application Support/com.openai.chat` (per-account-ID
  subdirs) and `com.openai.codex`
- Chromium safeStorage entry in login keychain: service **"Codex Safe Storage"** (metadata only)

**Meta-lesson:** app runtimes change under auto-update — the compatibility engine MUST re-probe per
app version (REPORT §17 requirement now proven necessary by a real case within one day of writing it).

### Dynamic results (🔵 experiment)

| Check | Result |
|---|---|
| `open -n ChatGPT.app --args --user-data-dir=<test dir>` | ✅ **Honored** — full Chromium profile (incl. `Default/`) created in test dir; real `Codex` root untouched (marker-diff clean) |
| Two concurrent instances (test-profile + default-profile) | ✅ Both main processes stable |
| Fresh test profile login state | ❌ **Auto-logged-in with the author's existing account** — auth is injected from a source OUTSIDE the user-data dir (Keychain token, `2DC432GLL2.*` access groups / legacy `com.openai.chat` store) |

### Verdict: **PARTIAL — T5 confirmed empirically**

- Chromium-layer data (cookies, cache, site storage) isolates per profile.
- **Identity does not**: every profile auto-authenticates as the same account. Two different ChatGPT
  accounts side-by-side are NOT achievable via `--user-data-dir=` alone.
- ⚠️ Operational hazard for the adapter: with a shared token, **signing out in any profile may kill
  the session in all profiles** (untested — deliberately; do not test casually on a live account).
- Adapter value proposition shrinks to "separate workspaces/site-data of one account" — must be
  labeled honestly. True multi-account ChatGPT needs the hard-isolation tier (separate macOS user/VM).

## E6 — Gemini.app (`com.google.GeminiMacOS`)

### Static (🔵)

- Genuinely native Swift: no Chromium/Electron frameworks (`Frameworks/` holds only a Swift compat
  dylib), no `NSPrincipalClass` override, **no `user-data-dir`/profile strings in the binary**.
- State locations (all bundle-ID- or group-keyed, none `$HOME`-relative in a usable way after E1b):
  `~/Library/HTTPStorages/com.google.GeminiMacOS` (cookie/network store, 488 KB),
  `~/Library/Application Support/com.google.GeminiMacOS`, `~/Library/Preferences/com.google.GeminiMacOS.plist`,
  `~/Library/Group Containers/group.com.google.gemini`.

### Dynamic (🔵)

- `open` then `open -n`: **two concurrent instances ran** (no single-instance enforcement).
- But both instances point at the identical bundle-ID-keyed stores → **same account, same data,
  concurrent-write risk**. No isolation lever exists: no flags, `HOME` ineffective (E1b),
  `HTTPStorages`/group containers are bundle-ID-keyed (like sandbox containers).

### Verdict: **UNSUPPORTED for profiles.** "Multi-window of one account" is all redirection can offer,
and even that carries shared-store concurrency risk — the adapter should refuse with an honest
explanation and point to the future hard-isolation tier.

---

## Matrix deltas

| App | Old verdict (report v1) | New verdict (verified) |
|---|---|---|
| ChatGPT | Unsupported by redirection ⚪ | **Partial** 🔵 — Chromium embed honors `--user-data-dir=`; data isolates; **account shared** |
| Gemini | Unsupported-leaning ⚪ | **Unsupported** 🔵 — native, no levers; concurrency-only |

## Cumulative Phase 0 status

| Exp | Status |
|---|---|
| E1 · E2 · E3-core · E5 | ✅ done (see earlier write-ups) |
| **E4 ChatGPT** | ✅ **done — PARTIAL** |
| **E6 Gemini** | ✅ **done — UNSUPPORTED** |
| E7 Telegram · E8 update · E9 wrappers · E10 leak detector · E11 Electron floor | ⏳ pending |
