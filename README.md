# Lumi

Lumi is a Discord bot and distributed microservices platform written in Rust, engineered for high-concurrency fleet operations. Its architecture supports single-process monolithic execution as well as clustered, multi-node deployments capable of serving thousands of shards and millions of guilds. Lumi provides gateway connection management, stateless HTTP interaction dispatching, PostgreSQL storage with PgBouncer connection pooling, Redis caching and distributed shard-lease coordination, and an isolated module runtime for feature code. Lumi does not use legacy Discord embeds, does not execute unchecked raw SQL in feature modules, and does not bundle cloud credentials or proprietary assets.

## Quick start

The recommended local development workflow uses Nix with Flakes enabled, or a standard Rust 1.85+ toolchain with Clang and Mold installed.

1. Ensure Docker Compose, Git, and the Rust toolchain are available.
2. Clone this repository and open a terminal in its directory.
3. Start the local database, connection pooler, and cache services:
   ```sh
   docker compose -f docker/docker-compose.yml up -d
   ```
   This launches PostgreSQL 18 on port 5432, PgBouncer on port 6432, and Redis 8 on port 6379.
4. Create your local configuration from the sample environment template:
   ```sh
   cp .env.example .env
   ```
   Edit `.env` and set `BOT_TOKEN` to your Discord bot token.
5. Run database migrations using the operational helper:
   ```sh
   cargo run -p xtask -- migrate
   ```
6. Start the Lumi unified process:
   ```sh
   cargo run -p lumi
   ```
   By default, this launches in the `all` role, starting the Discord gateway client, HTTP and RPC servers, and background worker concurrently.

## Deployment roles

Lumi binaries accept an optional role argument (`lumi [ROLE]`) to separate responsibilities across container fleets:

- `all`: The default unified role. Concurrently runs gateway shards, the HTTP and RPC servers, and background workers in a single process. Recommended for local development and small-to-medium single-server deployments.
- `gateway` (or `shard`): Runs the Discord gateway client and Poise event dispatcher. Connects to Discord, establishes shard sessions, and executes registered feature modules.
- `worker`: Headless background task worker without a Discord gateway connection. Connects to Redis and executes scheduled jobs such as automated addon repository updates.
- `api`: Stateless Axum HTTP server hosting administrative RPC actions, Prometheus metric export at `/metrics`, Kubernetes health probes at `/healthz`, and Discord HTTP interaction webhooks at `/api/interactions`.

## Shard coordination and clustering

For clustered deployments spanning multiple nodes or Kubernetes StatefulSets, Lumi coordinates shard assignments dynamically or statically:

- Static slicing: Set `SHARDS` to the fleet total and `SHARD_RANGE` to the inclusive slice for that pod (such as `0-7` or `8-15`). Pods calculate guild shard affinity using the deterministic Discord formula `(guild_id >> 22) % num_shards`.
- Dynamic coordinator: Set `SHARD_COORDINATOR=true` and configure `REDIS_URL`. Nodes automatically lease contiguous shard ranges from Redis using atomic Lua scripts and maintain leases via background heartbeats. If a node loses its lease or network connectivity to Redis, it gracefully fences itself and disconnects gateway shards to prevent split-brain collisions.

## Architecture and crate layout

The repository is organized as a Cargo workspace with strict dependency hierarchy:

- `crates/lumi`: Bot binary, role-based execution entrypoints, Poise application command tree, client bootstrap, and gateway event dispatching.
- `crates/kernel`: Shared core runtime, configuration validation, service container (`Services`), Redis cache with invalidation pub-sub, distributed shard coordinator, rate limiters, scheduler, and module registry.
- `crates/db`: SeaORM 2.0 schema, entity models, database migrations, snowflake ID newtypes (`GuildId`, `UserId`, `ChannelId`), connection retries, and transactional counters.
- `crates/ui`: Discord Components V2 presentation primitives (action rows, buttons, select menus, containers, status panels, and pagination).
- `crates/modules`: Self-contained feature modules implementing the `Module` trait, including AFK tracking, audit logging, auto-moderation filtering, full moderation suites, security and anti-raid panic modes, temporary voice channels, general utilities, virtual economy, leveling, and addon downloader tools.
- `crates/api`: Axum HTTP and RPC endpoints, Ed25519 signature verification for Discord interactions, Prometheus recorder, and rate-limited management RPC.
- `crates/script`: Sandboxed Rhai addon execution host featuring hard CPU operation budgets, memory limits, and strict SSRF filtering against private network CIDRs.
- `xtask`: Independent CLI operational helper for executing database migrations without importing Discord gateway libraries.
- `docker`: Local infrastructure stack containing PostgreSQL 18, PgBouncer in transaction mode, and Redis 8.

