# EVENT/LISTENER COVERAGE AUDIT — TS `lumi` vs Rust `lumi-rs`

Scope: 45 TS listeners under `src/modules/*/listeners/` compared against the single Rust dispatcher `crates/lumi/src/events/mod.rs` plus `crates/modules/src/{afk,leveling,logging,tempvc}/*`. Rust has **no** module event files for `security`, `welcome`, `sticky`, `reactionroles`, `mod`, `core`, or `filter` (filter lives inline in the dispatcher).

Rust modules that exist: `afk, economy, leveling, logging, moderation, tempvc, utility` (`crates/modules/src/lib.rs:1-15`). No `security`, `welcome`, `sticky`, `reactionroles`, `filter`, or `core` module crate exists.

## Dispatch reality in Rust

`crates/lumi/src/events/mod.rs:9-242` matches exactly these `FullEvent`s:

| Arm | line |
|---|---|
| `Ready` | 11 |
| `GuildCreate` | 21 |
| `InteractionCreate` | 24 |
| `Message` (create) | 58 |
| `MessageDelete` | 96 |
| `MessageUpdate` | 115 |
| `GuildMemberAddition` | 157 |
| `GuildMemberRemoval` | 165 |
| `GuildMemberUpdate` | 177 |
| `GuildBanAddition` | 194 |
| `GuildBanRemoval` | 211 |
| `VoiceStateUpdate` | 228 |
| `_ => {}` catch-all | 241 |

No `GuildAuditLogEntryCreate`, no role/channel delete, no `GuildUpdate`, no raw packet dispatch.

---

## Per-listener table

