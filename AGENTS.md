# AGENTS.md — lumi-rs

Guidance for agents working in this repository.

## Agent docs

Detailed per-topic docs live in `agents/`. Read them before touching anything:

| File | Topic |
|------|-------|
| `agents/00-overview.md` | Index |
| `agents/01-workspace.md` | Crate layout, dep direction |
| `agents/02-modules.md` | Module trait, adding a feature |
| `agents/03-db.md` | Entities, migrations, no-raw-SQL rule |
| `agents/04-ui.md` | V2 cards/panels, reference parity |
| `agents/05-script.md` | Rhai host, caps, SSRF |
| `agents/06-architecture.md` | Cluster model, shard math, scale |
| `agents/07-build.md` | Colab build, flake, profiles |
| `agents/08-conventions.md` | Lints, errors, tracing, naming |

## What this is
A new Rust Discord bot (`lumi`), multi-crate Cargo workspace, built to scale to millions of
guilds/users. It is **not** a port of the TS `lumi` project — the TS repo is a
read-only reference for behaviour and UI only.

## Hard rules

1. **Modules MUST NEVER use raw SQL.** No `Statement::from_*`, `execute_raw`,
   `query_all_raw`, `query_one_raw`, `sea_query` string SQL, or any SQL text
   anywhere under `crates/modules/**`, `crates/ui/**` or `crates/script/**`.
   - Use **SeaORM 2.0** (entities + typed queries) exclusively.
   - If something cannot be expressed with the ORM, add a **typed helper** in a
     sanctioned place and call it from the module:
     - generic DB helpers → `crates/db/src/lib.rs` (e.g. `lumi_db::next_counter`),
     - feature-specific persistence → the module's own `store.rs`.
   - The ONLY place raw SQL is ever permitted is inside `crates/db/**`, and only
     when the ORM genuinely cannot do it. Even there, prefer the ORM first.
2. **UI is Components V2 only — never embeds.** Use `crates/ui/*`
   (`Card`, `CardKind`, `CardOptions`, `CardReply::{reply,message}`,
   `make_*_card`, `make_list_card`, panels/containers/layout). Match the TS
   `lumi` cards closely. Disallowed method `CreateMessage::embed` is enforced
   via `clippy.toml`.
3. **Modules are self-contained** under `crates/modules/src/<name>/`:
   `mod.rs` (the `Module` impl), `commands.rs`, `events.rs`, `store.rs`,
   `views.rs`. Register the module in `crates/lumi/src/client.rs`, aggregate its commands in
   `crates/lumi/src/commands/mod.rs::all()`, and dispatch its events in `crates/lumi/src/events/mod.rs`.
4. **Version comes from Cargo** — `env!("CARGO_PKG_VERSION")`. Never hardcode.
5. **Purpose-built crates over hand-rolled.** Use serenity/poise/sea-orm/apalis/
   governor/redis/moka helpers. If unsure whether a crate exists, check
   Context7/MCP before writing your own.
6. **Strict lints are on** (see `Cargo.toml` `[workspace.lints]`): `unsafe_code = "forbid"`,
   `unwrap_used`/`expect_used`/`disallowed_methods` deny, clippy `correctness`/`suspicious`
   deny, `pedantic`/`nursery`/`unused`/`dead_code` warn. Do not add `#[allow]` for a denied lint.
   No `todo!()`/`unimplemented!()`; log with `tracing`, never `println!`.
7. **Never edit the reference repos** (`../lumi`, `../lumi-addons`, …).
8. **Storage is entity-first**: add an entity under `crates/db/src/entities/` and declare
   it in `crates/db/src/entities/mod.rs`; the baseline migration schema-syncs
   `lumi_db::entities::*`. Add composite indexes in the baseline migration.
9. **IDs are strongly typed**: use `GuildId`, `UserId`, `ChannelId`, `SnowflakeId` from
   `lumi_db::ids::*` for snowflake fields (stored as `BIGINT`). Convert at the edge.

## Build & verify
```
nix develop "path:/home/rebiz/opt/lumi-rs" --command cargo clippy --all-targets
nix develop "path:/home/rebiz/opt/lumi-rs" --command cargo test --workspace
```
(`path:` prefix is required; the flake lives in a dirty git tree.) Aim for 0
clippy errors.

## Workspace Layout
- `crates/db` — SeaORM entities, migrations (`m0001_initial`), snowflake newtypes (`ids`), connection helper.
- `crates/kernel` — Core runtime, config, services, permits, module state, rate limits.
- `crates/ui` — Components V2 presentation toolkit (cards, buttons, pagination).
- `crates/modules` — Self-contained feature modules (moderation, tempvc, leveling, economy, etc.).
- `crates/api` — Dashboard REST API and RPC handlers.
- `crates/script` — Rhai scripting host and SSRF protection.
- `crates/lumi` — Main Discord bot binary (poise framework, gateway events, command assembly).
- `xtask` — Operational helpers (e.g. `xtask migrate`).

Keep `todo.md` current.
