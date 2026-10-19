# lumi-rs Entity Usage Audit (read-only)

Scope searched: all of `crates/**` excluding `crates/db/src/entities/**` and `crates/db/src/migrations/**`. Migrations use `get_schema_registry("lumi_db::entities::*")` (m0001_initial.rs:14), so **only modules declared in `entities/mod.rs` produce tables**.

Important discrepancy: there are 42 entity `.rs` files, but `entities/mod.rs` declares only **41** (it lists `enums` too). `addon_kv.rs` exists on disk but is **not declared in `mod.rs`**, so it is not part of the registry and is never created as a table either. So: 41 real tables + 1 orphan file.

## 1. All tables

| table | status | writes | reads | note |
|---|---|---|---|---|
| `addon_kv` | UNUSED | – | – | Orphan file, not in `mod.rs`; not even migrated. No `addon_kv::` refs anywhere. |
| `afk_entries` | USED | afk/data.rs:15 (`insert` upsert), :39 (`delete_many`) | afk/data.rs:31, :48, :56 | full CRUD |
| `allowlist` | UNUSED | – | – | no `allowlist::` refs anywhere |
| `appeals` | UNUSED | – | – | no refs; `AppealStatus` enum also unused |
| `audit_ledger` | UNUSED | – | – | no refs; `AuditPlatform` enum also unused |
| `blocklist` | UNUSED | – | – | no refs |
| `case_type_settings` | UNUSED | – | – | no refs |
| `counters` | USED | db/lib.rs:44 (`update`), :53 (`insert`) | db/lib.rs:32 (`find .one` lock) | only via `db::next_counter` |
| `downloader_modules` | USED | downloader.rs:377 (`insert`), :366 (`update`), :272/:555 (`delete_many`) | downloader.rs:256, :354, :384, :538 | |
| `downloader_repos` | USED | downloader.rs:185 (`insert`), :233 (`update`), :277 (`delete_by_id`) | downloader.rs:95, :190, :239, :247, :294, :392, :399 | |
| `economy_accounts` | USED | economy/store.rs:51 (`insert`), :82/:120/:167/:216/:285/:289/:326/:363 (`update`) | economy/store.rs:35 | |
| `economy_transactions` | USED | economy/store.rs:94/:132/:179/:229/:242/:301/:338/:375 (all `insert`) | **none** | **write-only ledger** |
| `feature_flags` | UNUSED | – | – | no refs |
| `feature_flag_overrides` | UNUSED | – | – | no refs |
| `gdpr_export_jobs` | UNUSED | – | – | no refs; `GdprExportJobStatus` enum unused |
| `global_config` | USED | downloader.rs:699 (`update`) | downloader.rs:671, :689; db/lib.rs:77 | singleton; **never inserted** (must be seeded) |
| `global_allowlist` | UNUSED | – | – | no refs |
| `global_blocklist` | UNUSED | – | – | no refs |
| `global_module_state` | USED | downloader.rs:577 (`update`), :587 (`insert`) | downloader.rs:408, :567; module_state.rs:32, :100 | |
| `guilds` | USED | db/lib.rs:96/:123 (`update`), :108 (`insert`) | db/lib.rs:65, :91, :116 (`find_by_id`) | |
| `guild_backups` | UNUSED | – | – | no refs |
| `guild_case_counter` | UNUSED | – | – | superseded by `counters` table |
| `guild_config` | USED | logging/store.rs:46 (`insert` upsert) | moderation/store.rs:145; logging/store.rs:16 | |
| `guild_module_config` | USED | module_state.rs:168 (`update`), :179 (`insert`), :210 (`delete_many`) | module_state.rs:136, :157 | |
| `guild_module_state` | USED | module_state.rs:70 (`update`), :80 (`insert`) | module_state.rs:41, :60, :92; db/lib.rs:130 (count) | |
| `ignore_list` | UNUSED | – | – | no refs |
| `level_accounts` | USED | leveling/store.rs:88 (`insert`), :115/:152/:185/:226/:259 (`update`), :269 (`delete_many`) | leveling/store.rs:63, :287, :301 | |
| `level_xp_gains` | USED | leveling/store.rs:126, :163 (`insert`) | **none** | **write-only ledger** |
| `mod_notes` | USED | moderation/store.rs:124 (`insert`) | moderation/store.rs:132 | |
| `moderation_cases` | USED | moderation/store.rs:46 (`insert`), :72 (`update`); security.rs:315 (`insert`) | moderation/store.rs:55, :85, :99 | |
| `module_config_history` | UNUSED | – | – | no refs |
| `module_config_overrides` | UNUSED | – | – | no refs; `OverrideTargetType` enum unused |
| `module_dynamic_data` | UNUSED | – | – | no refs |
| `module_scoped_config` | UNUSED | – | – | no refs |
| `panic_states` | USED | security.rs:204 (`insert` upsert), :224 (`delete_by_id`) | security.rs:145 (`find_by_id`) | |
| `permits` | USED | db/lib.rs:152 (`delete_by_id`), :297 (`update` position) | permits.rs:141; db/lib.rs:143, :200, :264 | **no INSERT anywhere** |
| `permit_assignments` | USED | db/lib.rs:236 (`insert`), :248 (`delete_many`) | permits.rs:142; db/lib.rs:173, :215 | |
| `shared_api_tokens` | UNUSED | – | – | no refs |
| `tempvc_generators` | USED | tempvc/store.rs:37 (`insert` upsert), :80 (`update`), :92 (`delete_many`) | tempvc/store.rs:46, :57 | |
| `tempvc_records` | USED | tempvc/store.rs:140 (`insert` upsert), :189/:206 (`update`), :218 (`delete_many`) | tempvc/store.rs:149, :160, :172 | |
| `verification_panels` | UNUSED | – | – | no refs |
| `warn_thresholds` | UNUSED | – | – | no refs |

