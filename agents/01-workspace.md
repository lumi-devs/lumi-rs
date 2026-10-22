# agents/01-workspace.md

## Layout

```
crates/
  db/       lumi-db      — entities, migrations, next_counter + other typed helpers
  kernel/   lumi-kernel  — config, error, cache, permits, scheduler, ratelimit, module trait
  ui/       lumi-ui      — V2 card/panel/layout builders (serenity-next types)
  script/   lumi-script  — Rhai addon host + capability gating
  modules/  lumi-modules — all feature modules (afk, logging, moderation, …)
  api/      lumi-api     — axum dashboard / RPC + guild→pod routing
  rest/     lumi-rest    — Discord REST proxy (phase 2, mostly stub)
  lumi/     lumi (bin)   — main entry: `lumi shard|api|rest|all`
  lints/    lumi-lints   — internal proc-macro lints (placeholder for now)
xtask/                   — codegen, release, db helpers
```

## Dependency direction

```
lumi (bin) → modules, api, rest, script, ui, db, kernel
modules    → db, kernel, ui, serenity, poise
api        → kernel
rest       → kernel
script     → (rhai only, no internal deps)
ui         → serenity
db         → sea-orm, sea-orm-migration
kernel     → tokio, dashmap, moka, arc-swap, governor, …
```

`ui`, `db`, `kernel` never depend on each other sideways (except kernel can use
db helpers if needed). Cyclic deps will fail to compile — keep the direction.

## Adding a new crate

1. `mkdir crates/<name>` + `crates/<name>/src/lib.rs`
2. Add a minimal `Cargo.toml` using `*.workspace = true` for all shared fields
   and `[lints] workspace = true`.
3. Add `"crates/<name>"` to `[workspace.members]` in the root `Cargo.toml`.
4. Add `lumi-<name> = { path = "crates/<name>" }` to `[workspace.dependencies]`.
5. Push and let Colab run `cargo check --workspace`.

## Version

All crates inherit `version.workspace = true` (currently `0.2.0`). Never set a
per-crate version; bump the workspace version only.
