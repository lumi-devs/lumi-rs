# Progress Log — lumi-rs

## Completed Milestones

### Workspace & Architecture Foundation
- 8-crate modular workspace (`lumi-db`, `lumi-kernel`, `lumi-ui`, `lumi-script`, `lumi-modules`, `lumi-api`, `lumi`, `xtask`).
- SeaORM 2.0 entity-first architecture covering all 41 Prisma models.
- Fast linking via mold + clang in `flake.nix`.
- Zero-raw-SQL rule across feature modules with atomic counter helpers in `lumi-db`.
- Process roles in `main.rs` cleanly decoupled (`all`, `gateway`, `worker`, `scheduler`, `api`).

### Command Framework & Single Source of Truth
- Dynamic command resolution via `ModuleRegistry`: commands are derived from registered modules, eliminating split definitions.
- Poise framework configured with:
  - Global slash command sync on gateway `ready`.
  - Dynamic per-guild prefixes querying `guilds.prefix` entity with fallback to `global_config.default_prefix`.
  - Mention-as-prefix support (`@bot command`).
  - Prefix command attributes wired across all active commands.
- Autocomplete handlers:
  - Dynamic moderation case reasons (`kick`, `ban`, `warn`, `unban`, `mute`).
  - Addon repository name autocomplete (`repo remove`, `download install`).
  - Command directory autocomplete (`help [command]`).

### Core Feature Ports
- **Moderation**: `/warn`, `/kick`, `/ban`, `/unban`, `/mute` (with duration parsing and Discord timeout), `/unmute`, `/case`, `/reason`, `/note`, `/softban`, `/lock`, `/lockdown`, `/quarantine`, `/dm`, `/say`, `/sanitize`.
- **TempVC**: Automatic channel creation upon joining a generator, automatic moving, and automatic channel deletion when empty via `VoiceStateUpdate`. Interactive channel control deck (`/tempvc panel`) and generator configuration (`/tempvc admin`).
- **Leveling**: Message XP calculation and persistence via `leveling::events::on_message`, `/rank`, and `/leaderboard`.
- **Economy**: `/balance`, `/daily`, `/pay`, `/bank deposit`, `/bank withdraw`.
- **Utility**: `/avatar`, `/whois`, `/serverinfo`, `/purge`.
- **Security**: Panic lockdown (`/panic on|off|status`) with permission denial restoration.
- **AFK**: `/afk`, `/afklist`, `/afkstats`, mention notifications and welcome-back cards.
- **API**: HTTP API server (`/healthz`, `/metrics`, `/action` with typed dispatchers).

---

## Active Parity Gaps Tracked (from Full TS Audit)

1. **Permissions & Preconditions**:
   - Enforce hierarchical `PermitResolver` in Poise pre-command hooks.
2. **Missing Moderation Flows**: None — `/softban`, `/quarantine`, `/lock`, `/lockdown`, `/dm`, `/say`, `/sanitize` implemented (still pending: `/vcmute`, `/cases delete`).
3. **Staff Management Commands**:
   - `/level add|remove|set` (admin XP adjustments).
   - `/bank set|add|remove` (admin currency adjustments).
4. **Utility & Personalization**:
   - `/banner`, `/nick`, specialized `/purge` subcommands (`user`, `bots`, `links`).
