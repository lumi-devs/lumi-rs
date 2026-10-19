# DB Parity Report: Prisma (TS) vs SeaORM (Rust)

Scope: `/home/rebiz/opt/lumi/prisma/schema.prisma` (41 models) vs `lumi-rs/crates/db/src/entities/*.rs` (42 entities so far, minus `enums.rs`/`mod.rs`). Migrations in Rust are auto-generated from the entity registry (`m0001_initial.rs` calls `sync`), so entity declarations are the Rust source of truth; Prisma migration SQL (`20261011000000_v1`) confirms actual column names.

## 1. Tables only in TS (Prisma, no Rust entity)

| Table | Prisma model |
|---|---|
| `reactionrole_menus` | `ReactionRoleMenu` |
| `reactionrole_options` | `ReactionRoleOption` |

No Rust entity for the reaction-role subsystem at all. Also note `ReactionRoleMenu.messageIds` has a GIN index and `ReactionRoleOption` has a composite unique — both entirely unmodeled in Rust.

## 2. Tables only in Rust (entity, no Prisma model)

| Table | Rust entity | Notes |
|---|---|---|
| `addon_kv` | `addon_kv` | PK `(guild_id, addon, key)`; `guild_id` is `i64` |
| `counters` | `counter` | generic `(guild_id, name) -> value`; overlaps `guild_case_counter` purpose |
| `guild_config` | `guild_config` | PK `(guild_id, module, key)`; `guild_id` is `i64` |

`addon_kv`/`guild_config` use `guild_id: i64`, whereas every guild-scoped Prisma table uses `VARCHAR(20)`. These are Rust-only so no direct conflict, but the ID type is inconsistent within the Rust schema itself.

## 3. Tables in both — behavioural differences

### 3a. Column-name mismatches (breaking — Rust reads/writes the wrong column)

| Table | Prisma column (field) | Rust column (field) |
|---|---|---|
| `guilds` | `guild_id` (`id @map("guild_id")`) | `id` (`pub id`) |
| `moderation_cases` | `duration_seconds` (`duration @map(...)`) | `duration` (`pub duration`) |
| `warn_thresholds` | `duration_seconds` (`duration @map(...)`) | `duration` (`pub duration`) |

Confirmed against SQL: `guilds` PK is `"guild_id"`, and both tables have `"duration_seconds" INTEGER`. SeaORM derives the column from the Rust field name, so these three columns will not line up against the existing Prisma-created database.

### 3b. Type-level mismatches (apply to every shared table)

- **Enums**: Prisma columns are native Postgres enum types (`CREATE TYPE "CaseAction" AS ENUM (...)` etc. — `kind "PermitKind"`, `action "CaseAction"`, `platform "AuditPlatform"`, `status "GdprExportJobStatus"`, `kind "EconomyTxnKind"`). Rust `DeriveActiveEnum` uses `db_type = "String(StringLen::None)"` (varchar/text). This can break prepared-statement binding/projection against the native enum columns, and a Rust-created schema would use text columns instead of enums.
- **Timestamps**: Prisma emits `TIMESTAMP(3)` (no time zone); Rust uses `DateTimeWithTimeZone` (timestamptz). Semantics differ for `left_at`, `expires_at`, `last_xp_at`, etc.
- **String lengths**: Prisma `@db.VarChar(N)` constraints (20/64/…) are not modeled in Rust (`String` = unbounded), so length limits are not enforced Rust-side.
- **DB defaults**: entity declarations carry no defaults. Prisma defaults such as `Global.id=1`, `Guild.locale='en-US'`, `Guild.ignored=false`, `GuildModuleState.enabled=true`, `Permit.polarity='grant'`, `Blocklist/…`, `AfkEntry.reason='AFK'`, `TempVcGenerator.limit=0`, etc. are absent in Rust. Rust inserts must supply values explicitly or they fail / differ.

### 3c. Column presence / optionality

No missing or extra columns were found in tables present on both sides (aside from the name mismatches above). Optionality (`Option<...>` vs `?`) matches for `reason`, `reviewed_by`, `reviewed_at`, `error`, `file_path`, `size_bytes`, `completed_at`, `expires_at`, `commit`, `signed_by`, `version`, `duration`/`duration_seconds`, etc. `BigInt` → `i64` and `Int` → `i32` mappings are consistent.

