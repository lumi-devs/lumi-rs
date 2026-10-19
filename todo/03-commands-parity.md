# Lumi TS → Rust Command Parity Audit (READ-ONLY)

## Scope & top-line facts

- TS command files: `/home/rebiz/opt/lumi/src/modules/*/commands/*.ts` (15 modules, 55 command definitions).
- Rust: `/home/rebiz/opt/lumi-rs/crates/modules/src/*/commands.rs` + core `/home/rebiz/opt/lumi-rs/crates/lumi/src/commands/`.
- The Rust repo is **not a port** (per its `AGENTS.md`); it implements a subset. Registration of what exists: `crates/lumi/src/commands/mod.rs:13` (`core_commands`) and `crates/lumi/src/commands/mod.rs:32` (`all`), plus per-module `commands()`.

### TS modules with NO Rust counterpart at all
- **`welcome`** — no Rust module (Rust dirs: afk, downloader, economy, leveling, logging, moderation, tempvc, utility). `/welcome` has no Rust command.
- **`reactionroles`** — no Rust module; `/reactionroles` absent.
- **`sticky`** — no Rust module, **and no TS command files** (listener-only). No action needed.
- **`filter`** — no Rust module, **and no TS command files** (listener-only). No action needed.
- **`logging`** — Rust module exists but declares no commands (`logging/mod.rs` does not override `Module::commands`; default empty at `crates/kernel/src/module.rs:52`). TS logging also has no commands → parity.
- **`core`** — TS core has no commands (listeners/services only). Rust core *adds* commands (`about`, `help`, `ping`, `lumi`, `panic`, `repo`, `download`) — these are treated under onboarding/security/admin below.
- **`onboarding`** — partially present as Rust core commands (`about`, `help`, `ping`, `lumi`); `/mydata` and `/setup` are absent.
- **`security`** — only `/panic` is ported (into core `security.rs`); `/restore` absent.
- **`admin`** — only the downloader subset (`repo`, `download`) is ported (into core); the rest absent.

### How to read the "rs present?" column
`file:line` is the Rust handler function; command name is the fn name with `rename`/`aliases` noted.

---

## Per-command parity table

