# agents/05-script.md

## Rhai addon host (crates/script)

Addons are user-supplied Rhai scripts that run per-event. The host:
- Runs scripts on a dedicated thread pool (never on tokio workers).
- Sets hard limits before any script executes:
  ```rust
  engine.set_max_operations(50_000);
  engine.set_max_call_levels(32);
  engine.set_max_string_size(65_536);
  engine.set_max_array_size(1_024);
  engine.set_max_map_size(1_024);
  ```
- Times out via a watchdog that calls `engine.set_max_operations` to 0 after
  the deadline.

## Capability gating

Each addon declares required capabilities in its manifest. The host checks
capabilities before loading the script. Capabilities:

| Name | What it allows |
|------|---------------|
| `net` | HTTP requests to external URLs |
| `db`  | Read/write to the addon KV store |
| `msg` | Send messages to channels |

`net` capability MUST block private/loopback IP ranges (SSRF protection).
Implement the check before the HTTP call, not after DNS resolution.

## SSRF block list

Before allowing any outbound request from a `net`-capable addon, reject:
- `127.0.0.0/8`, `::1`
- `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`
- `169.254.0.0/16` (link-local / AWS metadata)
- `fd00::/8` (ULA)
- Any hostname that resolves to the above after DNS.

## API surface

Exposed to scripts via `lumi_script::api`. Keep it minimal and versioned.
Never expose raw DB access — proxy through the capability model.
