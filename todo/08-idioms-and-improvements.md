# 08 — Idioms & Modernization Improvements for Lumi-RS

Ranked recommendations derived from research into top Serenity/Poise bots (TTS Bot, Ferrisbot, Aoede, MrMcEpic) and official SeaORM 2.0 / Moka / Redis idioms. Actionable guide for Agents 1–5.

---

### 1. Strongly-Typed Snowflake IDs (BIGINT Newtypes)
- **Crate**: `crates/db`, `crates/kernel`, `crates/modules`, `crates/lumi` (Agent 1, 2, 3, 5)
- **Why**: Replaces 44 fragile `String` fields and 74 `.get().to_string()` allocations with bit-exact `i64` (`BIGINT`) storage, eliminating snowflake parsing panics and cross-entity ID misassignments.
- **Idiom Source**: TTS Bot stores snowflake IDs as integer primitives in SQL; SeaORM 2.0 `#[derive(DeriveValueType)]`.
- **Sketch**:
  ```rust
  // In lumi_db::ids: GuildId, UserId, ChannelId, SnowflakeId wrap i64 with DeriveValueType
  let gid = lumi_db::ids::GuildId::from(ctx.guild_id().unwrap().get());
  guild::Entity::find_by_id(gid).one(db).await?;
  ```
- **Cross-Agent Action**: Kernel (`module_state`, `permits`, `security`), modules (`store.rs`), and Lumi command handlers must pass newtypes instead of `String`.

### 2. Cache-Stampede Elimination via `moka::future::Cache::try_get_with`
- **Crate**: `crates/kernel` (`cache.rs`) (Agent 1)
- **Why**: Current `Cache::get_json` performs an in-memory check, followed by an uncoordinated Redis query. Concurrent misses on identical keys cause dogpiling / cache stampedes.
- **Idiom Source**: Moka 0.12 `try_get_with` concurrent future memoization.
- **Sketch**:
  ```rust
  pub async fn get_or_load<T, F, Fut, E>(&self, key: &str, loader: F) -> Result<T, Arc<E>>
  where F: FnOnce() -> Fut, Fut: Future<Output = Result<T, E>>, T: Clone + Send + Sync + 'static {
      self.local.try_get_with(key.to_string(), loader()).await
  }
  ```

### 3. Unified Poise Framework Error Handling & Panic Recovery
- **Crate**: `crates/lumi` (`client.rs`, `main.rs`) (Agent 5)
- **Why**: Unhandled command errors currently log raw strings or fail silently. Missing panic handler crashes gateway connections.
- **Idiom Source**: Ferrisbot uses poise `handle_panics = true`; TTS Bot routes all framework errors to a deduped `on_error` dispatcher with user-friendly ephemeral cards.
- **Sketch**:
  ```rust
  poise::FrameworkOptions {
      on_error: |err| Box::pin(async move {
          crate::errors::handle(err).await;
      }),
      ..Default::default()
  }
  ```

### 4. Typed Per-Module Config Tables replacing JSON EAV
- **Crate**: `crates/kernel`, `crates/modules` (Agent 1, 2, 3)
- **Why**: `number`, `boolean`, `id_list`, `string_array` helpers are copy-pasted across `security/store.rs`, `filter/store.rs`, and `logging/store.rs`. Corrupted JSON values cause silent fallback to empty defaults.
- **Idiom Source**: MrMcEpic / Ferrisbot typed serde module structs.
- **Sketch**:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, Default)]
  pub struct SecurityConfig { pub antinuke: bool, pub join_gate: bool, pub action_threshold: u32 }
  // Store as single JSONB column per (guild_id, module_name) with SeaORM model
  ```

### 5. Enforce Zero-Embed Rule via Clippy Disallowed Methods
- **Crate**: `crates/ui`, `crates/lumi`, workspace root (Agent 4, 5)
- **Why**: Guarantees Components V2 design parity; prevents accidental regression to Discord embeds.
- **Idiom Source**: TTS Bot `clippy.toml` `disallowed-methods = ["serenity::builder::CreateMessage::embed"]`.
- **Sketch**: Enforced workspace-wide in `clippy.toml` with `disallowed_methods = "deny"`. All views must emit `lumi_ui::Card` and `lumi_ui::CardReply`.

### 6. SSRF Network Hardening with `std::net` / `ipnet`
- **Crate**: `crates/script` (`lib.rs`) (Agent 5)
- **Why**: Current `is_ip_allowed` uses manual byte checks that miss IPv4-mapped IPv6 (`::ffff:127.0.0.1`), CGNAT (`100.64.0.0/10`), and documentation blocks.
- **Sketch**:
  ```rust
  pub fn is_ip_allowed(ip: &IpAddr) -> bool {
      match ip {
          IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_private() && !v4.is_link_local() && !v4.is_broadcast(),
          IpAddr::V6(v6) => !v6.is_loopback() && !v6.is_unicast_link_local(),
      }
  }
  ```

### 7. Helper Deduplication & Context Extractors
- **Crate**: `crates/kernel`, `crates/ui` (Agent 1, 4)
- **Why**: `cut()` is copy-pasted in 5 places; `ctx.guild_id().ok_or_else(...)` is repeated 46 times; confirm prompts are re-implemented across moderation commands.
- **Idiom Source**: Ferrisbot `checks.rs` context helper; centralized `lumi_ui::confirm` collector.
