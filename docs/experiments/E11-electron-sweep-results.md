# Phase 0 — ChatGPT Classic verdict + E11 Electron Sweep Results

**Date:** 2026-07-15 · **Machine:** the author's Mac, Darwin 25.5.0 (macOS 26), Apple Silicon

---

## ChatGPT Classic.app — UNSUPPORTED (static verdict, conclusive)

Context: OpenAI merged Codex + ChatGPT into the new Chromium-based ChatGPT.app (see E4); the previous
native app now ships as "ChatGPT Classic".

| Fact (🔵 verified locally) | Value |
|---|---|
| Bundle ID | `com.openai.chat` — i.e. the "legacy" data dir found in E4 is actually **Classic's live data** |
| Version / updated | 1.2026.160 / 2026-06-24 |
| Runtime | Native Swift (ChatGPT.framework, LiveKitWebRTC, Lottie, Sparkle — no web-engine frameworks) |
| Profile flags in binary | None (`user-data-dir` etc. absent) |
| `LSMultipleInstancesProhibited` | `false` (concurrency allowed, but pointless — all state shared) |
| Keychain access group | `2DC432GLL2.com.openai.shared` — **shared with the new ChatGPT.app**: both apps draw the same identity |

**Verdict: UNSUPPORTED** — native app with zero redirection levers (E1b killed `HOME`), and its auth
is the same shared OpenAI token. No dynamic test performed (it was running with the live session;
statics are conclusive). Skipped per decision — adapter should list it as unsupported.

## E11 — Electron fleet sweep (`--user-data-dir=` equals form, ~11 s launch each, then killed by argv match)

| App | Bundled Electron | Flag honored? |
|---|---|---|
| OpenMTP | 18.3.15 | ✅ HONORED |
| GitHub Desktop | 40.1.0 | ✅ HONORED |
| Antigravity | 41.0.2 | ✅ HONORED (VS Code fork — also parses it itself) |
| Notion Calendar | 41.5.0 | ✅ HONORED |
| Notion | 42.3.3 | ✅ HONORED |
| Visual Studio Code | 42.5.0 | ✅ HONORED (T1 official) |
| Claude *(E2)* | 42.5.1 | ✅ HONORED |
| HDRezka-Client *(E1)* | 22.3.25 | ❌ IGNORED |
| Antigravity IDE | 39.2.3 | ⏭ skipped (was running; VS Code fork ⇒ T1) |

### Conclusion — the "version floor ~36" hypothesis is DEAD

Electron 18 honoring the flag while Electron 22 ignores it means the failure is **app-specific, not
version-gated**: HDRezka-Client (the author's own Electron+Vue3 app) almost certainly calls
`app.setPath('userData', …)` in its main process, which overrides the switch (🟡 — verifiable in its
source at `AI - automating systems/HDRezka client/` when convenient; worth checking during the
planned rewrite).

**Revised adapter rule:** treat `--user-data-dir=` as the default mechanism for ALL Electron apps
regardless of version, but **always confirm with a dynamic canary probe** — apps can override the
switch in their own main-process code. No static version gate is reliable in either direction.

## Cumulative Phase 0 status

| Exp | Status |
|---|---|
| E1 · E2 · E3-core · E4 · E5 · E6 · **E11** | ✅ done |
| ChatGPT Classic (ad-hoc) | ✅ done — UNSUPPORTED |
| E7 Telegram · E8 update-behavior · E9 wrappers · E10 leak detector | ⏳ pending |

**Verified-supported app list so far:** Claude, Notion, Notion Calendar, GitHub Desktop, OpenMTP,
VS Code, Antigravity, (Antigravity IDE by lineage). **Partial:** ChatGPT. **Unsupported:** ChatGPT
Classic, Gemini, HDRezka-Client (app override), Telegram (sandbox, pending E7 formality).
