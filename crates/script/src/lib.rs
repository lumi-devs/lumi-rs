use std::net::IpAddr;
use std::path::Path;
use rhai::{AST, Engine};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(#[from] rhai::ParseError),
    #[error("network policy violation: {0}")]
    SsrfBlocked(String),
}

pub type Result<T> = std::result::Result<T, ScriptError>;

/// Validate target IP address against RFC1918, link-local, loopback, and cloud metadata (SSRF).
pub fn is_ip_allowed(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            let octets = ipv4.octets();
            // Loopback (127.0.0.0/8)
            if octets[0] == 127 {
                return false;
            }
            // RFC1918 Private ranges
            if octets[0] == 10 {
                return false;
            }
            if octets[0] == 172 && (16..=31).contains(&octets[1]) {
                return false;
            }
            if octets[0] == 192 && octets[1] == 168 {
                return false;
            }
            // Link-local / AWS metadata (169.254.0.0/16)
            if octets[0] == 169 && octets[1] == 254 {
                return false;
            }
            // Broadcast
            if octets == [255, 255, 255, 255] {
                return false;
            }
            true
        }
        IpAddr::V6(ipv6) => {
            // Loopback ::1
            if ipv6.is_loopback() {
                return false;
            }
            // Unique local address fc00::/7
            let segments = ipv6.segments();
            if (segments[0] & 0xfe00) == 0xfc00 {
                return false;
            }
            // Link-local fe80::/10
            if (segments[0] & 0xffc0) == 0xfe80 {
                return false;
            }
            true
        }
    }
}

/// Loads and sandboxes Rhai addons from a directory. Each `.rhai` file is a
/// compiled unit; the engine enforces deterministic resource limits.
pub struct AddonHost {
    engine: Engine,
    scripts: Vec<AST>,
}

impl std::fmt::Debug for AddonHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddonHost")
            .field("scripts", &self.scripts.len())
            .finish()
    }
}

impl AddonHost {
    pub fn load(dir: &Path) -> Result<Self> {
        let mut engine = Engine::new();
        // Strict runtime limits per AGENTS / spec:
        engine.set_max_operations(50_000);
        engine.set_max_call_levels(32);
        engine.set_max_expr_depths(32, 32);
        engine.set_max_string_size(65_536);
        engine.set_max_array_size(1_024);
        engine.set_max_map_size(1_024);

        // Host API surface: safe structured logging
        engine.register_fn("log_info", |message: &str| {
            tracing::info!(target: "lumi::addon", "{message}");
        });
        engine.register_fn("log_warn", |message: &str| {
            tracing::warn!(target: "lumi::addon", "{message}");
        });

        let mut scripts = Vec::new();
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let path = entry?.path();
                if path.is_dir() {
                    let script_path = path.join("script.rhai");
                    if script_path.exists() {
                        let source = std::fs::read_to_string(&script_path)?;
                        let ast = engine.compile(source)?;
                        scripts.push(ast);
                    }
                } else if path.extension().is_some_and(|ext| ext == "rhai") {
                    let source = std::fs::read_to_string(&path)?;
                    scripts.push(engine.compile(source)?);
                }
            }
        }

        Ok(Self { engine, scripts })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn scripts(&self) -> &[AST] {
        &self.scripts
    }

    pub fn count(&self) -> usize {
        self.scripts.len()
    }

    /// Execute a function across all loaded addon scripts.
    pub fn call_function<T: Clone + Send + Sync + 'static>(&self, fn_name: &str, arg: impl Clone + Send + Sync + 'static) -> Vec<T> {
        let mut results = Vec::new();
        for ast in &self.scripts {
            if let Ok(res) = self.engine.call_fn::<T>(&mut rhai::Scope::new(), ast, fn_name, (arg.clone(),)) {
                results.push(res);
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_load_and_run_ping_addon() {
        let addon_dir = Path::new("../../addons");
        if !addon_dir.exists() {
            return;
        }

        let host = AddonHost::load(addon_dir).expect("failed to load addons");
        assert!(host.count() > 0, "should have loaded at least one addon");

        let responses: Vec<String> = host.call_function("handle_hello", "Alice".to_string());
        assert!(!responses.is_empty(), "expected reply from handle_hello");
        assert!(responses[0].contains("Hello, Alice!"), "unexpected reply: {}", responses[0]);
    }
}
