# Lumi TS → Rust Feature-Surface Audit (non-command)

Read-only comparison of the non-command surface. Rust module registry currently contains **7 modules** (`/home/rebiz/opt/lumi-rs/crates/modules/src/lib.rs:1-15`): afk, economy, leveling, logging, moderation, tempvc, utility. TS has **15 modules** (`/home/rebiz/opt/lumi/src/modules/`).

---

## 0. Summary

| Area | RUST | Notes |
|---|---|---|
| admin | Partial | downloader lib + hub addons only; no `/module`,`/shutdown`,`/announce`,`/serverlock`,`/sv`, config panel, overrides |
| core | Partial | only Ready + GuildCreate logging; no cleanup/retention/GDPR/shard telemetry |
| filter | Partial (kernel only) | static `ContentFilter`; no module/config/regex/heat/lockdown/RPC |
| onboarding | Partial | hub `lumi`/`panel` + ping/about; no help/mydata/setup wizard |
| reactionroles | **Missing** | no module at all |
| security | Partial | anti-nuke model + panic **not wired**; no backup/joingate/verification |
| sticky | **Missing** | — |
| welcome | **Missing** | — |
| afk / economy / leveling / logging / moderation / tempvc / utility | Present, incomplete | see §2 |
| permits | Partial | read/resolve/hub assign/order; **no create/update, no live refresh, quarantine hardcoded off** |
| RPC / API | Minimal | 4 actions vs ~91 |
| scheduler | Library only, **not running** | stub roles |
| cache / redis / ratelimit | Partial | no locks/cooldowns/eviction/event bus |
| Rhai addons | Minimal | log-only host, no platform bindings |

---

## 1. TS modules with NO Rust module

### 1.1 `admin`
Capabilities (`src/modules/admin/`, `Index.ts:3-13`): module manager and server admin tools.
- Commands: `Access`, `Announce`, `Download`, `Module`, `Repo`, `Serverlock`, `Shutdown`, `Sv` (`admin/commands/*`).
- Services: `DownloaderAutocomplete`, `Operations` (shutdown/announce/serverlock), `Pieces`, `Registry` (`admin/services/*`).
- Task: `AddonAutoUpdate` (`admin/tasks/AddonAutoUpdate.ts`); UI: `ModuleCommandCards`, `ModuleUpdateCard`, `Overrides`; Utilities: `ConfigUtility`, `GuildLogUtility`.

Exists elsewhere in Rust? **Partially.** `crates/kernel/src/downloader.rs` implements repo add/pull/update, install/uninstall/toggle/auto-update-config (`downloader.rs:75,189,289,383,481,537,564,670`). The hub has an addons tab (`crates/lumi/src/commands/hub/addons.rs`). **Missing:** `/module`, `/shutdown`, `/announce`, global `/serverlock`, `/sv`; per-command access config; config panel + overrides; guild log utility; admin `Operations` service.

### 1.2 `core`
Capabilities (`core/Index.ts:10-33` and files):
- Lifecycle listeners: `GuildCreate`, `GuildDelete` (bus), `ChannelDelete`, `RoleDelete`, `Ready`, `ClientError`; shard listeners (`ShardDisconnect/Error/Ready/Reconnecting/Resume`), `ShardTelemetry`, `TelemetryStats`; message routing `GuildUserMessage`/`GuildUserMessageEdit`.
- Services: `ConfigCleanup`, `DataRetention`, `GdprExportTask`; Tasks: `DataRetention`, `GdprExport`, `GdprExportCleanup`, `SendMessage`; `Rpc.ts` (core + downloader RPC handlers).

Exists elsewhere in Rust? **Partially.** `Ready` (slash sync) and `GuildCreate` (log only) are handled in `crates/lumi/src/events/mod.rs:11-23`. DB has `gdpr_export_job` entity but no job. **Missing:** guild/channel/role delete config cleanup, shard telemetry, telemetry stats, data retention, GDPR export/cleanup jobs, outbound send queue (`@lib/outbound`), guild-user message routing.

