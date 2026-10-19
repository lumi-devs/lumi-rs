# UX/UI Parity Audit — TS `lumi` vs Rust `lumi-rs`

Read-only. All file:line references verified. Priority given to functional gaps.

---

## 1. Hub panel

### 1a. Tab bar

| area | TS behaviour | Rust behaviour | verdict | gap |
|---|---|---|---|---|
| Active tab | `Panels.ts:391-410` — `tabRow` sets `.setDisabled(active)` (line 403); label clipped to 80 chars | `hub/mod.rs:522-547` — `tab_button` only styles active Primary, **not disabled**; no label clip | partial | Rust active tab is clickable (re-renders same tab); TS blurs it out. Minor. |

### 1b. Home tab

| area | TS behaviour | Rust behaviour | verdict | gap |
|---|---|---|---|---|
| Locale line | `Hub.ts:74-78` shows `🏰 Language \`xx\` • Prefix \`,\`` | `home.rs:26` shows only `🏰 Prefix \`,\`` | **partial** | Rust home never shows the server language. |
| Hints | `Hub.ts:82-85` Settings hint = "server language and command prefix" | `home.rs:17` Settings hint = "server command prefix" | partial | Reflects the missing language feature (below). |
| Footer/breadcrumb/title | `Hub.ts:90-99` | `home.rs:37-40` | match | — |

### 1c. Settings tab — **largest hub gap**

| area | TS behaviour | Rust behaviour | verdict | gap |
|---|---|---|---|---|
| Language selector | `Hub.ts:127-136` — string select `lumi:setlang` listing `SupportedLanguages` (≤25), default-marked | `settings.rs:15-55` — **no language select, no locale row at all** | **missing** | Users cannot change server language in Rust. `handle_component` has no `setlang` action (`hub/mod.rs:278-296`). |
| Prefix edit/reset | `Hub.ts:113-150` section + Edit/Reset/Update Addons | `settings.rs:23-49` section + Edit/Reset/Update Addons | match | — |
| Prefix default marker | `Hub.ts:107-111` shows `(default)` when unset | `settings.rs:21` `is_default` computed but only used to disable Reset; never rendered | partial | Rust doesn't tell the user the prefix is the default. |

### 1d. Modules tab

| area | TS behaviour | Rust behaviour | verdict | gap |
|---|---|---|---|---|
| List per page | `Modules.ts:32` 4/page, sorted by displayName (`:95`) | `modules.rs:15` 8/page, registry order | partial | Different density/order. |
| List action | `Modules.ts:109-117` one "Open" button per row | `modules.rs:36-49` "Configure" button **plus** a row of enable/disable toggle buttons (`:53-67`, label = module name) | partial | Toggle is in Rust list (label is the module name, state only shown by colour) vs TS detail ("Enable/Disable Module"). |
| Empty body | `Modules.ts:137-140` "*No features registered.*" | `modules.rs:77-81` "*No modules loaded.*" | partial | wording |
| Detail: bool toggle | `Modules.ts:212-225` inline ✅/❌ button per boolean | `modules.rs:190-222` booleans folded into one edit select "Label: On/Off" | partial | Different interaction; loses per-field inline toggle. |
| Detail: text/number/duration edit | `Modules.ts:227-238` "Edit" → modal (fedit) | **none** — non-enum/bool fields are read-only in Rust | **missing** | Can't set string/number/duration config from Rust panel. |
| Detail: enable/disable | `Modules.ts:271-300` "Enable/Disable Module" button | none in detail (only list) | partial | present, different location |
| Detail: Reset / Back | `Modules.ts:289-299,324-328` | `modules.rs:155-164` Reset Override / Back | match | — |
| Detail: History | `Modules.ts:329-338` "History" button | **none** | **missing** | No config-change history view. |
| Detail: Overrides | `Modules.ts:341-353` "Overrides" when `meta.configOverrides` | **none** | **missing** | No per-command override UI. |
| Detail: section grouping | `Modules.ts:166-186,302-322` named sections + "Jump to a section…" select | `modules.rs:126-129` flat field slice, no section names | **partial/missing** | Rust loses config section labels/grouping and jump. |
| Field edit view | `Modules.ts:387-522` channel/role/user/enum selects + "Enter value…" modal + "Back to Feature" | `modules.rs:146-171` single combined select + Reset Override + Back + pagination | partial | No dedicated per-field page; no channel/role/user pickers (only enum/bool strings). |