## 2. UNUSED tables (key output)

Declared and migrated but never queried anywhere in `crates/**`:

1. `allowlist`
2. `appeals`
3. `audit_ledger`
4. `blocklist`
5. `case_type_settings`
6. `feature_flags`
7. `feature_flag_overrides`
8. `gdpr_export_jobs`
9. `global_allowlist`
10. `global_blocklist`
11. `guild_backups`
12. `guild_case_counter`
13. `ignore_list`
14. `module_config_history`
15. `module_config_overrides`
16. `module_dynamic_data`
17. `module_scoped_config`
18. `shared_api_tokens`
19. `verification_panels`
20. `warn_thresholds`

Plus the orphan entity file:
- `addon_kv` — not declared in `entities/mod.rs`, so it is not migrated and not queried. Dead file.

## 3. Write-only / read-only tables

- **Write-only** (inserted, never read back):
  - `economy_transactions` (all inserts in economy/store.rs; no `Entity::find`).
  - `level_xp_gains` (inserts only; no reads).
- **Read/mutate-only, never created by code**:
  - `permits` — read, deleted, position-updated, but no insert. Rows must be seeded out-of-band or the feature is empty.
  - `global_config` — read and updated, but no insert; the singleton row must be seeded.
- No table is strictly read-only.

## 4. Declared columns the code never touches

- `global_config`: only `default_prefix`, `auto_update_enabled`, `auto_update_interval_minutes`, `updated_at` (write) and `id` (order) are touched. Never referenced: `bot_name`, `maintenance_mode`, `maintenance_message`, `invite_url`, `support_guild_id`, `server_lock_enabled`, `server_lock_guild_ids`, `auto_update_last_checked_at`.
- `guilds`: `locale`, `ignored`, `left_at` are written only in the insert defaults (db/lib.rs:102-104); never read or updated.
- `economy_accounts`: `last_payday_at` is written (store.rs:81) but never read — no cooldown check on daily.
- `economy_transactions`: `id`, `reason`, `balance_after` only written; no reads at all.
- `downloader_repos`: `signed_by` only ever `Set(None)`; `url` written but never read back.
- `downloader_modules`: `signed_by`, `pinned` only ever `Set(None)`/`Set(false)`; never read.
- `tempvc_records`: `generator_id` only written on insert, never read/updated (store.rs:115 — set, but not in the upsert update column list); `locked` and `hidden` are only ever written, never read.
- `moderation_cases`: `custom_action` (store.rs:37, security.rs:306) and `expires_at` (store.rs:40, security.rs:309) are only ever `Set(None)`.
- `permits`: `created_at` is never set (no insert) and never read; `updated_at` only set during `move_permit`.
- `level_xp_gains` / `permit_assignments` / `mod_notes`: `created_at`/`updated_at` written, not read (except `mod_notes.created_at`, read in views.rs:88).

## Enum variant sets

- Fully unused enums (only referenced by their unused entity): `AppealStatus`, `AuditPlatform`, `OverrideTargetType`, `GdprExportJobStatus`.
- `CaseAction::AntinukeAlert` is never constructed or matched (all other variants appear; `VoiceMute`/`UnvoiceMute` appear only in `views.rs` match arms).
- `EconomyTxnKind::Deposit` and `EconomyTxnKind::Withdraw` are never used — `deposit()`/`withdraw()` log `TransferOut`/`TransferIn` instead (economy/store.rs:331, :368), so those two variants are dead and the deposit/withdraw logs are mislabeled.

---

Skipped: I did not open every `lumi`/`api`/`script` crate file individually; I relied on a whole-tree `\b<entity>::` grep plus `lumi_db`/`enums::` greps, which is exhaustive for entity references. Risk: `addon_kv` is dead code that will silently never migrate; `permits` and `global_config` have readers but no insert path, so on a fresh DB those features are empty until seeded.