### 1.3 `filter`
Capabilities (`filter/Index.ts:58-397`): per-guild automod rule engine — filtered terms (Aho-Corasick), regex rules via `regex-worker`, invite/link blocking with allowlists, mention/caps limits, per-rule punishment timeouts, templated warn message, log channel, full **heat scoring** system, **auto-lockdown**, `filter` utility, RPC (`guild.filter.status`, `guild.filter.lockdown.release`), `filter-auto-lockdown-unlock` task.

Exists elsewhere? **Partially.** `crates/kernel/src/filter.rs` is a static `ContentFilter` (terms/invite/phishing/mass-mention) applied inline in `events/mod.rs:58-95` and `:115-156`. **Missing everything configurable:** per-guild terms/regex/link allowlists/caps, punishment timeouts, warn template, log channel, heat engine, auto-lockdown, RPC, module registration, permit.

### 1.4 `onboarding`
Capabilities (`onboarding/`): commands `About`, `Help`, `Lumi`, `Mydata`, `Ping`, `Setup`; interactions `HubPanel` (button/modal/select), `SetupWizard` (button/modal/select), `Ping`; services `HubPanel`, `PingCollect`, `SetupWizard`; ui cards.

Exists elsewhere? **Partially.** Hub `/lumi` + `/lumi panel` (`commands/hub/mod.rs:118-139`), `ping` (`commands/general/ping.rs`), `about` (`commands/about.rs`). **Missing:** `/help`, `/mydata` (self-service GDPR), setup wizard, ping collector, hub-onboarding copy.

### 1.5 `reactionroles`
Capabilities (`reactionroles/Index.ts:5-25`): button/dropdown/raw-reaction role menus, exclusive + gated options, `max_menus` config, repository/registry, listeners `RawReactions`/`Ready`, buttons/selects, `RoleToggle`, `MenuCard`, RPC (`menus.list/set/delete`), permit nodes `reactionroles.*`.

Exists elsewhere? **Entirely missing** — no Rust module, no DB store, no RPC, no permit node.

### 1.6 `security`
Capabilities (`security/Index.ts:19-425`):
- **Panic mode** (`panic_lock_mod_commands`, channels to lock) + `Panic`, `Restore` commands.
- **Anti-nuke**: window, trusted roles, log channel, per-action limits + responses (`log`/`quarantine`/`ban`), dangerous-permission-grant and quarantine-bypass reverts.
- **Join gate**: min account age, raid detection (count/window/action/scope), filters no-avatar / min-age / unverified-bot / username-pattern / advertising.
- **Verification**: emoji/none/web modes, verified + pending roles, timeout, kick-on-timeout, target everyone/suspicious.
- **Backups** (interval/keep) + `RestoreGuild`; services `AntiNuke`, `Audit`, `Backup`, `Captcha`, `JoinGate`, `JoinHeuristics`, `Verification`, `VerifySweepHandler`; listeners `AuditLogEntryCreate`, `GuildMemberUpdate`, `GuildUpdate`, `MemberJoin`, `RoleUpdate`; tasks `BackupSnapshot`, `VerifySweep`; RPC.

Exists elsewhere? **Partially (largely inert).** `crates/kernel/src/security.rs` defines `DestructiveActionKind`, `AntiNukeConfig`, `record_destructive_action`, and panic persistence (`security.rs:17-331`); panic commands exist (`crates/lumi/src/commands/security.rs`); API has `security.panic.status` (`crates/api/src/lib.rs:148`). **Critical gap:** `record_destructive_action` has no caller — there is no audit-log event in `events/mod.rs`, so **anti-nuke is never enforced**. Panic sets `invites_paused` in DB but never pauses invites. **Missing:** backups/restore, join gate + heuristics, verification/captcha/panel, audit service, threshold response execution, trusted roles/log channel config, `antinuke_alert` case type, all security RPC.

### 1.7 `sticky`
Capabilities (`sticky/Index.ts:14-61`): re-post a configured message to channel bottom on every message; rich entries (images/gallery, thumbnail, accent color, V2 blocks); `StickyStore`/`StickyIndex`; listener `MessageCreate`.

