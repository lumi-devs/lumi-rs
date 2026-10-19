# Lumi-RS — Comprehensive Parity & Architecture Roadmap

Location: `/home/rebiz/opt/lumi-rs` (its own git repo).
**This is an entirely new bot — not a reimplementation or port of `lumi`.**
`lumi/` and `lumi-addons/` are read-only references; never write into them.

Goal: A production-ready, highly-scalable Rust Discord bot built with Serenity Next, Poise, and SeaORM 2.0, **strictly matching or exceeding the reference TS bot (`lumi`) in commands, event handling, permissions, and UI quality**.

---

## 📊 Comprehensive TS vs Rust Parity Matrix

### 1. Commands & Subcommands
| Module | Reference TS Commands | Current Rust Commands | Priority Gaps |
| :--- | :--- | :--- | :--- |
| **afk** | `/afk`, `/afkstats`, `/afklist`, `/afkclean` | `/afk`, `/afklist`, `/afkstats` | Missing `/afkclean`, guild-owner permission gating on stats/list |
| **logging** | None (Event-driven) | None (Event-driven) | Complete |
| **moderation** | `/ban`, `/cases`, `/dm`, `/kick`, `/lock`, `/lockdown`, `/notes`, `/quarantine`, `/sanitize`, `/say`, `/softban`, `/timeout`, `/vcmute`, `/warn`, context menu | `/warn`, `/kick`, `/ban`, `/unban`, `/mute`, `/unmute`, `/case`, `/reason`, `/note`, `/softban`, `/lock`, `/lockdown`, `/quarantine`, `/dm`, `/say`, `/sanitize` | Missing `/vcmute`, `/cases delete` |
| **utility** | `/avatar`, `/banner`, `/nick`, `/purge` (subcommands: messages, user, bots, links, regex, duration), `/serverinfo`, `/whois` | `/avatar`, `/whois`, `/serverinfo`, `/purge` (flat amount) | Missing `/banner`, `/nick`, specialized `/purge` subcommands |
| **tempvc** | `/tempvc panel`, `/tempvc admin` (interactive UI deck) | `/tempvc panel`, `/tempvc admin setup\|delete\|list` | Core features matched; admin uses typed subcommands |
| **economy** | `/balance`, `/bank` (deposit, withdraw, set, add, remove), `/leaderboard`, `/pay`, `/payday`, `/slots` | `/balance`, `/daily`, `/pay`, `/bank deposit\|withdraw` | Missing `/slots`, admin `/bank` adjustments (`set`/`add`/`remove`) |
| **leveling** | `/rank`, `/leaderboard` (interactive views), `/level` (add, remove, set) | `/rank`, `/leaderboard` (top 10 text card) | Missing admin `/level add|remove|set` |
| **security** | `/panic` (direct confirmation), `/restore list\|latest` | `/panic on\|off\|status` | Missing `/restore` snapshot recovery |
| **admin / dl** | `/access`, `/announce`, `/download`, `/module`, `/repo`, `/serverlock`, `/sv` | `/repo add\|list\|remove`, `/download install\|list` | Missing `/access`, `/module` management, `/serverlock` |
| **core** | `/about`, `/help`, `/lumi panel`, `/mydata`, `/ping`, `/setup` | `/about`, `/help`, `/lumi`, `/ping` | Missing `/mydata` (GDPR exports), `/setup` wizard |

---

### 2. Gateway Event Listeners
| Gateway Event | Reference TS Handlers | Current Rust Handlers | Status |
| :--- | :--- | :--- | :--- |
| **`Ready`** | Global slash command deploy, TempVC orphaned channel cleanup | Global slash command sync (`poise::builtins::register_globally`) | Partial (needs startup TempVC reconciliation) |
| **`Message`** | Content filter, heat scores/auto-timeouts, AFK notifications, Leveling XP award | Content filter deletion, AFK clear/notify, Leveling XP award | Functional |
| **`MessageDelete`** | Logging message diff & attachment count | `logging::events::on_message_delete` | Complete |
| **`MessageUpdate`** | Content filter screening, Logging content diffs | Content filter screening, `logging::events::on_message_update` | Complete |
| **`GuildMemberAddition`**| Logging join, Security anti-raid/join-gate, Mod quarantine re-apply, Leveling role restore | `logging::events::on_member_add` | Partial |
| **`GuildMemberRemoval`** | Logging leave, Leveling progress purge (if configured) | `logging::events::on_member_remove` | Complete |
| **`GuildMemberUpdate`**  | Logging nicknames/roles, Security quarantine bypass protection | `logging::events::on_member_update` | Complete |
| **`GuildBanAddition`**   | Logging member ban & reason | `logging::events::on_ban_add` | Complete |
| **`GuildBanRemoval`**    | Logging member unban | `logging::events::on_ban_remove` | Complete |
| **`VoiceStateUpdate`**   | TempVC auto-generator & empty cleanup, Mod voice-mute enforce, Leveling voice XP | `tempvc::events::on_voice_state_update` | Functional |
| **`GuildAuditLogEntryCreate`** | Security anti-nuke triggers (kicks, bans, channel/role deletes) | None | Missing |
| **`GuildRoleUpdate`**    | Security revert dangerous permissions on `@everyone` | None | Missing |

---

### 3. Architecture & Core Services
- [x] **Process Roles (`main.rs`)**: Isolated loops for `gateway`, `worker`, `scheduler`, `api`, and concurrent supervisor for `all`.
- [x] **Command Source of Truth**: Dynamically aggregated from `ModuleRegistry` (`commands::all_from_registry`).
- [x] **V2 Components**: Strict adherence to Components V2 (no embeds).
- [ ] **Permits & Access Control**: Wire `PermitResolver::resolve` into Poise pre-command hooks or command checks; add guild scoping to in-memory permit indices.
- [ ] **Distributed Cache**: Centralize Redis key catalog matching TS `RedisKeys` and bind cache invalidation subscribers across modules.
- [ ] **Rhai Addons**: Expose versioned bot API (storage, Discord actions, events) to `lumi-script` sandbox.

---

## 🎯 Active Implementation Plan

1. **Permissions Precondition Hook**:
   - Add a `pre_command` hook in `poise::FrameworkOptions` that evaluates `ctx.data().permits.resolve(...)` against required permit nodes before command execution.
2. **Missing Moderation Commands**:
   - [x] Implement `/softban` (ban + unban with message purge).
   - [x] Implement `/quarantine add|remove` with role persistence.
   - [x] Implement `/lock`, `/lockdown`, `/dm`, `/say`, `/sanitize`.
3. **Missing Leveling & Economy Staff Commands**:
   - Implement `/level add|remove|set` in `crates/modules/src/leveling/commands.rs`.
   - Implement `/bank set|add|remove` in `crates/modules/src/economy/commands.rs`.
4. **Utility Extensions**:
   - Implement `/nick` and `/banner`.
   - Expand `/purge` with subcommands (`user`, `bots`, `links`).