| module | TS listener | discord event | rs equivalent file:line | present? | behavioural parity notes |
|---|---|---|---|---|---|
| afk | `listeners/MessageCreate.ts` | `GuildUserMessage` (MessageCreate) | `events/mod.rs:84` → `afk/listeners.rs:8` | Partial | Clears AFK + replies to mentions. **Missing**: `isCommandMessage` skip (`:63`), nickname `[AFK]` strip (`:83-90`), mention batching/persistence, "view mentions" button/pagination, 20s/10min self-delete scheduling, per-channel cooldowns, reason sanitizing/duration formatting parity. |
| core | `listeners/Ready.ts` | `ClientReady` (once) | `events/mod.rs:11` | Partial | Only logs + registers global slash commands. **Missing**: `publishBotStats`, `reconcileGuilds`/`findDepartedGuildIds`, application fetch, downloader command sync. |
| core | `listeners/GuildCreate.ts` | `GuildCreate` | `events/mod.rs:21` | Partial (log only) | Only `tracing::debug`. **Missing**: `markGuildRejoined`, `ensureBuiltinPermits`, server-lock leave-and-notify (`shouldLeaveOnJoin`). |
| core | `listeners/bus/GuildDelete.ts` | `GuildDelete` | — | **No** | No handler at all: no `markGuildLeft`, no guild Redis eviction, no filter eviction. |
| core | `listeners/ChannelDelete.ts` | `ChannelDelete` | — | **No** | No stale-config cleanup (`clearStaleConfigRefs`). |
| core | `listeners/RoleDelete.ts` | `GuildRoleDelete` | — | **No** | No stale-config cleanup. |
| core | `listeners/errors/ClientError.ts` | `Error` | — | **No** | No `Error` event handling. |
| core | `listeners/routing/GuildUserMessage.ts` | `MessageCreate` | `events/mod.rs:58` (inline) | Partial | Rust checks only `!author.bot()`; TS also skips partial fetch failure, `webhookId !== null`, `system`, and `!inGuild()`. Rust forwards to afk+leveling only. |
| core | `listeners/routing/GuildUserMessageEdit.ts` | `MessageUpdate` | `events/mod.rs:115` (inline) | Partial | Same missing webhook/system gates before routing to filter+logging. |
| core | `listeners/shard/ShardDisconnect.ts` | `ShardDisconnect` | — | **No** | — |
| core | `listeners/shard/ShardError.ts` | `ShardError` | — | **No** | — |
| core | `listeners/shard/ShardReady.ts` | `ShardReady` | — | **No** | — |
| core | `listeners/shard/ShardReconnecting.ts` | `ShardReconnecting` | — | **No** | — |
| core | `listeners/shard/ShardResume.ts` | `ShardResume` | — | **No** | — |
| core | `listeners/ShardTelemetry.ts` | `ClientReady` | — | **No** | No shard-log publishing/tetelmetry. |
| core | `listeners/TelemetryStats.ts` | `ClientReady` | — | **No** | No REST 429/invalid-request metrics or shard gauges. |
| filter | `listeners/MessageCreate.ts` | `GuildUserMessage` | `events/mod.rs:58-95` (inline) | Partial | Rust only runs the static `ContentFilter` and deletes on verdict. **Missing entire system**: per-guild rules engine (`runRules`), `shouldScreen`, mention-flood auto-lockdown, heat scoring/escalation (timeout/quarantine/warn), duplicate/similarity, immunity checks, auto-lockdown scheduling, `guild-log` dispatch. |
| filter | `listeners/MessageUpdate.ts` | `GuildUserMessageEdit` | `events/mod.rs:115-156` (inline) | Partial | Same gaps as above (only static filter + delete). |
| leveling | `listeners/MessageCreate.ts` | `MessageCreate` | `events/mod.rs:90` → `leveling/events.rs:54` | Partial | Fixed `15` XP + default cooldown. **Missing**: module-enabled check, ignored channels/roles, min-message-length + attachment allowance, webhook/system/partial guards, role rewards, level-up announce. |
| leveling | `listeners/VoiceStateUpdate.ts` | `VoiceStateUpdate` | `events/mod.rs:235` → `leveling/events.rs:76` | Partial | Fixed `5` XP/min, `>=2` audible humans. **Missing**: module-enabled check, `config.voiceEnabled`, role rewards/announce. `is_audible` logic roughly matches. |
| leveling | `listeners/GuildMemberAdd.ts` | `GuildMemberAdd` | — | **No** | No `restoreEarnedRoles` on rejoin. |
| leveling | `listeners/GuildMemberRemove.ts` | `GuildMemberRemove` | — | **No** | No XP account deletion / `keepXpOnLeave`. |
| logging | `listeners/MessageDelete.ts` | `MessageDelete` | `events/mod.rs:96` → `logging/events.rs:82` | Partial | Emits ignored-channel gate + card. **Missing**: author line (Rust logs only channel + message ID), content chunk, attachment count, i18n. |
| logging | `listeners/MessageUpdate.ts` | `MessageUpdate` | `events/mod.rs:115` → `logging/events.rs:115` | Yes (near parity) | Diffs before/after, toggle, ignored channel, 450 cut, jump link. |
| logging | `listeners/MemberAdd.ts` | `GuildMemberAdd` | `events/mod.rs:157` → `logging/events.rs:163` | Partial | Rust logs member + account-created; **missing** member-count line. |
| logging | `listeners/MemberRemove.ts` | `GuildMemberRemove` | `events/mod.rs:165` → `logging/events.rs:188` | Partial | Rust logs member only; **missing** joined-at + member-count lines. |
| logging | `listeners/MemberUpdate.ts` | `GuildMemberUpdate` | `events/mod.rs:177` → `logging/events.rs:254` | Yes (near parity) | Nickname + role add/remove diffs, partial-guard mirrors TS. |
| logging | `listeners/BanAdd.ts` | `GuildBanAdd` | `events/mod.rs:194` → `logging/events.rs:210` | Partial | Rust omits ban **reason** (`ban.reason`) line. |
| logging | `listeners/BanRemove.ts` | `GuildBanRemove` | `events/mod.rs:211` → `logging/events.rs:232` | Yes | Equivalent card. |
| logging | `listeners/ClaimMessage.ts` | `GuildUserMessage` | — | **No** | No log-claim code handling/permit/audit. |
| mod | `listeners/QuarantineMemberAdd.ts` | `guildMemberAdd` | — | **No** | No re-enforce quarantine role on rejoin. |
| mod | `listeners/VoiceStateUpdate.ts` | `VoiceStateUpdate` | — | **No** | No voice-mute disconnect enforcement. |
| reactionroles | `listeners/RawReactions.ts` | `raw` (`MESSAGE_REACTION_ADD/REMOVE`) | — | **No** | No raw gateway handling, no reaction toggle/roles. |
| reactionroles | `listeners/Ready.ts` | `ClientReady` | — | **No** | No registry wiring. |
| security | `listeners/AuditLogEntryCreate.ts` | `GuildAuditLogEntryCreate` | — | **No** | `SecurityManager` + `DestructiveActionKind` exist (`kernel/security.rs`) but `record_destructive_action` is **never called** — only `get_panic_status`/`activate_panic` are used (security slash commands, `crates/lumi/src/commands/security.rs:24,61`; api). Anti-nuke never fires. |
| security | `listeners/GuildMemberUpdate.ts` | `GuildMemberUpdate` | — | **No** | No quarantine-role revert / `quarantine_bypass` nuke eval. |
| security | `listeners/GuildUpdate.ts` | `GuildUpdate` | — | **No** | No vanity-change nuke eval. |
| security | `listeners/MemberJoin.ts` | `GuildMemberAdd` | — | **No** | No verification pending role, join-gate filters, raid mode. |
| security | `listeners/RoleUpdate.ts` | `GuildRoleUpdate` | — | **No** | No @everyone dangerous-permission revert. |
| sticky | `listeners/MessageCreate.ts` | `GuildUserMessage` | — | **No** | No sticky repost/delete. |
| tempvc | `listeners/VoiceStateUpdate.ts` | `VoiceStateUpdate` | `events/mod.rs:228` → `tempvc/events.rs:11` | Partial | Create-on-generator + delete-empty-managed works. **Missing**: module-enabled check, per-user create cooldown (TS `onCreateCooldown`), owner permission setup, delayed `scheduleCleanup`, cached occupancy tracking, bot-member guard (Rust doesn't skip bots — a bot joining a generator would create a VC), channel fetch fallback. |
| tempvc | `listeners/Ready.ts` | `ClientReady` (once) | — | **No** | No `reconcileGuild` on startup; stale records/channels not cleaned. |
| tempvc | `listeners/Raw.ts` | `raw` (`GUILD_CREATE`) | — | **No** | No voice-state seeding / occupancy clear on reconnect. |
| welcome | `listeners/GuildMemberAdd.ts` | `GuildMemberAdd` | — | **No** | No welcome channel card, autoroles, DM welcome. |
| welcome | `listeners/GuildMemberRemove.ts` | `GuildMemberRemove` | — | **No** | No goodbye card. |

## 1. Missing event handlers in Rust, grouped by module

- **security** — all 5 listeners missing. No `GuildAuditLogEntryCreate` (`AuditLogEntryCreate.ts`), no security-side `GuildMemberUpdate`, no `GuildUpdate`, no security `GuildMemberAdd` (`MemberJoin`), no `GuildRoleUpdate`. `SecurityManager`/anti-nuke is dead code from the event path (only panic state used by commands/API). Note `ModerationModule` (`crates/modules/src/moderation/mod.rs`) has **no `events.rs`** either.
- **welcome** — both `GuildMemberAdd` and `GuildMemberRemove` unhandled.
- **sticky** — `GuildUserMessage` repost unhandled.
- **reactionroles** — no raw `MESSAGE_REACTION_ADD/REMOVE`, no `ClientReady` registry wiring.
- **filter** — `MessageUpdate` *is* handled inline (`events/mod.rs:115`), but only the static filter; the TS heat/rule/lockdown engine is absent (see parity notes).
- **logging** — `ClaimMessage` (`GuildUserMessage`) missing. All other logging events present but partly lossy.
- **leveling** — `GuildMemberAdd` (role restore) and `GuildMemberRemove` (account delete) missing.
- **tempvc** — `Ready` reconciliation and raw `GUILD_CREATE` reconciliation missing.
- **core** — all of `ChannelDelete`, `RoleDelete`, `GuildDelete`, shard events, telemetry missing. `Ready` and `GuildCreate` exist but stub-level.
- **mod** — `guildMemberAdd` quarantine re-enforce and `VoiceStateUpdate` voice-mute missing.

## 2. Functional gaps in handlers that DO exist

- **logging**: `on_message_delete` lacks author/content/attachments (`logging/events.rs:82`); `on_member_add` omits member count (`:163`); `on_member_remove` omits joined-at + member count (`:188`); `on_ban_add` omits reason (`:210`). Only message/member/ban event types are implemented — no channel/role delete logging, no claim logging.
- **tempvc**: no startup reconciliation (`tempvc/events.rs` has only `on_voice_state_update`; `store.rs:156 list_records` is unused by any Ready hook); deletes empty channels immediately with no delay/cooldown; no create cooldown; no bot-member guard; no module-enabled check.
- **afk**: only reacts to messages (`afk/listeners.rs:8`); no nickname prefix management, no mention persistence.
- **leveling**: message/voice XP only, fixed constants (`leveling/events.rs:12-13`); no config, no rewards.
- **filter**: inline only static regex/wordlist (`events/mod.rs:63`, `kernel/filter.rs:63`); no heat, lockdown, rules, logging.
- **tempvc Ready / all raw listeners**: serenity `FullEvent` exposes no raw packet variant, so raw-packet listeners (reactionroles, tempvc GUILD_CREATE) would need a dedicated raw event handler — not present.

## 3. Gateway intents

`crates/lumi/src/client.rs:126-128`:

```rust
let intents = GatewayIntents::non_privileged()
    | GatewayIntents::MESSAGE_CONTENT
    | GatewayIntents::GUILD_MEMBERS;
```

`non_privileged()` = all intents except `GUILD_PRESENCES | GUILD_MEMBERS | MESSAGE_CONTENT`. Therefore the bot **already subscribes to** the intents needed for every event in the table:

- `GUILD_MODERATION` (in non-privileged) → bans/unbans and `GuildAuditLogEntryCreate` ✓
- `GUILDS` → `GuildCreate`, `GuildDelete`, `ChannelDelete`, `GuildRoleDelete/Update`, `GuildUpdate` ✓
- `GUILD_MESSAGES` + `MESSAGE_CONTENT` → `Message`, `MessageUpdate`, `MessageDelete` ✓
- `GUILD_MEMBERS` (explicitly added) → `GuildMemberAdd/Remove/Update` ✓
- `GUILD_VOICE_STATES` → `VoiceStateUpdate` ✓
- `GUILD_MESSAGE_REACTIONS` → reaction add/remove (for a raw handler) ✓

**Conclusion:** missing behavior is a **dispatch gap, not an intent gap**. No handler is blocked by a missing intent. The only intent caveat is that serde-level audit-log *reasons* (used by logging bans) require an extra `fetch_audit_logs` REST call, which neither side fully does; TS avoids needing it because discord.js provides reasons via audit-log gateway entries only for some events, and Rust omits the reason line entirely.

---

*Skipped: I did not read the TS shard listener bodies (only their event names) or the Rust `api`/scheduler-tier event paths beyond confirming no extra dispatch; `economy`/`utility`/`moderation` modules are command-only and have no TS listener counterparts. Risk: exact serenity `non_privileged()` bit set was inferred from the crate's documented privileged set, not runtime-verified.*
