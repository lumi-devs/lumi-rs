# agents/07-build.md

## Colab (primary heavy build)

The repo is public at https://github.com/lumi-devs/lumi-rs.
Compilation runs in a Colab notebook via the colab-mcp tool.

Workflow (agent):
1. Commit + push the branch.
2. Update the Colab cell (`TXz4eXAz0JhP`) to clone the branch and run
   `cargo check --workspace` or `cargo clippy --workspace`.
3. Call `colab_mcp_run_code_cell` — it will time out (MCP limit) but keeps
   running in Colab.
4. Wait ~5 min, then call `colab_mcp_get_cells` with `includeOutputs: true`
   to read results.

Standard Colab cell template:
```bash
%%bash
set -e
source "$HOME/.cargo/env"
cd /content/lumi-rs && git fetch && git checkout <branch> && git pull
cargo check --workspace 2>&1 | tail -60
```

## Local (nix flake)

```bash
nix develop "path:/home/rebiz/opt/lumi-rs" --command cargo clippy --workspace
nix develop "path:/home/rebiz/opt/lumi-rs" --command cargo test --workspace
```

`path:` prefix is required — the flake lives in a dirty git work tree.
Never run bare `cargo` locally (wrong toolchain version risk).

## Profiles

| Profile | Use |
|---------|-----|
| `dev` | local iteration (`debug = "line-tables-only"`, deps at opt-level 1) |
| `release` | prod (`lto = "thin"`, `strip = true`, `codegen-units = 1`) |
| `ci` | quick smoke build (`lto = "off"`, inherits release) |

## Flake devShell & Fast Development Loop

The Nix flake (`flake.nix`) supplies:
- **`mold` & `clang`**: High-performance linker (`RUSTFLAGS="-C link-arg=-fuse-ld=mold"`), slashing incremental link times from seconds to sub-second.
- **`cargo-watch`**: Live hot-reloading for gateway development:
  ```bash
  nix develop "path:." --command cargo watch -x "run --bin lumi"
  ```
  Watches `.rs` files, terminates the running bot gracefully, recompiles changes in ~1-3s, and reconnects to the Discord gateway.
- **`cargo-nextest`**: Concurrent, optimized test runner:
  ```bash
  nix develop "path:." --command cargo nextest run
  ```

## Git deps — pin by rev

Replace `branch = "..."` with `rev = "<sha>"` for serenity and poise so a
branch-force-push can't silently change the build.

## Nix build (todo)

Switch from `buildRustPackage` to **crane** + **fenix**:
- crane builds deps as a separate cached derivation (huge win for this dep tree).
- fenix pins the toolchain to `rust-version = "1.95"`.
- `cargoLock.outputHashes` must be filled for the git deps (serenity/poise).