Exists elsewhere? **Entirely missing** in Rust.

### 1.8 `welcome`
Capabilities (`welcome/Index.ts:18-123`): welcome + goodbye cards, rich V2 layout, auto roles, DM greeting; listeners `GuildMemberAdd`/`GuildMemberRemove`; service `Welcome`; RPC `guild.welcome.sendTest`.

Exists elsewhere? **Entirely missing** in Rust.

---

## 2. Modules present in both — TS capabilities missing in Rust

### 2.1 afk
Rust: `afk`, `afklist`, `afkstats`, mention notice, welcome-back, nick prefix, data CRUD (`modules/src/afk/*`).
**Missing:** `afkclean` command, the `afk-delete-message` task/`DeleteHandler` (delete-then-notify flow), mention button interaction (`interactions/buttons/Mentions.ts`), `guild.afk.list` RPC, GDPR export/delete hook.

### 2.2 economy
Rust: `balance`, `daily` (alias payday), `pay`, `bank` + `deposit/withdraw/set/add/remove`, `slots` (`modules/src/economy/commands.rs`).
**Missing:** `leaderboard` command (`economy/commands/Leaderboard.ts`); **entire config surface** — currency name/emoji, starting wallet/bank, max balance, payday amount/cooldown, transfer tax, slot min/max bid, cooldown, daily cap, payout table (`Index.ts:22-122`) — Rust `config_schema` is empty (`economy/mod.rs:27-29`) and `transfer` applies **no tax** / no max-balance cap / no slot config (`store.rs:264`); `InsufficientFundsError` unified handling; RPC `guild.economy.balance/leaderboard/adjust`; GDPR/audit hooks. (Note: no "shops/items" exist in the TS economy either.)

### 2.3 leveling
Rust: `rank`, `leaderboard`, `level add/remove/set`; message + voice XP; hardcoded **disgaea** formula (`store.rs:16-21`).
**Missing:** 7 selectable formulas (disgaea/fibonacci/linear/mee6/atlas/arcane/lurkr) (`leveling/services/Formulas.ts:3-11`); all config (`Index.ts:18-148`) — message xp/cooldown/min-length, voice toggle/rate, announcements (channel/template), ignored channels/roles, **level role rewards** + `role_stack`, `keep_xp_on_leave`, global/role/channel multipliers; `GuildMemberAdd`/`Remove` listeners for reward roles; level-up announcements; `LevelButtons`/`LevelBoardSelect` interactions; RPC; GDPR.

### 2.4 logging
Rust events: `on_message_delete`, `on_message_update`, `on_member_add`, `on_member_remove`, `on_member_update`, `on_ban_add`, `on_ban_remove` (`modules/src/logging/events.rs`), per-type channels, ignored channels.
**Missing:** **log claims** — `ClaimMessage` listener, `Claims` service (code/index/redis TTL, `logging/services/Claims.ts`), RPC `guild.logClaims.list/issue/dismiss`, audit queue entries; `FlushLogs` batched flush task + `Send` service (Rust writes synchronously); `logging.claim` permit node. (Config schema largely matches.)

### 2.5 moderation
Rust: `warn`, `kick`, `ban`, `unban`, `case`, `reason`, `note`, `mute`, `unmute`, `softban`, `lock enable/disable`, `lockdown enable/disable`, `quarantine add/remove`, `dm`, `say`, `sanitize` (`modules/src/moderation/commands.rs`).
**Missing:** `timeout`, `vcmute`, `punishauthor`, `notes` command, `cases` list/revoke; **appeals** entirely (`AppealRepository`, `AppealToken`, `AppealDm`, RPC `appeals.verify/submit/list/review`); warn-threshold auto-actions + `WarnDecayHandler` + `ThresholdRules` + `mod-lift`/`warn-decay` scheduled tasks (`mod/Index.ts:77-81`, `services/*`); multi-target (`max_multi_targets`); predefined-reason autocomplete; immune role exemptions; duplicate-case warning; auto-DM on punish; all RPC (`guild.cases.*`, `guild.warnThresholds.*`, `guild.modNotes.*`, `guild.appeals.*`); config schema mismatch (Rust only `dm_on_action`/`mod_log_channel`, `moderation/mod.rs:30-51`).

