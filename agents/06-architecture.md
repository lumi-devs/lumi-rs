# agents/06-architecture.md

## Scale target

~1–11M guilds (Dyno self-reports 11M+). Design for 1M guilds now, headroom to
10M without a rewrite. Pick a real measured target before any sizing decision.

## Cluster model

```
N shard processes (each owns a shard range)
  └─ runs gateway + all modules + timers in-process
        │
        ├─► Postgres (guild_id-routed, guild_id leading in every index)
        └─► Valkey/Redis (pub-sub, streams, addon KV)

api process ──► dashboard/RPC, routes requests to the cluster that owns the guild
rest proxy  ──► shared Discord REST ratelimiter (phase 2)
```

## Shard → guild mapping (deterministic, no registry)

```rust
pub fn shard_for(guild_id: u64, num_shards: u64) -> u64 {
    (guild_id >> 22) % num_shards
}
```

Each StatefulSet pod with ordinal `N` owns shards `[N*S, (N+1)*S)`.
The `api` crate computes the owning pod from this math — no service registry.

## Deploy

- k8s **StatefulSet** for shard processes (stable pod ordinals).
- Single `api` Deployment (stateless).
- Rolling per-cluster restarts only — never fleet-wide cold start.
  (Cold-starting 11k shards with `max_concurrency=16` takes ~57 min.)

## Memory

Serenity's cache grows per guild/member. Minimize it:
- Use `cache::Settings` to disable unused fields.
- Keep `max_messages` tiny (8–16).
- Measure RSS per 1,000 guilds early — this is the first likely wall.

## REST ratelimit

Serenity's ratelimiter is per-process. With >1 process you need a shared
limiter or a proxy. `HttpBuilder` can point at a proxy URL — verify before
building `crates/rest`. Phase 2.

## Build order

1. Monolith with `for_guild()` + minimal cache (now)
2. Timers per shard range
3. Multi-process StatefulSet
4. Memory benchmark (RSS/1k guilds)
5. `rest` proxy
6. PG sharding only if a measured limit forces it
