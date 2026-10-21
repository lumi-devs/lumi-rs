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
cp .env.example .env                           # fill in BOT_TOKEN + DISCORD_PUBLIC_KEY
cargo run -p xtask -- migrate                  # optional; lumi also migrates on boot
cargo run -p lumi
```

## Process roles

`lumi [all|gateway|api|worker]` selects the process boundary. Every role builds
one shared `Services` instance (DB, read replica, Redis, permits, modules) and
validates its config before any task starts. Roles are entry points in one
binary rather than separate services, so API and worker work is an in-process
crate call with no extra RPC hop.

| Role | Gateway session | Requires | Runs |
|------|-----------------|----------|------|
| `gateway` | yes | `BOT_TOKEN` | shard range, gateway events, commands |
| `api` | no | `DISCORD_PUBLIC_KEY` | `/action`, `/healthz`, `/metrics`, `/api/interactions` |
| `worker` | no | — | scheduled background jobs (addon auto-update) |
| `all` | yes | both | all of the above in one process (dev/single box) |

Bad config fails at boot: wrong storage URL scheme, missing gateway token,
malformed or missing interaction key for `api`/`all`, and inconsistent
`SHARDS`/`SHARD_RANGE`/`SHARD_COORDINATOR` settings. A node that loses its
Redis shard lease shuts its shards down instead of serving a duplicate session.

## Configuration

| Variable | Required | Notes |
|----------|----------|-------|
| `BOT_TOKEN` (or `DISCORD_TOKEN`) | gateway/all | Gateway token |
| `DATABASE_URL` | yes | Writes; use PgBouncer (`:6432`) in transaction mode |
| `DATABASE_READ_URL` | no | Read replica for read-only queries |
| `REDIS_URL` | no | Defaults to `redis://127.0.0.1:6379/` |
| `DISCORD_PUBLIC_KEY` | api/all | Ed25519 key used to verify `/api/interactions`; 64 hex chars |
| `RPC_PORT`, `RPC_TOKEN` | no | Inter-node RPC; without `RPC_TOKEN`, `/action` is unauthenticated |
| `OWNERS` / `OWNER_IDS` | no | Comma-separated owner snowflakes |
| `SHARDS` | no | Total shard count; defaults to Discord's recommendation |
| `SHARD_RANGE` | no | `start-end` subset for this node |
| `SHARD_COORDINATOR` | no | Enables Redis shard-lease coordination (needs `SHARDS`) |
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