### 2.6 tempvc
Rust: `/tempvc`, `/tempvc panel`, `/tempvc admin setup|delete|list`; voice-join creates channel, empty-leave deletes it (`modules/src/tempvc/events.rs`). Panel emits buttons `tempvc:lock|unlock|hide|unhide|trust|untrust|block|kick|transfer|delete` (`tempvc/commands.rs:40-87`).
**Critical gap:** those buttons are **never dispatched** — `events/mod.rs:24-57` routes only `lumi:` to the hub, everything else to `general::handle_component` (ping only). The panel is non-functional.
**Missing:** all panel modes (lock/unlock/hide/trust/block/kick/transfer/rename/limit/claim/delete), admin panel buttons/modals/selects, `PanelGuard`/`PanelHelpers`, `VoiceOccupancy`, `CleanupHandler` (Rust deletes immediately, no `TempvcCleanupDelayMs`), create cooldown, `panel_title/message/color/rich_content` config (Rust `config_schema` empty, `tempvc/mod.rs:34-36`), RPC (`generators.list/set`, `records.list`), permit `tempvc.admin`.

### 2.7 utility
Rust: `avatar`, `banner`, `whois`, `serverinfo`, `nick`, `purge` + `messages/user/bots/links/regex/duration` (`modules/src/utility/commands.rs`).
**Missing:** `View` button interaction (`utility/interactions/buttons/View.ts`) — no dispatcher; `MediaUtils`; `cooldown_seconds` config (Rust `config_schema` empty, `utility/mod.rs:26-28`); RPC.

### 2.8 downloader (shared lib, for completeness)
Rust `downloader.rs` does git operations but **no** commit-signature verification (`@lib/downloader/Signature.ts:13 verifyCommitSignature`) and **no** addon validation / version compatibility (`Validate.ts:198,226 validateAddon/isVersionCompatible`); `pinned` is written `false` (`downloader.rs:374`) with no pin logic.

---

## 3. Permit system

Rust pieces: `PermitRow`/`PermitPolarity`/`NodeScope`/`resolve`/`explain_permit` (`crates/kernel/src/permits.rs:17-537`); hub Permissions tab (`crates/lumi/src/commands/hub/permissions.rs`); DB helpers (`crates/db/src/lib.rs:139-301`).

| TS capability | Rust |
|---|---|
| owner/manager/enforced/custom user/channel/role tier order | **Present** (`permits.rs:206-309`) |
| node match (`*`, `ns.*`, exact) | Present (`permits.rs:63-78`) |
| explain walk | Present (`permits.rs:376-467`), but caller passes `role_ids: Vec::new()`, `is_manager:false` (`hub/permissions.rs:304-311`) so role/manager tiers never appear |
| `guild.permits.list` | DB + hub only, **no API** |
| `guild.permits.create` | **Missing** (no DB fn; TS `PermissionRepository.ts:221`) |
| `guild.permits.update` (rename/re-scope) | **Missing** (TS `:234-264`) |
| `guild.permits.delete` | DB `delete_permit` exists but **no API** |
| `guild.permits.move` | DB `move_permit` + hub `↕ Order`, **no API** |
| `guild.permits.assign/unassign` | DB + hub Revoke/assign, **no API** |
| builtin seed (`Extra Owner` `*`, `Trusted Admin` `admin.*`) | **Missing** (TS `ensureBuiltinPermits` `:52-59,140-157`) |
| Redis permit-chain cache + per-assignment invalidation | **Missing** — Rust `PermitResolver::load` runs once at boot (`client.rs:73`, `bin/main.rs:75`); hub/RPC edits are **not reflected until restart** |
| quarantine integration | **Missing** — `is_quarantined` hardcoded `false` (`client.rs:51`, `hub/mod.rs:326,359`); TS reads live quarantine state (`PermitResolver.ts:116,140`) |
| enforced kind only applies to user targets; survives quarantine | Evaluated user-only (`permits.rs:249-255`), but no quarantine survival |
| `polarity` deny | Model supports (`permits.rs:18-28`, `evaluate_bucket:333`), but **no create/assign path sets deny**, and no RPC exposes it |
| command→node map | Only 8 entries (`permits.rs:117-129`) vs TS `PermitNodes.ts` (`admin.*/config/welcome`, `mod.*/lockdown/notes/softBan/voiceMute/say/dm`, `economy.*`, `reactionroles.*`, `owner.*`, `logging.claim`) and TS per-module `authorize()` |
| RPC auth `guildManager` via Discord REST (`Implement.ts:55-81`) | Rust hub checks live cache instead; API has no guildManager auth |

