# TODO.md — lumi-rs

## Plan

- Short crate dir names: `db`, `ui`, `kernel`, `script`, `modules`, `api`, `rest`, `lumi` (bin).
- Create workspace (`Cargo.toml`) with `[workspace.members]` and `[workspace.dependencies]`.
- Move `src/db.rs` and `src/entities/` into `crates/db` (entities + migrations + helpers).
- Move kernel bits (config, error, cache, permits, scheduler, ratelimit, module trait) into `crates/kernel`.
- Keep V2 UI under `crates/ui` (cards/panels/layout).
- Move script (rhai host) into `crates/script`.
- Keep modules in `but` crate (as `modules` crate for relocating later).
- Add `api` crate (axum dashboard/rpc).
- Add `rest` crate (Discord REST proxy — phase 2).
- Rewrite `todo.md` and `AGENTS.md` for the 11M+ target, community focus, and cluster model.
- Add devtools: clang/mold, sccache, cargo-nextest, cargo-machete, cargo-deny.
- Run `cargo tree -d` and fix dep dupes.
- Prefer `crane` + `fenix` for the flake build, pin serenity/poise revs.
- Keep modules port only in the but crate for dev; rest split phase 2.

## Milestones

1. Workspace + crate split (db, kernel, ui).
2. Dumb monolith with `for_guild()` and minimal cache.
3. Timers per shard range.
4. Multi-process test (StatefulSet) with deterministic mapping.
5. Memory benchmarks (RSS per 1K guilds).
6. `api` crate: dashboard/RPC.
7. `rest` proxy (phase 2).
8. Only then: Postgres sharding (if measured limits force it).

## Early Wins

- No raw SQL in modules; db helper is in `db::next_counter`.
- V2 UI matches TS side; no embedded components.
- Strict lints + CI enforce `unused = warn` (deny locally).
- Shard→cluster math: `shard = (guild_id >> 22) % num_shards`.
- Reusable `Module` trait with `commands()` and `on_event()`.
- Dynamics: one repo, zero remote git deps (rev pins).