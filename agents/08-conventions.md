# agents/08-conventions.md

## Lints

Workspace lints apply everywhere (`[lints] workspace = true` in every crate).

- `unsafe_code = "forbid"` — no unsafe, ever.
- `unused = "warn"`, `dead_code = "warn"` — warn locally, `RUSTFLAGS=-Dwarnings`
  in CI converts to error. Do NOT flip these to deny in Cargo.toml while
  actively porting code.
- `clippy::correctness = "deny"`, `clippy::suspicious = "deny"`.
- `clippy::unwrap_used = "deny"`, `clippy::expect_used = "deny"`.
- Never add `#[allow(...)]` for a denied lint. Fix the code.

## Error handling

- Every public function returns `Result<T>` using the crate's own error type.
- `anyhow` for application-level error propagation in the bin crate.
- `thiserror` for typed errors in library crates (`db`, `kernel`, `script`).
- Never `.unwrap()` or `.expect()` in non-test code.
- Log errors with `tracing::error!` before returning; don't swallow them.

## Tracing / logging

- `tracing` everywhere; never `println!`, `eprintln!`, or `log::`.
- Span fields: always include `guild_id`, `user_id`, `channel_id` where known.
- Use `tracing::instrument` on async handlers.

## Naming

- Crate dirs are short (`db`, `kernel`, `ui`, `script`).
- Package names are prefixed: `lumi-db`, `lumi-kernel`, etc.
- In `use` statements alias if the prefix is noisy:
  ```rust
  use lumi_db as db;
  use lumi_ui as ui;
  ```
- Module file layout: `mod.rs commands.rs events.rs store.rs views.rs
  config.rs tasks.rs`.

## Comments

No comments unless they explain *why* something non-obvious is done.
One line max. Format: `// reason` not `// This does X because Y.`

## No raw SQL

See `agents/03-db.md`. This applies globally, not just per-module.

## No embeds

See `agents/04-ui.md`. V2 only.

## No todo!()/unimplemented!()

Use a logged stub that returns a user-visible "not yet available" card instead.

## Dependencies

- Never add a dep for a few lines of code.
- Check `crates/*/Cargo.toml` and `[workspace.dependencies]` before adding.
- Run `cargo machete` after any dep change to catch unused ones.
- Run `cargo deny check` to enforce license + advisory policy.
