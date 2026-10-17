# Progress log

Newest first. One entry per meaningful unit of work: what changed, what was
verified, what is next. Keep it short.

## 2026-10-17 (afk + cleanup)

- **AFK module ported** (self-contained `src/modules/afk/`): `mod.rs`
  (`AfkModule` + config schema, version from `env!("CARGO_PKG_VERSION")`),
  `data.rs` (set/get/clear/list/count over `afk_entry`), `cards.rs`
  (`mention_card` with avatar **thumbnail** + `welcome_back_card`),
  `listeners.rs` (`on_message` clears author AFK → welcome-back; notifies for
  mentioned AFK users), `commands.rs` (`/afk`, `/afklist`, `/afkstats`).
  `src/entities/afk_entry.rs` (composite PK `(user_id, guild_id)`).
- **Wiring**: `src/bot.rs` holds `Data/Error/Context/Command` so modules own
  their commands; `client.rs` re-exports `lumi::bot::{Context,Data,Error}`;
  `commands::all()` aggregates `modules::afk::commands()`; `events` dispatches
  `FullEvent::Message` → `afk::on_message`. Deferred: nick prefix, `afkclean`,
  mention buttons, message auto-delete.
- **Cruft removed**: empty scaffold dirs `migrations/`, `custom/`, `docs/`
  (migrations live in `src/migrations/`); addon dir default → `addons/`.
- **Verified**: `cargo clippy --all-targets` exit 0; `cargo test --lib` 12 ok.
- **Memory**: stored the RedBot addon-API exposure model (its `redbot.core`
  public surface + versioning guarantee) as the reference for our addon API.
- Next: continue modules (welcome → logging → utility → leveling → …), then the
  `lumi::api` surface + Rhai namespaces, and clean the ~52 pedantic warnings.

## 2026-10-17 (later)

- **Collapsed to ONE crate.** `lumi-core` + `lumi` were pointless: with Rhai
  addons, nothing needs a separate SDK crate. Now a single `lumi` package
  (`src/lib.rs` + `src/main.rs`), matching the structure of real poise bots
  (`getchoo/moyai-bot`, `ryanccn/valfisk`) that were cloned as references.
- **Layout**: `src/{main,client,commands,events}` (bin) + `src/{config,error,
  services,db,entities,migrations,addon,ui}` (lib). `Data = Arc<Services>`.
- **Verified**: full dependency graph compiles and unifies — serenity `next`
  + poise `serenity-next` (0.7.0) + sea-orm/-migration 2.0.4 + rhai 1.26;
  `cargo clippy --all-targets` has 0 errors. API corrections learned:
  poise `Framework::new(options)` (no `.setup()`), `Token` via `FromStr`,
  `FullEvent` is non-exhaustive (`..`), shutdown via
  `ShardManager::get_shutdown_trigger()`, `CreateReply` must be `<'static>`.
- **Strict lints** in `[lints]`: deny `unused` + `dead_code`, clippy
  correctness/suspicious deny, pedantic+nursery warn; per-lint allows for
  style noise (`use_self`, `return_self_not_must_use`, `multiple_crate_versions`).
- **UI port started** from `lumi/src/lib/ui/*`: `ui/layout.rs`
  (`format_status_badge`/`format_subtitle`/`format_breadcrumbs`/
  `format_breadcrumb_header`/`format_page_footer`) and `ui/cards.rs`
  (`Card`/`CardKind`/`CardOptions`/`CardReply`/`make_*_card`/`make_list_card`/
  `make_empty_card`/`fit_lines`) with unit tests.
- **Next / in progress**: the `ping` dashboard must match
  `onboarding/ui/PingCards.ts` exactly (thumbnail header section, 6 detail
  categories, `executiveSection`, nav buttons, live updates). Then the
  remaining modules in `todo.md`.