### 1e. Permissions tab

| area | TS behaviour | Rust behaviour | verdict | gap |
|---|---|---|---|---|
| Overview rows + Revoke | `Permissions.ts:171-206` | `permissions.rs:31-70` | match | — |
| Add Custom/Enforced/Order/Explain | `Permissions.ts:185-206` | `permissions.rs:53-70` | match | — |
| Pagination 4/page | `Permissions.ts:125,209-217` | `permissions.rs:14,73-75` | match | — |
| Footer wording | `Permissions.ts:228` `formatPageFooter(..., "N assignment(s)")` | `permissions.rs:79` `PageFooterHint::Count` → "N item(s)" | partial | Rust says "items" not "assignments". |
| Permit Order | `Permissions.ts:247-298` up/down, full numbered list, 3/page | `permissions.rs:104-164` same | match | — |
| Explain picker (user + node select + node pagination 25) | `Permissions.ts:93-123` | `permissions.rs:247-295` | match | Node sort: TS `localeCompare` vs Rust byte `sort` (`:359`). Minor. |
| Explain detail | `Permissions.ts:27-48` uses passed `subjectLabel` | `permissions.rs:344-352` renders `<@id>` | partial | cosmetic |
| Pick permit / assign target (role vs user) | `Permissions.ts:300-365` | `permissions.rs:167-244` | match | — |

### 1f. Addons tab

| area | TS behaviour | Rust behaviour | verdict | gap |
|---|---|---|---|---|
| Dashboard intro sentence | `Addons.ts:88-91` | `addons.rs:36-39` omits intro | partial | — |
| Pending updates | `Addons.ts:98-102` lists **module names** | `addons.rs:40-42` shows **count only** ("1") | **partial** | Rust tells you updates exist but not which. |
| Owner-access footer | `Addons.ts:127-130` "Bot Owner access is required…" | omitted | partial | Rust *does* owner-gate (`addons.rs:485`) but gives no hint. |
| Nav buttons | `Addons.ts:104-120` Configure Repos / Configure Addons / Auto-Update | `addons.rs:44-61` Repositories / Installed / Auto-update / **Add Repo** | partial | labels differ; Rust adds Add Repo on dashboard. |
| Repos view | `Addons.ts:164-220` per-repo commit + **installed count** + update; add/remove repo | `addons.rs:76-144` per-repo branch+commit + update; add/remove repo | partial | Rust omits per-repo installed count. |
| Installed view | `Addons.ts:254-323` per-module toggle + repo-pick select; **no uninstall button** | `addons.rs:147-211` per-module **Uninstall** accessory + toggle rows | partial | Different model; Rust adds uninstall. |
| Repo modules install/uninstall | `Addons.ts:325-434` button + info button per module | `addons.rs:274-371` "Install or uninstall…" select + info buttons | partial | Different interaction. |
| Update confirm | `Addons.ts:222-252` Update/Skip | `addons.rs:415-440` Update/Skip | match | — |
| Auto-update intervals | `Addons.ts:63-69` 60/360/720/1440/**10080** | `addons.rs:18-24` **15**/60/360/720/1440 | partial | Rust adds 15-min, drops 7-day. |

---

## 2. Core cards

