# Progress Log — lumi-rs

## Completed Milestones

### Workspace & Monolith Elimination
- Refactored monolithic `src/` directory into 8 specialized workspace crates.
- Defined explicit dependency paths to guarantee fast incremental compile loops and isolate heavy dependencies.

### Database Architecture & Parity
- Defined all 41 Prisma schema models as native SeaORM 2.0 entities inside `crates/db/src/entities/`.
- Schema baseline migration (`m0001_initial`) connects to PostgreSQL and registers the schema directly from the entity registry.
- Guaranteed zero raw SQL rule across modules; atomic monotonic counter (`next_counter`) encapsulated inside `lumi-db`.

### Observability & Runtime
- Replaced custom string parsing of Linux `/proc` files with the `sysinfo` library.
- Refactored ping diagnostics to reflect the native Rust engine, Serenity Next, and SeaORM stack instead of legacy D.JS/Prisma strings.
- Enabled `mimalloc` global allocator for optimal long-running server memory characteristics.
- Local compilation verified using Nix dev-shell (`cargo check --workspace` at 0 errors, 0 warnings).

## Active Focus
- Porting core feature modules from TS reference to `lumi-modules`:
  1. `moderation`
  2. `utility`
  3. `script` (Addon engine enhancements)
