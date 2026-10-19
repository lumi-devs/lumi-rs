# Addon/module dynamic data store (TODO — not implemented)

## Why this exists
Addons (see `/home/rebiz/opt/lumi-addons`) persist their own data through a dynamic
key/value store. In the TS reference this is `services.db.guildKV`
(`src/lib/prisma/repositories/GuildKvRepository.ts`) backed by the Prisma model
`ModuleData` → table `module_dynamic_data`.

Usage across the real addons is dominated by `db.guildKV` (activity-roles,
booster-roles, confessions/rolementions, dragme, economy-games, multi-lounge,
promoter, status, thread-cleaner, utility). It is the single most-used addon
storage API.

## The store (already modelled in Rust — do NOT drop it)
Rust entity `crates/db/src/entities/module_data.rs` already maps the same table:

```
table module_dynamic_data
PK (guild_id, module_name, target_id, key)
value: Json
expires_at: Option<timestamptz>
```

This is the correct home for addon data. `addon_kv` (an orphan entity, never
declared in `entities/mod.rs` and never migrated) was removed as dead — it
duplicated this table with an incompatible key shape.

## API surface the addons need (TS `db.guildKV`)
- `getModuleData(guildId, module, targetId, key) -> T | null` (expiry-aware; deletes stale rows)
- `getModuleDataMany(guildId, module, [{targetId, key}]) -> Map<"targetId:key", T>`
- `setModuleData(guildId, module, targetId, key, value, { ttlMs? })` (upsert; ensures the guild row)
- `purgeExpiredModuleData() -> { count }`
- `listModuleData({ module, key, guildId? }) -> [{ guildId, targetId, value }]`
- `listGuildModuleData(guildId, { moduleName?, targetId?, key?, skip?, take? }) -> { entries, total }`
- `listModuleDataForTarget(module, targetId)` and `deleteModuleDataForTarget(module, targetId)` (GDPR: self-data export/erase)

## Design constraints for the Rust version (decided)
- **No shims, clean code.** Implement a proper storage module (a typed repository
  over the `module_data` entity) — do NOT add a compatibility alias layer or a
  second KV table.
- Values are `serde_json::Value` (JSONB), matching the `Json` column and the
  `module_dynamic_data` GIN index (add the GIN index when implementing, via a
  migration, since it was skipped in `m0002`).
- Expiry is enforced in the read path and cleared by a periodic purge; the
  scheduler is currently a stub (see `todo/05-modules-services-parity.md`), so the
  purge needs the scheduler round first or a best-effort on-read delete.

## Build it when
The addon/Rhai platform round — the store is worthless without host bindings that
expose it to addons (`@lumi/db` equivalent). Do the store and its host bindings
together, not separately.

## Out of scope now
Adding the repository helper before any addon consumes it would be dead code;
that is why it is deferred rather than stubbed.
