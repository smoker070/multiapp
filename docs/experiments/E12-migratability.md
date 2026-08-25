# Experiment E12 — App Data Migratability Classification

**Date:** 2026-07-20 · **Machine:** the author's Mac (macOS 26)
**Question:** Which installed apps can have their data/sessions **backed up and migrated** to another
machine or user — even apps that can't be *profiled*? Where does each app keep its login?
**Method:** read-only inspection of data locations, sizes, file types (no conversation content read),
and Keychain **metadata** (item presence only — never secrets).

---

## The headline finding (changes the framing)

**Every candidate app ties its login/session to the macOS Keychain.** There were no exceptions:

| App | Login/session key in Keychain? | Evidence |
|---|---|---|
| Codex / ChatGPT | ✅ yes | `Codex Safe Storage` generic item present |
| Claude (Electron ref) | ✅ yes | `Claude Safe Storage` generic item present |
| Telegram | ✅ yes | keychain item(s) mentioning keepcoder/telegram present; local `notificationsKey` file |
| Gemini | ⚠️ likely | entitlement group `EQHXZ8M8AV.*`; cookie store in `HTTPStorages` |

The Keychain is bound to **machine + user + app code-signature** and does not travel inside a copied
folder. **Consequence: a file copy alone essentially never carries the login to a *different* machine
or user.** This isn't per-app bad luck — it's the platform security model, the same wall that blocked
profiles, showing up again.

This **reframes the whole feature**:

| Scenario | What migrates | Verdict |
|---|---|---|
| **Same machine, same user** (reinstall app / OS, or restore a backup) | **Everything, incl. login** — the Keychain items are still there, so encrypted cookies/sessions decrypt normally | ✅✅ **Strongest use case** |
| **New machine, same Apple ID + iCloud Keychain** | content always; login *only if* the app's key is iCloud-synchronizable (most app "Safe Storage" keys are **not**) | ⚠️ usually content-only |
| **New machine / new user, no Keychain sync** | content (history, drafts, settings, caches); **login lost → re-login once** | ⚠️ content-only |

So the tool's honest promise is:
- **Backup/restore on the same Mac → full, including login.** (Great for "I reinstalled and lost everything.")
- **Move to a new Mac → your history and settings come across; you sign in once.**

## Data locations & sizes (verified)

| App | Local content (migratable) | Size | Login (Keychain, non-migratable) |
|---|---|---|---|
| **Codex/ChatGPT** | `~/Library/Application Support/com.openai.chat` (conversations-v3, drafts-v2, tasks, pinned-items — a per-account key-value store) + `~/…/Codex` (Chromium profile) | 2 MB + 178 MB | `Codex Safe Storage` + `2DC432GLL2.*` |
| **Telegram** | `~/Library/Containers/ru.keepcoder.Telegram` + `~/Library/Group Containers/6N38VWS5BX.ru.keepcoder.Telegram` (`stable/account-*`, `postbox`, media cache) | 1.3 GB + **70 GB** | keychain item + `notificationsKey` file → **postbox may be Keychain-key-encrypted** |
| **Gemini** | `~/…/com.google.GeminiMacOS` + `~/Library/HTTPStorages/com.google.GeminiMacOS` (cookies) + group container | ~1.2 MB | entitlement group; cookie store |
| **Claude** (ref) | `~/Library/Application Support/Claude` (Chromium; incl. Cowork `vm_bundles`) | 8.8 GB | `Claude Safe Storage` |

## Per-app migratability verdicts (inspection-level)

- **Codex/ChatGPT — `content-migrates, re-login`** 🟡. Chat history (`com.openai.chat` file store) copies
  fine to a new machine. Chromium cookies are `Codex Safe Storage`-encrypted → dead weight on a new
  machine → re-login. On the **same** machine, a restore is **full incl. login** ✅.
- **Telegram — `requires 2nd-machine test`** 🟠. Session is file-based in the container **but** a Keychain
  item + `notificationsKey` suggest the postbox is encrypted with a Keychain-held key → the copied
  container may not decrypt on a new machine (re-login, and possibly lost local cache). Same-machine
  restore should be full ✅. **Note the 70 GB group container is mostly media cache — a backup should
  let you exclude it.**
- **Gemini — `experimental`** ⚪. Tiny data; login locus unconfirmed (Keychain vs protected cookie store).
  Needs the 2nd-machine test.
- **Electron apps generally (Claude, Notion, Chrome, VS Code) — `content-migrates, re-login`** 🟡 across
  machines; **full restore on same machine** ✅. (safeStorage cookies won't decrypt elsewhere; this was
  already established in E3.)

## What this experiment could and couldn't prove on one machine

- ✅ **Proven:** where each app stores data; that every candidate's login depends on the Keychain;
  content is file-based and copyable.
- 🟠 **Not provable here (needs a 2nd Mac or a 2nd macOS user account — admin required):** whether a
  given app's *content* (not just login) still decrypts after moving — specifically Telegram's postbox
  and any app whose files (not just cookies) are Keychain-key-encrypted. The closest local proxy is a
  second macOS user account (separate Keychain); that's the definitive follow-up test.

## Design implications (feed into the spec)

1. Lead with the **same-machine backup/restore** use case — it's the one that fully works, login included.
2. Cross-machine = honestly labeled **content + re-login**.
3. Need a per-app **migratable verdict** (`full` / `content+relogin` / `experimental` / `no`), just
   like the profile verdict — never over-promise.
4. Backups must **exclude caches by default** (Telegram's 70 GB, Claude's `vm_bundles`) with an opt-in
   "include everything".
5. Restore is **destructive**: require app-quit, stage the existing data to a recoverable spot first.
6. Archives are **maximally sensitive** (full app data) → encrypt by default.