---

## 4. RPC / API

TS: axum-equivalent HTTP server (`@lib/rpc/HttpServer.ts`) + SSE (`SseServer.ts`, `DashboardEvents`), typed zod router of **~91 actions** across 14 slices (`contracts/rpc/Router.ts:17-49`), auth kinds `guildManager|botOwner|session|public`, idempotency (`Idempotency.ts`), OTEL trace propagation, Discord bulkhead, audit queue, module-enabled checks, completeness verification (`Registry.ts:74-95`).

Rust: single `/action` (`crates/api/src/lib.rs:73-183`) handling **4 actions**:
- `ping` (`:116`), `cache.clear` (stub, `:123`), `modules.list` (`:131`), `security.panic.status` (`:148`).
Plus `/healthz` (`:91`) and `/metrics` (`:95`), constant-time bearer auth (`:194-214`), tower-governor rate limit (`:67-75`).

**Missing:** every TS slice:
- `guild.*` dashboard: shell/module/entities/summaries, module.toggle, config.set/setMany, settings.set, audit.list, history.list/rollback, overrides.list/set, moduleData.list (`contracts/rpc/Dashboard.ts`).
- `guild.permits.*` (7), `guild.blocklist.*`, `guild.ignored.*` (`Core.ts`).
- `guild.cases.*`, `guild.warnThresholds.*`, `guild.modNotes.*`, `guild.appeals.*` (`Mod.ts`).
- `guild.panic.get/set`, `guild.verificationPanel.*`, `guild.verificationWeb.complete`, `guild.backups.*` (`Security.ts`).
- `guild.tempvc.*`, `guild.reactionroles.menus.*`, `guild.logClaims.*`, `guild.welcome.sendTest`, `guild.afk.list`, `guild.economy.*`, `guild.filter.*`.
- `system.*` (dashboard, maintenance, module toggle/clear, identity, audit, blocklist, shards, flags + overrides, tokens, status) (`System.ts`).
- `downloader.*` (repo add/list/modules, module install/uninstall/rollback) (`Downloader.ts`) — the Rust downloader lib has the logic but exposes no action.
- `account.*` (`auth.whoami`, `global.gdpr.*`) (`Account.ts`).
- SSE dashboard event stream, contract version negotiation, idempotency, zod validation, Discord bulkhead, audit trail, module-not-loaded errors.

---

## 5. Scheduler / cache / redis / ratelimit

**Scheduler.** Rust lib exists (`crates/kernel/src/scheduler.rs`, apalis-redis: enqueue/enqueue_delayed/register_interval/run). But the `Worker` and `Scheduler` roles are **no-op stubs** (`crates/lumi/src/bin/main.rs:112-155`) and `Role::All` starts only API + gateway (`:52-65`); nothing ever enqueues or registers jobs. TS has BullMQ-style `Queue`/`JobQueue`/`Runner`, recurring task registry (`Tasks`/`Schedule`/`Producer`), oneshot handlers, distributed scheduler lock + heartbeat, unicast/broadcast task-fire bus over Redis streams, queue metrics/events, OTEL propagation and catch-up. **Missing in Rust:** job handlers, all recurring tasks, oneshot, lock/heartbeat, metrics, task-fire bus.

