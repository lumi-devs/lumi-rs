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
| `agents/04-ui.md` | V2 cards/panels, TS parity |
| `agents/05-script.md` | Rhai host, caps, SSRF |
| `agents/06-architecture.md` | Cluster model, shard math, scale |
| `agents/07-build.md` | Colab build, flake, profiles |
| `agents/08-conventions.md` | Lints, errors, tracing, naming |

## What this is
A new Rust Discord bot (`lumi`), single crate, built to scale to millions of
guilds/users. It is **not** a port of the TS `lumi` project — the TS repo is a
read-only reference for behaviour and UI only.

## Hard rules

1. **Modules MUST NEVER use raw SQL.** No `Statement::from_*`, `execute_raw`,
   `query_all_raw`, `query_one_raw`, `sea_query` string SQL, or any SQL text
   anywhere under `src/modules/**`, `src/ui/**` or `src/addons/**`.
   - Use **SeaORM 2.0** (entities + typed queries) exclusively.
   - If something cannot be expressed with the ORM, add a **typed helper** in a
     sanctioned place and call it from the module:
     - generic DB helpers → `src/db.rs` (e.g. `db::next_counter`),
     - feature-specific persistence → the module's own `store.rs`.
   - The ONLY place raw SQL is ever permitted is inside `src/db.rs`, and only
     when the ORM genuinely cannot do it. Even there, prefer the ORM first.
2. **UI is Components V2 only — never embeds.** Use `src/ui/*`
   (`Card`, `CardKind`, `CardOptions`, `CardReply::{reply,message}`,
   `make_*_card`, `make_list_card`, panels/containers/layout). Match the TS
   `lumi` cards closely.
3. **Modules are self-contained** under `src/modules/<name>/`:
   `mod.rs` (the `Module` impl), `commands.rs`, `events.rs`, `store.rs`,
   `views.rs`. Register the module in `src/client.rs`, aggregate its commands in
   `src/commands/mod.rs::all()`, and dispatch its events in `src/events/mod.rs`.
4. **Version comes from Cargo** — `env!("CARGO_PKG_VERSION")`. Never hardcode.
5. **Purpose-built crates over hand-rolled.** Use serenity/poise/sea-orm/apalis/
   governor/redis/moka helpers. If unsure whether a crate exists, check
   Context7/MCP before writing your own.
6. **Strict lints are on** (see `Cargo.toml` `[lints]`): `unused` + `dead_code`
   deny, `unwrap_used`/`expect_used` deny, clippy `correctness`/`suspicious`
   deny, `pedantic`/`nursery` warn. Do not add `#[allow]` for a denied lint.
   No `todo!()`/`unimplemented!()`; log with `tracing`, never `println!`.
7. **Never edit the reference repos** (`../lumi`, `../lumi-addons`, …).
8. **Storage is entity-first**: add an entity under `src/entities/` and declare
   it in `src/entities/mod.rs`; the baseline migration schema-syncs
   `lumi::entities::*`. Add composite indexes in a new migration when needed.

## Build & verify
```
nix develop "path:/home/rebiz/opt/lumi-rs" --command cargo clippy --all-targets
nix develop "path:/home/rebiz/opt/lumi-rs" --command cargo test --all
```
(`path:` prefix is required; the flake lives in a dirty git tree.) Aim for 0
clippy errors.

## Layout
- `src/core`-style runtime lives at the crate root: `config`, `error`,
  `services`, `db`, `entities`, `migrations`, `cache`, `scheduler`, `rpc`,
  `ratelimit`, `module`.
- `src/ui` — V2 presentation toolkit.
- `src/modules/<name>` — features (afk, logging, …).
- `src/addons` — Rhai host for runtime addons.
- `src/bot.rs` — shared `Data`/`Error`/`Context`/`Command` types.

Keep `todo.md` and `progress.md` current.