## Configuration

Lumi loads settings from process environment variables and optional local `.env` files. The binary validates loaded values against the designated execution role before opening connections.

```sh
# Discord gateway credentials
BOT_TOKEN=your_bot_token_here

# Primary database connection (writes and schema migrations)
# In production, connect through PgBouncer on port 6432 in transaction mode
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/lumi

# Optional read-replica database connection for read-heavy queries
# DATABASE_READ_URL=postgres://postgres:postgres@127.0.0.1:6432/lumi

# Redis / Valkey connection string for caching, pub-sub, and coordination
REDIS_URL=redis://127.0.0.1:6379/

# Logging filter directives (tracing-subscriber)
RUST_LOG=info,lumi=debug,lumi_kernel=debug

# Discord application public key (64-character hex string)
# Required when running the 'api' or 'all' roles to verify incoming POST /api/interactions webhooks
# DISCORD_PUBLIC_KEY=

# Internal RPC and metrics server port (default: 8080)
# RPC_PORT=8080

# Bearer token for authenticating POST /action requests (leave unset only for local development)
# RPC_TOKEN=

# Comma-separated list of Discord user IDs with bot administrator privileges
# OWNERS=123456789012345678,987654321098765432

# Total fleet shard count; when unset, defaults to Discord's recommended total
# SHARDS=16

# Inclusive shard range assigned to this specific node (e.g. 0-7, 8-15)
# Requires SHARDS to be defined
# SHARD_RANGE=0-7

# Enable Redis dynamic shard-lease coordination across container fleets
# SHARD_COORDINATOR=false

# Maximum number of shards a single node may lease under dynamic coordination
# MAX_SHARDS_PER_NODE=16

# Privileged gateway intent opt-in for member events (GUILD_MEMBERS)
# INTENTS_GUILD_MEMBERS=false

# Directory for user-installed addon modules and Rhai scripts
# CUSTOM_DIR=addons
```

## Data and credentials

Lumi stores persistent application data in PostgreSQL and ephemerally caches state in Redis. The repository contains zero bundled secrets, production connection strings, or user tokens. Credentials are supplied solely through environment variables at launch.

The HTTP API binds to `0.0.0.0` on the configured `RPC_PORT`. Sensitive management endpoints (`/action`) require an `RPC_TOKEN` bearer header when configured. Public interaction endpoints (`/api/interactions`) verify Ed25519 signatures cryptographically against `DISCORD_PUBLIC_KEY` before reading request bodies.

Database migrations run idempotently via SeaORM migration models. Snowflake IDs are validated at the edge and stored as native 64-bit signed integers (`BIGINT`). All database indexes lead with `guild_id` to ensure isolation and support database sharding.

## Development

Local compilation and testing can be run inside the Nix developer shell:

```sh
# Enter development shell with compiler, mold linker, and cargo helpers
nix develop "path:."

# Run workspace unit and integration tests
cargo test --workspace

# Run accelerated tests via nextest
cargo nextest run

# Run strict workspace lints
cargo clippy --workspace --all-targets

# Check code formatting
cargo fmt --all --check

# Watch source changes and run the bot locally
cargo watch -x "run --bin lumi"
```

The workspace enforces strict lints configured in the root `Cargo.toml`. `unsafe_code` is forbidden. Calls to `.unwrap()`, `.expect()`, and `CreateEmbed` are denied. Application logs use structured `tracing` spans; raw console output is rejected.

## Upstream projects

- [Serenity](https://github.com/serenity-rs/serenity) Discord gateway and REST client library
- [Poise](https://github.com/kalsmith/poise) application command framework
- [SeaORM](https://www.sea-ql.org/SeaORM/) async relational ORM for Rust
- [Axum](https://github.com/tokio-rs/axum) HTTP server framework
- [Tokio](https://tokio.rs/) async runtime and primitives