- **Ping dashboard ported** (`src/ui/{container,panels,ping}.rs`, `src/ping.rs`,
  `src/commands/general/ping.rs`, `src/events/mod.rs`): thumbnail header
  `Section`, 6 detail categories, `executive_section`, `fmt_*` helpers, the
  `Small` divider + "Back to Overview" button. Component ids
  `ping:{category}:{user_id}`, handled type-7 in-place from
  `FullEvent::InteractionCreate`; `/ping` defers ephemeral then edits with the
  V2 overview. Real metrics: shard latency (`ctx.runner_info`), guilds/users/
  channels, bot identity, host `/proc`+`/sys` stats. One `shortcut:` left for
  Redis/Postgres probes (`src/ping.rs`). `cargo clippy --all-targets` exits 0.
- **Still to do**: live-update loop, prefix command variant, the 15 feature
  modules, and the rest of `ui/panels.rs`.
- **Core runtime foundation** (`module.rs`, `permits.rs`, `cache.rs`,
  `scheduler.rs`, `rpc.rs`, `ratelimit.rs`, `entities/{permits,
  permit_assignments}.rs`): Module trait+registry+config schema; permit
  resolver matching `PermitResolver.ts`; moka+Redis cache with pub/sub
  invalidation; prometheus `/metrics` + `/healthz`; apalis worker stub.
- **Lints/profiles tightened** (per request): `unsafe_code=forbid`,
  `unwrap_used`/`expect_used=deny`, `todo=warn`, pedantic+nursery warn,
  correctness/suspicious deny; `[profile.dev.package."*"] opt-level=2`;
  release `opt-level=3, lto="thin", strip=true`. All `.unwrap()/.expect(`
  removed from our code.
- **Purpose-built libs over hand-rolled**: swapped the hand-rolled Redis
  ZSET+lease scheduler for **apalis** + **apalis-redis** (1.0); added
  **tower-governor** (axum layer) + **governor** (`ratelimit::RateLimiter`).
  (`redis` 0.28 vs apalis-redis's `redis` 1.7 coexist.)

## 2026-10-17

- **Direction corrected (important).** The bot is a **new bot**, not a
  reimplementation/port of `lumi`, and lives at `/home/rebiz/opt/lumi-rs`
  (own git repo). `lumi/` + `lumi-addons/` are read-only references.
  The accidental `lumi/rust/` scaffold (untracked) was moved out to `lumi-rs/`.
- **ORM decided: SeaORM 2.0, entity-first.** Checked both repos: `lumi/` has
  exactly 9 raw-SQL spots (`$queryRaw`/`$executeRaw` — `FOR UPDATE` lock,
  `pg_stat_*` health, atomic case-counter upsert, ranked leaderboard) and
  `lumi-addons/` has 0. So: ORM everywhere, raw SQL as a documented escape
  hatch for those four cases only. SeaORM 2.0 ships an official `CLAUDE.md`
  agent guide; 2.0 = `#[sea_orm::model]` + relation fields, typed `COLUMN`,
  `ActiveModel::builder()`, entity-registry schema-sync, IDENTITY (not serial).
- **Embedded addon language decided: Rhai.** Pure-Rust (clean nix/musl static
  builds), typed host bindings, deterministic sandbox limits matching lumi's
  `addon-sdk.md` model. Rejected Rune (immature), Dyon (niche), Lua (embeds C).
- **Addon SDK surface preserved.** Rhai host modules mirror lumi's addon names
  (`commands`/`config`/`kv`/`ui`/`net`/`scheduling`/`discord`/`events`/
  `interactions`/`permissions`/`redis`/`utils`) with the same default-deny
  capability model, so `lumi-addons` concepts carry over.
- **RedBot studied as the addon-API reference.** Cloned
  `Cog-Creators/Red-DiscordBot` and read its local docs (`guide_cog_creation`,
  `version_guarantees`, `framework_bank`, `framework_commands`,
  `cog_permissions`). Borrowed ideas: public-API versioning guarantee, scoped
  `Config` store, `Bank` currency API, permissions rule-priority resolution.
- **Scaffold** (moved to `lumi-rs/`, `git init` done): `flake.nix`, workspace
  `Cargo.toml` (serenity+poise git, **sea-orm 2.0** + sea-orm-migration, rhai),
  `crates/lumi-core` + `crates/lumi` manifests, `.gitignore`, `todo.md`,
  `progress.md`.
- Next: dependency spike (poise+serenity git unify + sea-orm 2.0 compile),
  then `lumi-core` (ui, config, db/entities, addon host).
