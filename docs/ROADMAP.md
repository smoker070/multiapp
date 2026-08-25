# Multiapp — Phased Roadmap (Working Version)

> Working companion to `REPORT.md` §24. The report is the rationale; this file is the checklist.
> Rule of the project: **no phase starts until the previous phase's acceptance criteria are checked off.**

---

## Phase 0 — Proof of Concept (2–3 weeks) — CURRENT NEXT STEP

Goal: convert every 🟡/🟠 claim in REPORT.md into a verified fact before writing product code.

### Experiments (from REPORT.md §23; evidence in `experiments/`)
- [x] **E1** ✅ DONE 2026-07-15 (`experiments/E1-results.md`) — `open -n` propagates env (PASS), **but**
      `NSHomeDirectory`/`NSSearchPath…`/Electron `userData` ignore `$HOME` on macOS 26; the working
      lever is `--user-data-dir=/path` (**equals form!**), honored by Electron core ≥ ~36
      (verified: 43.1.1 ✅, Claude's 42.5.1 ✅, Electron 22 ❌)
- [x] **E2** ✅ FULL PASS 2026-07-15 (`experiments/E2-E3-results.md`) — two Claude accounts logged in
      simultaneously (real profile + `Multiapp/Profiles/claude/e2-test`), sessions fully separated
      (Cookies/Local Storage/IndexedDB per profile), all helpers pinned to their profile
- [x] **E3 (core)** ✅ 2026-07-15 — second account survived quit + relaunch (safeStorage decrypted;
      single shared "Claude Safe Storage" keychain key confirmed, metadata-only check).
      **Remaining:** long-run token refresh; behavior if a Claude update rotates the key
- [x] **Session transfer VERIFIED end-to-end 2026-07-18 (by the author, live)** — sessions transferred
      main → `second` profile appear in the second ACCOUNT's UI ⇒ cross-account visibility of
      copied `local_*.json` index files is CONFIRMED (they carry no account binding; the
      account-scoped folder path is the only scoping). Transfer feature now also in the
      menu-bar app (checkbox dialog, claude profiles)
- [x] **E4** ✅ DONE 2026-07-15 (`experiments/E4-E6-results.md`) — **ChatGPT is now a Chromium 150
      embed** (rebuilt 2026-07-14, Atlas lineage; old "native Swift" classification stale). Honors
      `--user-data-dir=`, two concurrent instances OK, real profile untouched — **but a fresh profile
      auto-logs-in with the existing account** (Keychain-injected identity) → **verdict: PARTIAL**
      (data isolates, account shared; two different accounts impossible via redirection). ⚠️ never
      test sign-out on a live account — may kill all profiles' sessions
- [x] **E5** ✅ PASS for Claude 2026-07-15 — full login + restart-persistence flow ran with NO
      Keychains symlink and NO env overrides; multigravity's symlink is vestigial. Multiapp will
      never touch Keychains paths
- [x] **E6** ✅ DONE 2026-07-15 — Gemini is genuinely native Swift; no flags, no levers
      (HTTPStorages/group containers bundle-ID-keyed). Two instances CAN run concurrently but share
      ALL state → **verdict: UNSUPPORTED** for profiles (adapter should refuse honestly, point to
      hard-isolation tier)
- [ ] **E7** Telegram (sandboxed): confirm container NOT redirectable (expected fail → T4 exclusion)
- [ ] **E8** Squirrel auto-update while two Claude profiles run → supervisor strategy
- [~] **E9** PARTIAL 2026-07-15 — wrapper `.app` generation works (osacompile applet + borrowed icon;
      plain script-bundles are silently refused by LS on macOS 26). One-click Dock launch verified
      via `Claude – e2-test.app`. Remaining: formal two-wrapper Dock/Cmd-Tab UX documentation
- [ ] **E10** Leak-detector prototype (fs snapshot/diff) distinguishes isolated vs leaked reliably
      (marker-file `find -newer` approach from E2-core is the seed)
- [x] **E11** ✅ DONE 2026-07-15 (`experiments/E11-electron-sweep-results.md`) — swept 8 Electron
      apps: OpenMTP(18)✅, GitHub Desktop(40)✅, Antigravity(41)✅, Notion Calendar(41.5)✅,
      Notion(42.3)✅, VS Code(42.5)✅, Claude(42.5)✅ — only HDRezka(22)❌. **"Version floor"
      hypothesis DEAD**: Electron 18 honors the flag; failures are app-specific
      (`app.setPath('userData')` overrides) → dynamic canary probe mandatory, no static version gate.
      Also: **ChatGPT Classic (`com.openai.chat`) = UNSUPPORTED** (native, no levers, shares the
      OpenAI keychain token `2DC432GLL2.com.openai.shared` with the new app) — skipped per decision

### Deliverables
- [x] Experiment write-ups in `experiments/` (E1, E2-E3, E4-E6, E11 files)
- [x] Compatibility matrix updated with verified verdicts (REPORT.md §8)
- [x] Probe-engine prototype — `multiapp probe <app>` in `prototype/multiapp`
- [x] **BONUS: working prototype CLI** `prototype/multiapp` — validated live on Claude (2nd account
      incl. Dock wrapper) and full lifecycle on Notion/Chrome
- [x] **v0.2.0 (2026-07-18): cross-platform + move-proof**
      - macOS + **Linux** in one bash script (platform layer: launch/paths/wrappers/scan branch on
        `uname`); Linux implemented per multigravity's verified patterns, **pending real-Linux test**
      - **Windows** `prototype/multiapp.ps1` (PowerShell, same `--user-data-dir=` mechanism),
        **pending real-Windows test** (not syntax-checkable on macOS)
      - **move-proof launcher**: `install-stub` writes a self-locating stub to `~/.local/bin/multiapp`;
        self-heals via `mdfind`/`locate` (NOT recursive find — that hung for minutes on the cloud drive).
        Folder can now be moved/renamed without breaking the command or wrappers
      - added `sessions/transfer/export/import` (Claude Code session moving, macOS/Linux)
- [x] **Menu-bar app v0.1.0 (2026-07-18)** — `prototype/app/` (single-file AppKit Swift, built with
      swiftc, no Xcode); thin GUI over the CLI engine; ad-hoc signed → `~/Applications/Multiapp.app`
      + `Multiapp-0.1.0.dmg`. Distribution to others requires Developer ID + notarization ($99/yr) —
      deliberately deferred. Cross-platform GUI (Tauri → exe/AppImage) deferred until Windows/Linux
      mechanisms are hardware-verified

### Acceptance criteria
- All 10 experiments executed with recorded evidence
- Leak detector: <5% false verdicts across 3 runs per app
- Go/no-go memo written

### Stop / redesign triggers
- **E1 fails and direct-spawn fallback also fails** → Architecture A unbuildable as designed
- **Claude AND generic Electron fail isolation** → Arch A value collapses → redesign around C1 (multi-user) or stop

---

## Phase 1 — MVP (4–8 weeks)

### Deliverables
- [ ] Core library (ProfileStore, AppInspector, CompatibilityEngine, AdapterRegistry, Launcher, ProcessSupervisor)
- [ ] CLI: `create / list / launch / clone / delete / export / import / doctor / probe`
- [ ] Adapters: `vscode-family`, `electron-user-data-dir` (Electron-version-gated), `claude`
      (`--user-data-dir=` equals form — home-redirect adapters retired after E1b)
- [ ] Wrapper `.app` generation (compiled stub, our signature — no shell scripts)
- [ ] Menu-bar app (minimal), signed + notarized
- [ ] Safety set: staged Trash deletion, sanitized import, atomic manifests, per-profile locks

### Acceptance criteria
- New user creates 2 Claude profiles with different accounts, runs both concurrently, in < 2 minutes
- Delete is staged and recoverable for 30 days; restore tested
- Import rejects path-traversal / symlink-member archives (automated tests)
- Audit: zero writes outside product root + profile dirs
- Every UI verdict equals the probe result (no optimistic labels)

### Risks
- Adapter maintenance cost; Squirrel update mid-run (mitigation from E8)

### Stop / redesign trigger
- \>30% of tier-1 target apps end "unsupported" → reassess product scope before investing in UI polish

---

## Phase 2 — Beta Hardening (4–6 weeks)

### Deliverables
- [ ] Onboarding + honest compatibility UX (supported / partial / unsupported / built-in alternative)
- [ ] Templates; encrypted export (secrets excluded by default)
- [ ] Sparkle auto-update for our app
- [ ] Diagnostics bundle (no telemetry), `doctor` repair flows
- [ ] Intel Mac validation run of E1–E10
- [ ] Docs incl. ToS-guidance page; user-supplied adapter manifests (env/args only)

### Acceptance criteria
- 20 external beta users
- Profile-data-loss incidents < 2% (target 0)
- `doctor` repairs 100% of artificially corrupted states in the test matrix
- App-update re-probe verified across ≥ 2 real updates of Claude / VS Code

### Stop / redesign trigger
- Any confirmed data-loss bug class without a staging/rollback fix → halt release train

---

## Phase 3 — Production Readiness (6+ weeks)

### Deliverables
- [ ] 1.0 Developer ID release
- [ ] Legal review: ToS positioning, trademark use of third-party names/icons
- [ ] Adapter update channel (signed manifests)
- [ ] Decision gate: long-term hard-isolation tier (C1 multi-user pilot behind a flag)
- [ ] Localization uz / ru / en

### Acceptance criteria
- Crash-free sessions > 99.5%
- Probe verdicts stable across one quarter of app updates
- Uninstall leaves zero orphans (automated test)

### Stop / redesign trigger
- Legal review rejects trademarked-app presentation → rework naming/branding before launch

---

## Standing constraints (all phases)
- Never modify, patch, or re-sign a third-party binary
- Never read, copy, export, or symlink Keychain data
- Never present unverified isolation as supported
- Every destructive operation: confirm → stage → log → recoverable