| module | TS command | rs present? | rs fn / file:line | functional? | missing subs/options | behavioural gaps |
|---|---|---|---|---|---|---|
| admin | `access` (Access.ts:81) | **NO** | — | — | all (block/unblock/allow/unallow/list) | owner block/allow list entirely absent |
| admin | `announce` (Announce.ts:33) | **NO** | — | — | all | global broadcast absent |
| admin | `download` (Download.ts:156) | yes | `download` `core/downloader.rs:133` | functional | **panel, uninstall, rollback**; rust adds `list` | install has no `revision` arg; no confirm prompts; no autocomplete parity |
| admin | `module` (Module.ts:190) | **NO** (command) | hub UI only (`hub/modules.rs`) | n/a | all 14 subs (list/info/enable/disable/install/uninstall/reload/load/unload/status/update/pin/unpin/help) | enable/disable/config exist as hub panel buttons, not a `/module` command |
| admin | `repo` (Repo.ts:23) | yes | `repo` `core/downloader.rs:31` | functional | **help, update, modules** | missing `update`/`modules`; no pagination, no confirm, no signature warnings |
| admin | `serverlock` (Serverlock.ts:9) | **NO** | — | — | all (on/off/status) | absent |
| admin | `shutdown` (Shutdown.ts:7) | **NO** | — | — | — | absent |
| admin | `restart` (Shutdown.ts:22) | **NO** | — | — | — | absent |
| admin | `sv` (Sv.ts:7) | **NO** | — | — | all (leave) | absent |
| afk | `afk` (Afk.ts:39) | yes | `afk` `afk/commands.rs:7` | functional | — | no ALREADY/UPDATED status text; no nickname prefix |
| afk | `afkclean` (Afkclean.ts:11) | **NO** | — | — | — | absent |
| afk | `afklist` (Afklist.ts:10) | yes | `afklist` `afk/commands.rs:23` | functional | — | no pagination; no duration-since |
| afk | `afkstats` (Afkstats.ts:7) | yes | `afkstats` `afk/commands.rs:39` | functional | — | shows count only, not cooldown stats |
| economy | `balance` (Balance.ts:9) | yes | `balance` `economy/commands.rs:11` | functional | — | no configurable vault name/formatting |
| economy | `bank` (Bank.ts:79) | yes | `bank` `economy/commands.rs:108`; deposit:120, withdraw:152, set:265, add:283, remove:301 | functional | none | permit is `MANAGE_GUILD` vs `economy.admin`; no reason max-length |
| economy | `leaderboard` (Leaderboard.ts:10) | **NO** | — | — | `count` option | economy "richest" board absent (Rust `/leaderboard` is leveling XP only) |
| economy | `payday` (Payday.ts:8) | yes | `daily` `economy/commands.rs:36` (alias `payday`) | functional | — | reward hardcoded 250; no cooldown/cap config |
| economy | `pay` (Pay.ts:9) | yes | `pay` `economy/commands.rs:56` | functional | — | **no transfer tax** |
| economy | `slots` (Slots.ts:12) | yes | `slots` `economy/commands.rs:313` | functional | — | no min/max bid config; simpler symbol render |
| leveling | `leaderboard` (Leaderboard.ts:251) | yes | `leaderboard` `leveling/commands.rs:59` | functional | — | **missing voice/weekly/monthly boards**, pagination, image card, user select |
| leveling | `level` (Level.ts:82) | yes | `level` `leveling/commands.rs:91`; add:103, remove:135, set:168 | functional | none | parity (permit name differs) |
| leveling | `rank` (Rank.ts:11) | yes | `rank` `leveling/commands.rs:11` | functional | — | card rendering differs; no "View Leaderboard" button |
| mod | `ban` (Ban.ts:94) | yes (split) | `ban` `moderation/commands.rs:104`; `unban` :137 | functional | **mass `users`, `delete_days`** | Rust ban has no delete-days param (always 0); no confirm; unban separate |
| mod | `cases` (Cases.ts:167) | partial | `case` `moderation/commands.rs:170`; `reason` :193 | functional | **`delete`, list view** | only single-case lookup + reason edit; no list-by-user/action, no case deletion |
| mod | `dm` (Dm.ts:8) | yes | `dm` `moderation/commands.rs:595` | functional | — | parity |
| mod | `kick` (Kick.ts:67) | yes | `kick` `moderation/commands.rs:70` | functional | **mass `members`** | no bulk kick, no confirm |
| mod | `lockdown` (Lockdown.ts:8) | yes | `lockdown` `moderation/commands.rs:423` (+enable:433/disable:469) | functional | — | no confirm prompt; `ADMINISTRATOR` vs `mod.lockdown` |
| mod | `lock` (Lock.ts:31) | yes | `lock` `moderation/commands.rs:370` (+enable:380/disable:403) | functional | — | no already-locked/not-locked state check |
| mod | `notes` (Notes.ts:33) | yes | `note` `moderation/commands.rs:214` | functional | — | Rust also lists notes; no threshold/decay side effects |
| mod | `punish-author` (PunishAuthor.ts:51) | **NO** | — | — | — | message context-menu absent |
| mod | `quarantine` (Quarantine.ts:85) | yes | `quarantine` `moderation/commands.rs:505` (+add:515/remove:555) | functional | — | no ALREADY/NOT-quarantined sentinel messaging |
| mod | `sanitize` (Sanitize.ts:63) | yes | `sanitize` `moderation/commands.rs:650` | functional | — | min-length fallback name differs (`"Sanitized User"` vs empty-reset) |
| mod | `say` (Say.ts:7) | yes | `say` `moderation/commands.rs:619` | functional | — | no explicit bot-can-send permission check; `MANAGE_MESSAGES` |
| mod | `softban` (Softban.ts:64) | yes | `softban` `moderation/commands.rs:334` | functional | — | parity |
| mod | `timeout` (Timeout.ts:84) | yes (renamed) | `mute` `moderation/commands.rs:255`; `unmute` :300 | functional | — | command named `mute`/`unmute` not `timeout`; duration parser simpler |
| mod | `vcmute` (Vcmute.ts:65) | **NO** | — | — | add/remove | voice mute absent |
| mod | `warn` (Warn.ts:38) | yes | `warn` `moderation/commands.rs:39` | functional | — | no warn thresholds/decay handling |
| onboarding | `about` (About.ts:138) | yes | `about` `core/about.rs:11` | functional | — | no dependency/module counts, invite button, Redis version |
| onboarding | `help` (Help.ts:292) | yes | `help` `core/about.rs:93` | functional | — | TS is interactive paginated select; Rust static list (+single-cmd autocomplete lookup) |
| onboarding | `lumi` (Lumi.ts:7) | yes | `lumi` `core/hub/mod.rs:125` (+panel:131) | functional | — | parity; hub has component handlers |
| onboarding | `mydata` (Mydata.ts:9) | **NO** | — | — | whatdata/3rdparty/getmydata/forgetme | GDPR export/deletion absent |
| onboarding | `ping` (Ping.ts:66) | yes | `ping` `core/general/ping.rs:21` | functional | — | no live 10s auto-refresh loop (buttons/pages handled) |
| onboarding | `setup` (Setup.ts:7) | **NO** | — | — | — | setup wizard absent (Rust `setup` string is tempvc admin) |
| security | `panic` (Panic.ts:13) | yes | `panic` `core/security.rs:15` (+on:55/off:111/status:155) | functional | — | TS single-command w/ confirm; Rust adds on/off/status subcommands, no confirm |
| security | `restore` (Restore.ts:9) | **NO** | — | — | list/latest | backup restore absent |
| tempvc | `tempvc` (Tempvc.ts:289) | yes | `tempvc` `tempvc/commands.rs:17` (+panel:102, admin:115, setup:128, delete:155, list:175) | functional | — | TS admin uses interactive channel/select menus (add/manage/rename/limit/remove); Rust uses `admin setup/delete/list` subcommands |
| utility | `avatar` (Avatar.ts:6) | yes | `avatar` `utility/commands.rs:9` | functional | — | no `av` alias |
| utility | `banner` (Banner.ts:6) | yes | `banner` `utility/commands.rs:33` (alias `b`) | functional | — | parity |
| utility | `nick` (Nick.ts:5) | yes | `nick` `utility/commands.rs:151` | functional | — | no role-hierarchy guard; `MANAGE_NICKNAMES` |
| utility | `purge` (Purge.ts:204) | yes | `purge` `utility/commands.rs:261` (+messages:273, user:282, bots:292, links:301, regex:315, duration:330) | functional | none | **regex runs raw `Regex::new` in-process (no sandbox/validation, ReDoS risk)**; max 100 (u8) vs TS 500; no >50 confirm |
| utility | `serverinfo` (Serverinfo.ts:102) | yes | `serverinfo` `utility/commands.rs:110` | functional | — | no channel-type breakdown, boosts, features section, icon button |
| utility | `whois` (Whois.ts:142) | yes | `whois` `utility/commands.rs:62` | functional | — | no key-permissions section, boost status, profile/avatar buttons |
| welcome | `welcome` (Welcome.ts:15) | **NO** | — | — | — | absent (no module) |
| reactionroles | `reactionroles` (Reactionroles.ts:25) | **NO** | — | — | — | absent (no module) |

