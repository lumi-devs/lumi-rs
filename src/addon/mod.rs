use std::path::Path;

use rhai::{AST, Engine};

use crate::Result;

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
        engine.set_max_operations(10_000_000);
        engine.set_max_call_levels(64);
        engine.set_max_expr_depths(64, 64);
        engine.set_max_string_size(1_000_000);
        engine.set_max_array_size(10_000);
        engine.set_max_map_size(10_000);

        // Host API surface (grows module by module); `log` for now.
        engine.register_fn("log", |message: &str| {
            tracing::info!(target: "lumi::addon", "{message}");
        });

        let mut scripts = Vec::new();
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let path = entry?.path();
                if path.extension().is_some_and(|ext| ext == "rhai") {
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

    pub fn count(&self) -> usize {
        self.scripts.len()
    }
}
