# lumi-rs

A Discord bot written in Rust for fleet-scale operation: sharded gateway
workers, stateless HTTP interactions, Postgres + Redis storage, and a small
module runtime for feature code.

## Layout

| Crate | Role |
|-------|------|
| `crates/kernel` | Config, cache, coordinator, services, shared primitives |
| `crates/db` | SeaORM entities, migrations, typed query helpers |
| `crates/ui` | Discord V2 component builders (cards, panels, pagination) |
| `crates/modules` | Feature modules implementing the `Module` trait |
| `crates/api` | Axum server for signed HTTP interactions (`POST /api/interactions`) |
| `crates/lumi` | Bot binary: gateway client, poise framework, event dispatcher |
| `xtask` | Operational helpers (`xtask migrate`) |

Dependency direction: `lumi` → `api`/`modules` → `ui`/`db` → `kernel`.
See `agents/` for the full architecture notes.

## Quickstart

```sh
nix develop                                    # toolchain + services
docker compose -f docker/docker-compose.yml up -d   # postgres, pgbouncer, redis
cp .env.example .env                           # fill in BOT_TOKEN
cargo run -p lumi -- migrate                   # or: cargo run -p xtask -- migrate
cargo run -p lumi
```

## Configuration

| Variable | Required | Notes |
|----------|----------|-------|
| `BOT_TOKEN` (or `DISCORD_TOKEN`) | yes | Gateway token |
| `DATABASE_URL` | yes | Writes; use PgBouncer (`:6432`) in transaction mode |
| `DATABASE_READ_URL` | no | Read replica for read-only queries |
| `REDIS_URL` | no | Defaults to `redis://127.0.0.1:6379/` |
| `DISCORD_PUBLIC_KEY` | no | Enables `/api/interactions` signature checks |
| `RPC_PORT`, `RPC_TOKEN` | no | Inter-node RPC |
| `OWNERS` / `OWNER_IDS` | no | Comma-separated owner snowflakes |
| `SHARDS` | no | Total shard count; defaults to Discord's recommendation |
| `SHARD_RANGE` | no | `start-end` subset for this node |
| `SHARD_COORDINATOR` | no | Enables Redis shard-lease coordination |
| `MAX_SHARDS_PER_NODE` | no | Cap for leased shards |
| `INTENTS_GUILD_MEMBERS` | no | Opt in to the privileged members intent |

## Development

```sh
cargo test --workspace        # unit + integration tests
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

Workspace lints deny `unwrap`, `expect`, `allow`, and unsafe-looking patterns
(`Cargo.toml` `[workspace.lints]`). Local Postgres and Redis are required for
the integration tests.