| area | TS behaviour | Rust behaviour | verdict | gap |
|---|---|---|---|---|
| `makeCard`/success/error/warning/info | `Cards.ts:168-197` | `cards.rs:335-360` | match | — |
| Container build (breadcrumbs/badge/title/subtitle/separator/thumbnail/gallery/footer/action rows) | `Cards.ts:52-160` | `cards.rs:186-288` | match | 1:1 |
| `makeListCard` (25 visible, 200-char cut, `-# …and N more`) | `Cards.ts:224-242` | `cards.rs:363-382` | match | — |
| `makeEmptyCard` (📭 title, "**Try:**", default footer) | `Cards.ts:244-262` | `cards.rs:385-410` | match | — |
| `fitLines` budget/notice | `Cards.ts:201-222` | `cards.rs:304-322` | match | — |
| `formatPageFooter` | `Layout.ts:52-65` | `layout.rs:59-68` | match | — |
| Breadcrumbs / `formatBreadcrumbHeader` | `Layout.ts:39-50` | `layout.rs:26-49` | match | — |
| Status badge / subtitle | `Layout.ts:1-37` | `layout.rs:4-23` | match | — |
| Pagination row | `Panels.ts:220-253` label `'{n} / {total}'`, emoji ⬅️/➡️, always built | `panels.rs:94-116` label `'{n}/{total}'`, emoji ◀/▶, **omitted when 1 page** | partial | Spacing/emoji differ; Rust callers must guard. |
| Back button | `Panels.ts:194-202` "← Back" | `panels.rs:84-91` "Back" + ⬅ emoji | partial | wording |
| `headerImages`/media gallery | `Cards.ts:125-138` | `cards.rs:248-267` | match | — |
| `buildSafeActionRows` (5-row/5-comp clamp) | `Panels.ts:255-277` | **no equivalent**; callers chunk manually (`modules.rs:65`, `addons.rs:175`) | partial | Rust has no global clamp/logging safeguard. |
| `Palette` keys | `Palette.ts` (primary,neutral,gold,purple,cyan) | `cards.rs:17-47` `CardKind` incl. Neutral/Gold/Purple/Cyan | match | — |

Core card surface is essentially at parity; only pagination/back-button wording and the missing action-row clamp differ.

---

## 3. Command replies

### ping — `Ping.ts` + `PingCards.ts` vs `general/ping.rs` + `ui/ping.rs`

| area | TS | Rust | verdict | gap |
|---|---|---|---|---|
| Overview layout | `PingCards.ts:82-130` single card: per-shard `### __Shard N__` with Latency/Uptime/RAM/CPU/Servers/Members, then External Services (Redis read ms, DB ms + tx/s) | `ping.rs:263-317` `__## {bot} Status:__`; 5 shards/page; per-shard one-liner; External Services shows cache hit-ratio + DB latency only | **partial** | Rust drops CPU%, Redis read ms and DB load; different KPI set. |
| Shard pagination | none (all shards, one card) | prev/next, 5/page (`ping.rs:247-260,276-279`) | partial | Rust-only. |
| Live updates (10 s × 1 min) | `Ping.ts:27-64` edits the message | none | **missing** | Rust ping is a static snapshot. |
| Detail categories (gateway/engine/host/postgres/redis/bot) | `PingCards.ts:399-443`; triggered via handler `Ping.ts:25-27` | `ping.rs:319-639`; triggered via handler `general/ping.rs:122-132` | match | Both overviews expose **no** category selector (TS overview has no components; Rust nav is only prev/close/next). Detail cards are effectively unreachable from the overview in both. |
| Close button | TS overview has no buttons | Rust overview has `Close` (`ping.rs:252-254`) | partial | Rust-only. |

### help / about

| area | TS | Rust | verdict | gap |
|---|---|---|---|---|
| help model | `Help.ts:109-307` category **select**, Previous/Next pagination, per-category pages, footer count, disabled-on-timeout | `about.rs:92-174` one static card, all categories concatenated; `/help <cmd>` returns a card | **partial/missing** | Rust has no category select or pagination interaction. |
| about | `About.ts:22-136`: invite button, codebase lines/modules/deps, Bun/discord.js/Postgres versions, uptime relative | `about.rs:11-89`: no invite button, no codebase metrics, "Rust vX · Serenity/SeaORM", extra `Interval`/no invite | **partial** | Rust lacks Invite button and codebase stats; shows version twice. |

### ban / kick / warn / case / notes

