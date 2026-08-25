# Multiapp — Backup & Migrate Feature Spec (v0.1 design)

**Status:** design only — no code yet · **Date:** 2026-07-20
**Grounded in:** `experiments/E12-migratability.md`

---

## 1. Why this is a separate feature from Profiles

Profiles solve **concurrent isolation** (run two logins of one app at once) — blocked by sandboxing
and Keychain. Backup/Migrate solves **moving an app's data through time or across machines** — no
concurrency needed, so it works for many apps that can't be profiled (sandboxed, native, Keychain-auth).

They share plumbing (archive/restore, safety staging) but are **distinct commands with distinct
verdicts**. Do not merge them into the profile export/import.

## 2. The core truth this feature is built around (from E12)

The macOS Keychain holds every tested app's login and does **not** travel in a file copy. Therefore:

| Use case | Promise | Reliability |
|---|---|---|
| **Restore on the same Mac/user with the Keychain intact** (app reinstall, oops-recovery) | **full, including login** — the Keychain still holds the key/token, so encrypted cookies decrypt | ✅ works (login inferred, see below) |
| **Fresh macOS install** (Keychain wiped), **new Mac**, or **new user** | **history + settings move; sign in once** | ⚠️ content-only |

**Corrected 2026-07-23:** an earlier draft said "OS reinstall" was in the full-restore case. That is
wrong — erasing the Mac wipes the login Keychain, so a fresh macOS install behaves like a new machine
unless the Keychain is restored separately (Migration Assistant / Time Machine).

**Verified by inspection (2026-07-23):** Electron archives *do* contain `Cookies`/`Local State`/
`Local Storage`/`IndexedDB`, and those cookies carry the `v10` prefix = encrypted with a Keychain-held
key. Native-app archives contain **no** session files at all (token is Keychain-only).
**Still inference, not tested:** an actual backup → restore → still-logged-in round trip. The reasoning
rests on E3 (verified: the Keychain key persists across restarts and decrypts the cookie store), but
the round trip itself should be click-tested before the claim is stated as fact in user-facing copy.

The product must **lead with same-machine backup** (the reliable win) and label cross-machine migration
honestly as *content + re-login*. Never imply logins teleport.

## 3. Commands (proposed)

```
multiapp backup  <app> [out.tar.zst]        # archive an app's local data (caches excluded by default)
multiapp restore <app> <archive> [--force]  # quit app → stage current data → restore
multiapp migrate-list                        # per-app migratable verdicts on this machine
multiapp backup  <app> --include-cache       # opt in to the big media/vm caches
```

- Operates on the app's **real data locations** (not a profile dir) — this is machine-level backup.
- `<app>` uses the same registry keys as profiles; a **separate `migratable` field** governs it.

## 4. Per-app adapter — new fields

Extend the existing registry entry with a backup block (illustrative):

```jsonc
{
  "id": "com.openai.codex",
  "backup": {
    "verdict": "content-relogin",         // full | content-relogin | experimental | no
    "dataPaths": [                          // copied on backup
      "~/Library/Application Support/com.openai.chat",
      "~/Library/Application Support/Codex"
    ],
    "cachePaths": [                         // excluded unless --include-cache
      "~/Library/Application Support/Codex/Cache",
      "~/Library/Application Support/Codex/Code Cache"
    ],
    "authLocus": "keychain",               // keychain | file | mixed  → drives the verdict/notes
    "notes": "Chat history migrates; login is Keychain-bound → re-login on a new Mac."
  }
}
```

### Verdict ladder (shown in `migrate-list` and before every backup/restore)

| Verdict | Meaning |
|---|---|
| `full` | same-machine restore is complete incl. login; cross-machine = content + re-login |
| `content-relogin` | as above — the common case for Keychain-auth apps |
| `experimental` | data locations known but decrypt-after-move unverified (needs 2nd-machine test) |
| `no` | data is device-bound/unreadable after move, or app forbids it |

## 5. Verdicts for the apps we inspected (E12)

| App | Backup verdict | Same-Mac restore | New-Mac migrate |
|---|---|---|---|
| Codex / ChatGPT | `content-relogin` | full ✅ | history ✅, re-login |
| Claude, Notion, Chrome, VS Code (Electron) | `content-relogin` | full ✅ | data ✅, re-login |
| Telegram | `experimental` | likely full ✅ | **needs test** (postbox may be Keychain-key-encrypted); exclude 70 GB cache |
| Gemini | `experimental` | likely full ✅ | needs test |

## 6. Backup flow

1. Resolve `dataPaths` for the app; refuse if the app is **running** (`is_running`-style check on its
   real process) — a live app would produce an inconsistent copy.
2. Sum sizes; if `cachePaths` are large, report what's being skipped (e.g. "excluding 70 GB of cache —
   use --include-cache to keep it").
3. `tar --zstd` the data paths into `out` with a **manifest** header: app id, app version, source
   machine name, macOS version, date, `authLocus`, verdict, and whether caches were included.
4. Default output encrypted (age/AES); warn the archive is sensitive.

## 7. Restore flow (destructive — full safety)

1. Read manifest; **warn** if the archive's app version differs from the installed one.
2. Require the app to be **quit**.
3. **Stage current data**: move the existing `dataPaths` into `~/…/Multiapp/Trash/restore-backup-<ts>/`
   (recoverable) — never overwrite in place.
4. Extract archive to the real locations.
5. On cross-machine restore, print the honest note: "History restored. You'll be asked to sign in
   because the login stays on the original Mac (Keychain)."
6. Rollback path: if extraction fails, move the staged data back.

## 8. Safety & privacy rules (inherit + extend)

- Never read, copy, or migrate **Keychain** items (unchanged rule) — that's *why* login doesn't move.
- Archives are the most sensitive artifact the tool makes → **encrypt by default**, and never place in
  a synced/cloud folder without explicit opt-in.
- Path-traversal validation on restore (reject absolute/`..`/symlink members), as with profile import.
- Sandboxed-app restore caveat (Telegram): restoring a container may trip `containermanagerd`
  permission resets → part of the `experimental` verdict until the 2nd-machine test clears it.

## 9. What's still needed before building

1. **The 2nd-machine (or 2nd-user) test** for Telegram + Gemini → turn `experimental` into a real
   verdict. Definitive proxy: a second macOS user account (separate Keychain) — restore there and see
   if content decrypts / login survives.
2. Decide MVP scope: I'd ship **same-machine backup/restore first** (the reliable, high-value case),
   then add cross-machine migrate once the experimental verdicts are settled.

## 10. Positioning

This makes Multiapp *"profiles **and** backup/migrate"* — and the backup half applies to **far more
apps** than profiles do (anything with file-based local data, sandboxed or not). Worth a line in the
README once shipped.