**Cache.** Rust `cache.rs` is good (moka + Redis JSON KV + cross-process pub/sub invalidation, `cache.rs:42-183`). Missing vs TS `@lib/cache`: TTL on writes, repository `CacheStore`, `GuildContext` aggregation, Redis key/TTL namespacing, cluster-slot handling, `GuildEviction`.

**Redis.** Rust has only the cache connection. TS `@lib/redis` provides cluster/slot helpers (`Cluster.ts`), `Cooldown.ts`, distributed `Lock.ts`, `GuildEviction.ts`, and a full `Store.ts` (`RedisKeys`/`RedisTTL`, `InvalidationBus`, `SignalBus`), plus `@lib/event-bus` streams. **Missing in Rust:** generic cooldown, distributed lock, guild eviction, signal bus, event-bus streams. Leveling "cooldown" is a DB timestamp check only (`leveling/store.rs:50`).

**Ratelimit.** Rust `ratelimit.rs` (governor per-key helper) is **exported but unused anywhere** (`lib.rs:25`; no call sites). The API uses `tower_governor` directly. TS uses `@sapphire/ratelimits`, `@discord/Cooldown`, and the REST adapter for command/interaction limits. **Missing:** command/interaction rate limiting.

---

## 6. Rhai addons vs TS addon loader

**TS (`@lumi/*`).** Addons are ordinary in-process modules loaded from `data/addons` via the same `ModuleStore` (`lib/Services.ts:128-133`), importing the public surface `@lumi/module|commands|context|listeners|interactions|ui|config|gdpr|seal|services|scheduler|bank|time|panels|cooldown` (`tsconfig.base.json:83-112`). An addon can define commands, listeners, interactions, utilities, scheduled tasks, oneshots, config schema, GDPR hooks, RPC handlers and V2 UI cards, with direct DB/Discord/Redis access. Example addons: activity-roles, rolementions, thread-cleaner, status, dragme, promoter, multi-lounge (`/home/rebiz/opt/lumi-addons`). There is no sandbox (documented in `lumi/AGENTS.md`).

**Rust (`crates/script/src/lib.rs`).** `AddonHost` loads `script.rhai` or `*.rhai` files into a Rhai `Engine` with resource limits (`:82-119`) and **only two host functions** — `log_info`/`log_warn` (`:93-98`) — plus `call_function` to invoke top-level functions (`:134-142`). `is_ip_allowed` provides SSRF checks (`:19-64`). Example `addons/ping_addon/script.rhai`.

**Capability gaps:** no host bindings for Discord (send/delete/member/role/channel), DB, config, cache/redis, permits/authorize, scheduler, events/listeners, command registration, interaction/button handling, UI card builders, config schema, GDPR, RPC, or permissions. No lifecycle or event hooks; no per-addon scoping/quotas beyond Rhai operation limits; no ability to contribute a command or listener. Effectively an isolated scripting sandbox rather than an addon platform.

---

### Highest-impact risks
1. **Anti-nuke is inert** — `record_destructive_action` has no audit-log caller; panic doesn't pause invites (`kernel/src/security.rs`, `lumi/src/events/mod.rs`).
2. **Permits never refresh** — resolver loaded once at boot; hub/RPC permit edits don't take effect until restart (`client.rs:73`); quarantine always false.
3. **TempVC panel buttons are dead** — emitted under `tempvc:` but no dispatcher (`tempvc/commands.rs:40-87` vs `events/mod.rs:24-57`).
4. **Scheduler not running** — worker/scheduler roles are stubs; no recurring/oneshot jobs execute (`bin/main.rs:112-155`).
5. **RPC surface ~4%** — 4 actions vs ~91; dashboards cannot configure guilds.
6. **Whole modules absent** — reactionroles, sticky, welcome; security/onboarding/filter mostly absent.

*Skipped: UI card internals, DB schema/migration parity, downloads/signature flows beyond function presence, i18n, and command-argument-level parity. Risk: line numbers are from the working trees as read; grep output for a few symbols was display-redacted, but conclusions are drawn from the files cited directly.*