| area | TS | Rust | verdict | gap |
|---|---|---|---|---|
| Confirmation prompt | `ModerationFlow.ts:286-304` confirm/cancel buttons before acting | `moderation/commands.rs:38-133` executes immediately | **missing** | Rust applies destructive actions with no confirm step. |
| Mass targets | Ban `Ban.ts:23-38` user + `users` list; Kick `Kick.ts:25-38` member + `members` list | single `user: UserId` only | **missing** | No multi-target ban/kick. |
| ban `delete_days` | `Ban.ts:45,57` 0–7 → deleteMessageSeconds | `commands.rs:115` hardcodes `0` | **missing** | Can't purge messages on ban. |
| Cases list / filter / pagination / delete | `Cases.ts:42-105` `cases view` by user+action, 10/page footer; `delete` (`:139-165`) | `commands.rs:170-233` only `case <n>` + `reason <n>`; no list, no filter, no delete | **missing** | Big functional gap in Rust cases. |
| Warn count in reply | `Warn.ts:29-34` includes `count` | `views.rs:53-72` no warn count | partial | — |
| Success-card fields | TS title/reason/case via i18n | `views.rs:53-72` case + reason | match | — |
| Reason autocomplete | `ReasonAutocomplete.ts` | `commands.rs:14-35` presets | match | preset list may differ; functionally equal |

### economy — `Balance.ts` / `Bank.ts` / `Leaderboard.ts` vs `economy/commands.rs`

| area | TS | Rust | verdict | gap |
|---|---|---|---|---|
| balance | `Balance.ts:40-48`: `💰 {username}`, Wallet / {vaultName} / Total; configured currency formatting | `commands.rs:21-28`: `💰 Balance — <@id>`, Wallet / Bank / **Net Worth**, hardcoded "coins" | **partial** | No configurable currency/vault name; "Total"→"Net Worth"; uses mention not username. |
| bank deposit/withdraw | `Bank.ts:52-77` uses vaultName + formatted amounts | `commands.rs:119-180` "bank" + raw numbers | partial | same config gap |
| bank set/add/remove | `Bank.ts:19-50` message uses `formatAmount` | `commands.rs:196-255` raw amounts | partial | — |
| economy leaderboard | `Leaderboard.ts:10-63` `paginateList`, title `🏆 Richest in {guild}`, "#rank mention - amount", "← you" marker, 10/page | **no `leaderboard` command** (`commands.rs:360-362` = balance,daily,pay,bank,slots) | **missing** | No economy leaderboard at all in Rust. |

### leveling — `Rank.ts` / `Leaderboard.ts` vs `leveling/views.rs` / `commands.rs`

| area | TS | Rust | verdict | gap |
|---|---|---|---|---|
| rank | `Rank.ts:46-77` **rendered PNG** (avatar, progress bar, messages/voice) + "View Leaderboard" button | `views.rs:5-32` text card; no image, no button | **missing** | Rust rank is plain text and non-interactive. |
| leaderboard | `Leaderboard.ts:143-295` **rendered PNG**, overall/voice/weekly/monthly **select**, pagination | `views.rs:44-75` text card, top 10, no boards/pagination | **missing** | Rust loses board kinds and pagination. |

### tempvc — `Panel.ts` vs `tempvc/views.rs` + `commands.rs`

| area | TS | Rust | verdict | gap |
|---|---|---|---|---|
| Panel | `Panel.ts:34-189`: status badges (LOCKED/UNLOCKED, HIDDEN/VISIBLE), **11-option select menu**, **Claim Ownership** button, configurable title/message/color/rich blocks | `commands.rs:21-98` static rows of buttons (lock/unlock/hide/unhide/trust/untrust/block/kick/transfer/delete); no badges, no select, no Claim | **missing** | No claim flow, no status badge, no owner-config maybe; different model. |
| Sub-views (kick/trust/untrust/block/unblock/transfer/delete) | `Panel.ts:281-437` user/role selects + confirm delete + back | **none** (only generator admin cards + `views.rs:47-49` error card) | **missing** | Rust `trust/block/kick` buttons have no target-picker UI. |
| Admin setup/list/delete | `Tempvc.ts` (admin) | `commands.rs:106-204` setup/delete/list cards | partial | present |
| Generator setup card | — | `views.rs:3-14` plain card (VS TS likely badge/thumbnail) | partial | cosmetic |