## 1. Commands present in TS but MISSING in Rust (priority list, grouped)

**admin**
- `/access` (`Access.ts:81`) — all 5 subs
- `/announce` (`Announce.ts:33`)
- `/module` (`Module.ts:190`) — 14 subs; only hub-panel UI exists
- `/serverlock` (`Serverlock.ts:9`) — on/off/status
- `/shutdown` (`Shutdown.ts:7`)
- `/restart` (`Shutdown.ts:22`)
- `/sv` (`Sv.ts:7`) — leave

**afk**
- `/afkclean` (`Afkclean.ts:11`)

**economy**
- `/leaderboard` (`Leaderboard.ts:10`) — richest-members board

**mod**
- `/punish-author` context menu (`PunishAuthor.ts:51`)
- `/vcmute` (`Vcmute.ts:65`) — add/remove

**onboarding**
- `/mydata` (`Mydata.ts:9`) — whatdata/3rdparty/getmydata/forgetme
- `/setup` (`Setup.ts:7`)

**security**
- `/restore` (`Restore.ts:9`) — list/latest

**welcome**
- `/welcome` (`Welcome.ts:15`)

**reactionroles**
- `/reactionroles` (`Reactionroles.ts:25`)

## 2. Commands present in Rust but not TS (extras)

- `unban` (`moderation/commands.rs:137`) — TS folds this into `/ban remove`.
- `mute` / `unmute` (`moderation/commands.rs:255`/`:300`) — TS names this `/timeout`.
- `case` (`moderation/commands.rs:170`) and `reason` (`:193`) — TS uses `/cases view` + `/cases modify`; Rust splits into two commands.
- `note` (`moderation/commands.rs:214`) — TS has `/notes` (add-only); Rust also views notes.
- `panic on` / `panic off` / `panic status` (`core/security.rs:55/111/155`) — TS `/panic` is single-shot with a confirm.
- `download list` (`core/downloader.rs:172`) — TS `/download` has no list sub (TS `/repo modules`/hub instead).
- `lumi panel` (`core/hub/mod.rs:131`) — TS exposes only default sub.
- `help <command>` autocomplete lookup (`core/about.rs:92-130`) — TS `/help` is category pagination only.
- Hub component surfaces (`hub/modules.rs`, `hub/addons.rs`, `hub/permissions.rs`, `hub/settings.rs`) — no TS slash equivalents (TS has `/module` command + dashboard).