Tables with no differences beyond 3b: `global_config`, `global_module_state`, `guild_module_state`, `guild_module_config`, `module_scoped_config`, `permits`, `permit_assignments`, `guild_case_counter`, `appeals`, `mod_notes`, `global_blocklist`, `blocklist`, `global_allowlist`, `allowlist`, `feature_flags`, `feature_flag_overrides`, `gdpr_export_jobs`, `ignore_list`, `afk_entries`, `downloader_repos`, `downloader_modules`, `audit_ledger`, `module_dynamic_data`, `module_config_history`, `module_config_overrides`, `case_type_settings`, `panic_states`, `verification_panels`, `guild_backups`, `tempvc_generators`, `tempvc_records`, `level_accounts`, `level_xp_gains`, `economy_accounts`, `economy_transactions`, `shared_api_tokens`.

## 4. Enums / bitflags

All 9 Prisma enums are mirrored in `entities/enums.rs`, and every variant string matches exactly:

- `PermitKind` (enforced, custom), `PermitPolarity` (grant, deny), `PermitTargetType` (user, role, channel), `CaseAction` (12 variants incl. `antinuke_alert`), `AppealStatus` (pending, approved, denied, denied_blacklisted, dismissed), `OverrideTargetType` (channel, category, role, user), `AuditPlatform` (discord, web), `GdprExportJobStatus` (pending, running, done, failed), `EconomyTxnKind` (10 variants).

No bitflags exist anywhere in `crates/` (grep found none). The only enum issue is the storage representation (native PG enum vs `String`) described in 3b.

## 5. Indexes / relations / unique constraints

Rust entities declare only **two** unique constraints: `Appeal.case_id` (`#[sea_orm(unique)]`) and `DownloaderRepo.name` (`#[sea_orm(unique)]`). No `#[sea_orm(indexed)]` anywhere and no FK relations are declared.

Missing in Rust vs Prisma (all `@@unique` except the one covered):

- `permits`: `@@unique([guild_id, name])`
- `permit_assignments`: `@@unique([permit_id, target_type, target_id])`
- `moderation_cases`: `@@unique([guild_id, case_number])`
- `blocklist`: `@@unique([user_id, guild_id])`
- `allowlist`: `@@unique([user_id, guild_id])`
- `feature_flag_overrides`: `@@unique([flag_key, guild_id])`
- `ignore_list`: `@@unique([guild_id, channel_id])`
- `module_config_overrides`: `@@unique([guild_id, module_name, key, model_type, model_id])`
- `reactionrole_options`: `@@unique([menu_id, slug])` (table absent entirely)
- `DownloaderRepo.name` and `Appeal.case_id` are the only two Rust carries over.

Indexes: Rust models **none** of Prisma's `@@index`es. Notable ones whose absence changes query plans/behaviour: `moderation_cases` (10 indexes, incl. `[guild_id, case_number]`, `[guild_id, active, expires_at]`, several partial-looking `active/id` orders), `appeals` (`[guild_id, status, created_at]`, `[status, created_at, id]`), `guilds ([left_at])`, `module_dynamic_data` GIN on `value`, `reactionrole_menus` GIN on `message_ids`, `audit_ledger`, `level_accounts ([guild_id, level, xp])`, `economy_transactions`, `module_config_history`, `gdpr_export_jobs`, plus many single-column `user_id`/`actor_id` lookup indexes. All absent Rust-side.

No index/unique exists in Rust that is absent from Prisma.

---

**Biggest break risks, in order:** (1) `guilds.id` vs `guild_id` — every guild-scoped query. (2) `duration` vs `duration_seconds` on `moderation_cases` and `warn_thresholds`. (3) native PG enum columns vs Rust text binding. (4) all composite unique constraints missing — duplicate rows now possible in Rust paths. (5) `reactionrole_*` tables/entities entirely absent.

Not checked: no other crates define entities (verified by grep); runtime bind/cast behaviour was inferred from types, not executed (commands were restricted to read-only). Risk: I assumed SeaORM derives column names as plain snake_case of the field — true for these entities, but any `#[sea_orm(column_name=...)]` overrides would be missed; none were present.