### avatar / whois / serverinfo

| area | TS | Rust | verdict | gap |
|---|---|---|---|---|
| avatar | `MediaUtils.ts`: cooldown, ephemeral for others, MediaGallery image + link / "View" button | `commands.rs:8-29` thumbnail + direct link only | **partial** | No media gallery, no cooldown, no view-button; no ephemeral-for-others logic. |
| whois | `Whois.ts:34-140`: user section, member info, **roles list with overflow**, **key permissions**, boosting, Profile/Avatar link buttons, member display colour | `commands.rs:61-106`: account/created/joined/nick/roles; no permissions, no boosting, no buttons, no display colour | **missing fields** | Rust shows materially less. |
| serverinfo | `Serverinfo.ts:20-100`: owner+created+guild id; members+boosts; text/voice/category/total channels; roles, emojis, verification level; icon link button | `commands.rs:109-147`: guild id/owner/created/members/channels total/roles | **missing fields** | No channel breakdown, emojis, verification, boosts, icon button. |

---

## 4. Footer / legend / pagination wording

| string | TS | Rust | note |
|---|---|---|---|
| Pagination indicator | `'{n} / {total}'` (`Panels.ts:237`) | `'{n}/{total}'` (`panels.rs:113`) | spacing |
| Pagination emoji | ⬅️/➡️ (`Panels.ts:231,244`) | ◀/▶ (`panels.rs:104,109`) | visual |
| Back button | "← Back" (`Panels.ts:196`) | "Back" + ⬅ (`panels.rs:87-88`) | visual |
| Permissions footer | "N assignment(s)" (`Permissions.ts:228`) | "N item(s)" (`permissions.rs:79`) | meaning drift |
| Modules list footer | "Open a module to enable, disable, or configure it." (`Modules.ts:151`) | "Open a module to configure it." (`modules.rs:88`) | loses enable/disable hint |
| Modules empty | "No features registered." (`Modules.ts:139`) | "No modules loaded." (`modules.rs:78`) | wording |
| Addons owner hint | present (`Addons.ts:130`) | absent (`addons.rs`) | meaningful omission |
| Ping External Services | Redis read ms + DB load (`PingCards.ts:124-125`) | cache hit-ratio + DB latency (`ping.rs:308-312`) | different KPI legend |
| TempVC status legend | LOCKED/UNLOCKED + HIDDEN/VISIBLE badges (`Panel.ts:51-70`) | absent (`commands.rs`) | meaningful omission |

---

## Top functional gaps (ranked)

1. **Hub language selection entirely missing in Rust** (`Hub.ts:127-136` vs `settings.rs`).
2. **TempVC panel**: no Claim button, no status badges, no select menu, no trust/block/kick/transfer sub-view pickers (`Panel.ts` vs `tempvc/commands.rs`).
3. **Leveling**: rank & leaderboard are plain text instead of rendered images; no board kinds/select/pagination (`Rank.ts`,`Leaderboard.ts` vs `leveling/views.rs`).
4. **Moderation**: no confirmation step, no mass targets, no ban `delete_days`; `cases` lacks list/filter/delete/pagination.
5. **Economy leaderboard command absent**; no configurable currency/vault name.
6. **Hub module config**: no text/number/duration editing, no per-field editors, no History/Overrides/section-jump.
7. **whois/serverinfo** missing permissions, boosts, channel breakdown, emojis, verification, link buttons.
8. **Ping**: no live refresh; different KPI set.
9. **help**: static single card (no category select/pagination).
10. **Addons**: pending-update names replaced by a count; owner-access hint dropped.

---

Skipped: addons/downloader/admin/security modules, filter/leveling listeners, and the dashboard RPC surfaces — outside the requested UX scope. Risk: I compared presentation and flow only; I did not execute either bot, so some "unreachable" component handlers (e.g. ping category nav) may be wired by a dispatcher I did not trace.
