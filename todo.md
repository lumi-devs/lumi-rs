# Lumi-RS — Todo

Location: `/home/rebiz/opt/lumi-rs` (its own git repo).
**This is an entirely new bot — not a reimplementation or port of `lumi`.**
`lumi/` and `lumi-addons/` are read-only references; never write into them.

Goal: a new Rust Discord bot built to scale to millions of guilds and users,
**feature- and UI-matched to `lumi`** (almost the same UI, or better), with a
first-class, runtime-loaded addon system.

## Decisions

- [x] Framework: **poise** (git `serenity-next`) on **serenity** (git `next`) —
      the only revisions with Components V2 (`CreateComponent`,
      `MessageFlags::IS_COMPONENTS_V2`).
- [x] UI is Components V2 only — **no embeds, ever**. Port `lumi`'s
      `src/lib/ui/{Cards,Panels,Layout,Palette}.ts` faithfully to `src/ui/`
      (same builders, same strings, same colours), or better.
- [x] Storage: **SeaORM 2.0**, entity-first (entities are the schema source of
      truth; baseline migration syncs from the entity registry). Raw SQL is a
      documented escape hatch only (row locks, `pg_stat_*`, atomic counters).
- [x] Addons: **Rhai** (pure-Rust, deterministic sandbox, typed host API).
- [x] Process split: `gateway` · `worker` · `api` · `scheduler` · `all`.
- [x] Cache: `moka` + Redis + pub/sub invalidation. Queue: **apalis** +
      **apalis-redis** (Redis). Rate limiting: **tower-governor** (RPC) +
      **governor**.
- [x] Strict lints: deny `unused` + `dead_code`, `unwrap_used`/`expect_used`
      deny, clippy correctness/suspicious deny, pedantic+nursery warn. Purpose-
      built crates over hand-rolled; use serenity/poise/sea-orm helpers.
- [x] **Version comes from Cargo** (`env!("CARGO_PKG_VERSION")`) — no hardcoded
      module versions.
- [x] Modules are **self-contained** (`src/modules/<name>/`: own commands,
      events, store, views) like RedBot cogs / lumi modules.
- [x] Addon API exposure follows RedBot: one documented stable surface
      (`lumi::api` + Rhai namespaces), versioned like RedBot's `__all__`.

## Feature parity with lumi (port each module in `lumi/src/modules/*`)

Reference LOC in parentheses (size gauge for porting effort).

- [ ] `core` (1172) — hub, help, ping dashboard, settings, permits admin
- [ ] `onboarding` (3530) — incl. the **ping dashboard** + `PingCards` (see below)
- [ ] `mod` / moderation (3679)
- [ ] `security` (3111)
- [ ] `tempvc` (2998)
- [ ] `admin` (3529)
- [ ] `leveling` (2329)
- [ ] `economy` (2077)
- [ ] `reactionroles` (1669)
- [ ] `filter` (1657)
- [~] `afk` (1237) — **ported**: `/afk [reason]`, `/afklist`, `/afkstats`,
      mention-notify card (avatar thumbnail) + welcome-back card, `afk_entries`
      entity. Deferred: `afk` nickname prefix, `afkclean` (needs safe
      permission gating), mention-card buttons, scheduled message auto-delete.
- [ ] `utility` (1062)
- [ ] `logging` (796)
- [ ] `welcome` (671)
- [ ] `sticky` (257) — first vertical slice (smallest)
- [ ] `src/lib/*` (164 TS files): services, rpc, scheduler, module-system, ui

### UI parity details that must not be "dumbed down"

- **`ping` dashboard** (`onboarding/ui/PingCards.ts` + `commands/Ping.ts` +
  `services/PingCollect.ts`): the header is a `Section` with a **Thumbnail**
  accessory (bot avatar, `-# ⠀…` anchor). Six detail categories — `gateway`,
  `engine`, `host`, `postgres`, `redis`, `bot` — each built with
  `executiveSection(title, fields)` (`⠀__Title__:` + `⠀⠀**Label:** value`),
  a `Small` divider separator, and a **"Back to Overview"** button row.
  Overview card covers per-shard latency/uptime/RAM/CPU/servers/members +
  Redis/DB lines. Live-updates every 10s for 1 minute, ephemeral, no-ping.