## 3. Present in both but missing subcommands/options

- **`/bank`** (`Bank.ts:79` vs `economy/commands.rs:108`): all 5 subs present (deposit/withdraw/set/add/remove). Only behavioural: reason max-length + `economy.admin` permit.
- **`/purge`** (`Purge.ts:204` vs `utility/commands.rs:261`): all 6 types present (messages/user/bots/links/regex/duration). Gaps: regex sandbox/validation, 500 vs 100 cap, >50 confirm.
- **`/level`** (`Level.ts:82` vs `leveling/commands.rs:91`): add/remove/set present; parity.
- **`/download`** (`Download.ts:156` vs `downloader.rs:133`): **missing `panel`, `uninstall`, `rollback`**; Rust adds `list`; install lacks `revision`.
- **`/repo`** (`Repo.ts:23` vs `downloader.rs:31`): **missing `help`, `update`, `modules`**; no pagination/confirm/signature warnings.
- **`/cases`** (`Cases.ts:167` vs `case`/`reason`): **missing `delete` and list view**; no list-by-user/action filters.
- **`/ban`** (`Ban.ts:94`): **missing mass `users` and `delete_days`**; Rust never passes delete-seconds.
- **`/kick`** (`Kick.ts:67`): **missing mass `members`**.
- **`/leaderboard`** (leveling, `Leaderboard.ts:251`): **missing `voice`/`weekly`/`monthly` board kinds, pagination, image card**.
- **`/tempvc admin`** (`Tempvc.ts:289` vs `tempvc/commands.rs:115`): TS interactive menus (add/manage/rename/limit/remove) vs Rust `setup/delete/list`; coverage differs.
- **`/about`, `/whois`, `/serverinfo`, `/help`, `/ping`, `/nick`, `/rank`, `/pay`**: present but with reduced field/feature sets (see table).

## 4. Rust stubs

No registered Rust command is a pure placeholder: every handler in `afk/commands.rs`, `economy/commands.rs`, `leveling/commands.rs`, `moderation/commands.rs`, `utility/commands.rs`, `tempvc/commands.rs`, and core `about.rs`/`downloader.rs`/`security.rs`/`general/ping.rs`/`hub/mod.rs` performs real work.

Caveat worth flagging (not a stub, but a shell): the Rust **`downloader` module directory** `crates/modules/src/downloader/` is empty; the actual `/repo` + `/download` live in core `crates/lumi/src/commands/downloader.rs`, so the module-registry route (`all_from_registry`) does not own them.

### Notes / limits of this audit
- I read every TS file under `*/commands/*.ts` and every Rust file under `crates/modules/src/*/commands.rs` + `crates/lumi/src/commands/`. Line numbers for Rust point at the handler `fn`; the registered slash name is the fn name unless `rename`/`aliases` is shown.
- `sticky` and `filter` are excluded from tables because neither has TS command files (listener-only) and neither has a Rust module.
- Behavioural "gaps" are based on the command bodies as read, not runtime testing; permit-checking in Rust relies on `default_member_permissions` rather than the TS permit-node system, which accounts for many of the "permission" differences.

I did not run the bots or verify live Discord registration; I also did not inspect the TS `commandRegistry` to confirm the two same-named `/leaderboard` defs (economy + leveling) coexist at runtime — if TS disambiguates them per-module, the Rust single-XP replace is a real regression.

**Ponytail note:** read-only report, nothing built or run; the biggest unverified risk is that both TS modules register a `/leaderboard`, which would silently collide once Rust only ships the leveling one.
