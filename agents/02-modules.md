# agents/02-modules.md

## Where modules live

All feature modules live in `crates/modules/src/<name>/`:

```
crates/modules/src/<name>/
  mod.rs       — Module impl (metadata, config schema, wiring)
  commands.rs  — poise commands
  events.rs    — serenity event handlers
  store.rs     — DB access (via lumi-db helpers, NO raw SQL)
  views.rs     — V2 UI builders (via lumi-ui)
  config.rs    — per-guild config struct + serde
  tasks.rs     — apalis / timer jobs
```

## Module trait (lumi-kernel)

```rust
pub trait Module: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn commands(&self) -> Vec<poise::Command<Data, Error>>;
    fn on_event(&self, ctx: &Context, event: &FullEvent, data: &Data)
        -> impl Future<Output = Result<()>> + Send;
}
```

## Registering a module

In `crates/modules/src/lib.rs`:
```rust
pub fn all() -> Vec<Box<dyn Module>> {
    vec![
        Box::new(afk::Afk),
        Box::new(logging::Logging),
        // add here
    ]
}
```

The bin (`crates/lumi`) aggregates commands and dispatches events through
`lumi_modules::all()`. Adding a module = one folder + one entry in `all()`.
Never touch the bin's main.rs for a new module.

## Toggles

Each module can be toggled per-guild. Use `arc-swap` over the active module set
(stored in `lumi-kernel`'s registry) so hot-swaps don't require a lock convoy.