- **Cards**: breadcrumbs + status badge in one header line joined by `·`;
  `## title`; `-# subtitle`; `divider` separator; sections; thumbnail via
  `Section`+`Thumbnail`; header images via `MediaGallery`; `-# footer`;
  optional separator above action rows. `fitLines` clamps to 4000 chars with
  `-# ...and N more item(s).`; list cards cap at 25 items, `• ` prefix, 200-char
  items. Empty cards use `📭` + default footer.
- **Panels** (`ui/Panels.ts`, 482 LOC): `settingRow`, `thumbRow`, `tabRow`,
  `confirmRow`, `backRow`, `navRow`, `createPaginationRow`, `buildSafeActionRows`
  (5x5 clamp), `componentEmoji`.
- Colours: exact `BrandColors` hex values; `primary` is blank by default.

## Phases

### 0. Scaffold — DONE
- [x] flake.nix (devShell + buildRustPackage), single-crate `Cargo.toml`,
      `.gitignore`, `git init`, strict lints, `src/{lib,main}.rs`.
- [x] Dependency graph compiles (serenity next + poise 0.7 git + sea-orm 2.0.4
      + rhai 1.26), `cargo clippy --all-targets` clean of errors.

### 1. Core runtime
- [x] `config`, `error`, `services`, `db` (connect+migrate), `entities`
      (guild, addon_kv), `migrations` (baseline schema-sync), Rhai `addon` host.
- [x] `ui::{cards,layout}` faithful text/colour port; `ping` command (V2 card).
- [ ] `ui::{palette(embedded), panels}`, `ui::container` primitives
      (add_text_display/section/separator/action_row/media_gallery).
- [ ] `permits` (resolver: owner → manager → enforced → custom tiers).
- [ ] `module` (Module trait, registry, config schema), `cache`, `scheduler`,
      `rpc` (axum, `/healthz`, `/metrics`), `metrics`.
- [ ] Addon host: full SDK namespaces (`commands`/`config`/`kv`/`ui`/`net`/
      `scheduling`/`discord`/`events`/`interactions`/`permissions`/`redis`/
      `utils`) + capability gating + invocation timeout/crash-loop guard.

### 2. Ping dashboard (immediate)
- [ ] `PingData` struct (mirror `PingCollect.ts` fields).
- [ ] `collect_ping_data`: shards/latency/guilds/users/channels from serenity
      cache + `ShardManager`; host stats via `/proc` + `sysinfo`; Redis `INFO`;
      Postgres `pg_stat_*` (the documented raw-SQL escape hatch); jitter, loop
      lag, tx rate, commands/s, messages/min.
- [ ] `ui::ping` overview + 6 detail cards + nav buttons; live-update task.

### 3. Modules (in order)
- [ ] sticky → afk → welcome → logging → utility → leveling → economy →
      reactionroles → filter → tempvc → security → mod → admin → onboarding
- [ ] each: commands, listeners, config schema, V2 cards/panels, tests.

### 4. Addons (Rhai)
- [ ] `custom/README.md` authoring guide + host API; example scripts per
      capability; addon unit-test harness.

### 5. Docs — `README.md`, `AGENTS.md`, `docs/{architecture,scaling,addons}.md`

### 6. Ops — `Dockerfile`, `docker-compose.yml`, `.dockerignore`, `.env.example`

### 7. Verification — `cargo fmt --check`, `cargo clippy --all-targets -D warnings`,
      `cargo test`, `nix build`

## Scale path (documented, not all built now)

- Shard count from `GET /gateway/bot`; one gateway process per shard range.
- Gateway → Redis Streams; workers consume with consumer groups (NATS/Kafka at
  the ceiling).
- Postgres: leading `guild_id` in every index; hash-partition append-only
  high-volume tables (economy tx, xp events, audit logs).

## Non-goals

- No TypeScript compatibility layer, no legacy shims, no 1:1 port of `lumi`
  internals.
- No embeds, no Components V1 replies.
