# TODO.md — lumi-rs

## Architectural Foundation (Completed)
- [x] 8-crate Cargo workspace: `lumi-db`, `lumi-kernel`, `lumi-ui`, `lumi-script`, `lumi-modules`, `lumi-api`, `lumi-rest`, `lumi`
- [x] Zero raw SQL outside `crates/db/src/` rule enforced in AGENTS.md
- [x] SeaORM 2.0 schema initialization & automatic sync via `m0001_initial`
- [x] Replaced hand-parsing `/proc` telemetry with `sysinfo` in `PingData`
- [x] Layered configuration via `figment` (TOML + env vars)
- [x] Components V2 card engine ported in `lumi-ui` (zero legacy embeds)
- [x] PermitResolver ported with full tier chain evaluation
- [x] Full workspace clean check (`cargo check --workspace` passes with 0 warnings)

## Next: Feature Module Ports
- [ ] **Moderation Module** (`crates/modules/src/moderation/`)
  - [ ] Store: cases, appeals, notes, thresholds, counters (`db::next_counter`)
  - [ ] Commands: `/warn`, `/mute`, `/unmute`, `/kick`, `/ban`, `/unban`, `/case`, `/reason`, `/modnotes`
  - [ ] Views: Moderation V2 alert/confirm cards
  - [ ] Event listeners: Audit log synchronization, auto-timeout tracking
- [ ] **Utility Module** (`crates/modules/src/utility/`)
  - [ ] Avatar, serverinfo, userinfo, roleinfo, channelinfo
  - [ ] Reminders with `apalis` integration
  - [ ] Poll builder
- [ ] **Rhai Addon Host Hardening** (`crates/script/`)
  - [ ] SSRF blocklist on outbound requests (`127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, link-local)
  - [ ] Per-addon memory and instruction limits
  - [ ] Capability-scoped sandboxed DB/HTTP access
- [ ] **RPC / Dashboard API** (`crates/api/`)
  - [ ] REST endpoints for guild configurations and state management
- [ ] **Performance & Sharding Benchmarks**
  - [ ] Guild-to-shard mapping math verification
  - [ ] Memory footprint analysis under synthetic guild volume
