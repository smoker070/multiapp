# Phase 0 — Experiments E2 (full) + E3 (core) + E5 Results

**Date:** 2026-07-15 · **Machine:** the author's Mac, Darwin 25.5.0 (macOS 26), Apple Silicon
**App under test:** Claude.app (`com.anthropic.claudefordesktop`, bundled Electron 42.5.1)
**Status: E2 PASS · E3 core PASS · E5 PASS (for Claude)** — the flagship use case is verified end-to-end.

---

## Setup

Durable profile at the product's proposed location (not scratchpad):

```
~/Library/Application Support/Multiapp/Profiles/claude/e2-test/
```

Launch command (the verified recipe):

```bash
open -n /Applications/Claude.app --args --user-data-dir="$HOME/Library/Application Support/Multiapp/Profiles/claude/e2-test"
```

No `HOME` override, **no Keychains symlink, no env vars at all** — flag only, equals form.

## E2 — Two accounts, isolated sessions

1. Test instance launched while the primary Claude (the author's main account, real profile) was running
   → fresh onboarding/login window appeared. ✅ concurrent run of real-profile + test-profile instances.
2. the author logged in with a **second account** in the test instance.
3. Verified the session landed **only** in the test profile:
   - `Cookies` (20 KB, mtime = login time), `Local Storage/leveldb` active,
     `IndexedDB/https_claude.ai_0.indexeddb.leveldb` created — all inside `e2-test/`.
   - Main process + every helper (GPU/Renderer/Utility) pinned to the profile via
     `--user-data-dir=` in their argv.
4. Both accounts usable simultaneously in separate windows.

**Verdict: PASS — verified by experiment.** Two logged-in Claude accounts, fully separate session
stores, one untouched app binary.

## E3 (core) — Session persistence across restart (safeStorage)

1. Test instance quit gracefully (SIGTERM to the main pid — helpers followed, 0 processes left).
2. Relaunched with the same `--user-data-dir=` (new pid).
3. **Window opened already logged in as the second account — no re-login.** ✅

Keychain observation (metadata only — no secret was read):
`security find-generic-password -s "Claude Safe Storage"` shows a **single** entry
(`acct="Claude Key"`, login keychain). Interpretation: all Claude profiles share one Chromium
safeStorage encryption key; each profile's cookie DB is encrypted with that shared key and decrypts
fine after restart. Exactly the "shared key, separated data" model predicted in REPORT.md §19.

**Verdict: core PASS.** Still open for full E3: long-run token refresh behavior, and whether a future
Claude update rotates the safeStorage entry (would affect all profiles at once — re-login, not data loss).

## E5 — Is multigravity's Keychains symlink needed?

The entire E2/E3 flow ran with **no** `~/Library/Keychains` symlink and no `HOME` override —
login, safeStorage encryption, and post-restart decryption all worked.

**Verdict: PASS (for Claude) — the symlink is vestigial.** Keychain access goes through `securityd`
via the user session, not through `$HOME` path resolution. Design decision confirmed: Multiapp will
never touch Keychains paths. (Native Keychain-auth apps remain a different story — see E4/E6.)

## Operational notes for the adapter

- Graceful quit: SIGTERM to the **main** process only; match it with
  `Contents/MacOS/Claude --user-data-dir=<profile>` — matching loosely on `MacOS/Claude` also catches
  helpers ("Claude Helper…"), which briefly broke the kill step during the test.
- Cookie DB flushes cleanly on SIGTERM (20 KB, no journal recovery needed on relaunch).
- The profile dir is self-contained and relocatable in principle (clone/export candidates:
  everything; secret-bearing: `Cookies*`, `Local Storage`, `IndexedDB`).

## Cumulative Phase 0 status

| Exp | Status |
|---|---|
| E1 (env propagation + real lever) | ✅ PASS — `--user-data-dir=` equals form |
| E2 (two logged-in profiles) | ✅ PASS |
| E3 (persistence/safeStorage) | ✅ core PASS (long-run + key-rotation cases open) |
| E5 (no Keychains symlink) | ✅ PASS for Claude |
| E4 ChatGPT · E6 Gemini · E7 Telegram · E8 update · E9 wrappers · E10 leak detector · E11 Electron floor | ⏳ pending |
