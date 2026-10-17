//! Module system: the [`Module`] trait, its config schema model, and a registry.
//!
//! Command wiring lives in the bin (`src/commands`); the lib only carries the
//! metadata and config-schema surface a dashboard needs.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Static module metadata (mirrors lumi's `ModuleMeta` subset we render).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleMeta {
    pub display_name: String,
    pub emoji: String,
    pub description: String,
    pub version: String,
}

/// One module config field (mirrors `ConfigSchema.ts`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    pub description: String,
    pub kind: ConfigFieldKind,
    pub default: Value,
    #[serde(default)]
    pub subfields: Vec<ConfigField>,
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "values", rename_all = "snake_case")]
pub enum ConfigFieldKind {
    String,
    Integer,
    Boolean,
    Channel,
    Role,
    User,
    Duration,
    ObjectArray,
    StringArray,
    Enum(Vec<String>),
}

/// A feature module. Hooks (load/unload, commands) are owned by the bin.
pub trait Module: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn metadata(&self) -> ModuleMeta;
    /// Config fields exposed to the dashboard; empty by default.
    fn config_schema(&self) -> Vec<ConfigField> {
        Vec::new()
    }
}

/// Ordered, name-addressable set of loaded modules.
#[derive(Default)]
pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}

impl std::fmt::Debug for ModuleRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleRegistry")
            .field("names", &self.iter().map(Module::name).collect::<Vec<_>>())
            .finish()
    }
}

impl ModuleRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, module: Box<dyn Module>) {
        self.modules.push(module);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Module> {
        self.modules
            .iter()
            .find(|module| module.name() == name)
            .map(|module| &**module)
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn Module> {
        self.modules.iter().map(|module| &**module)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Example;

    impl Module for Example {
        fn name(&self) -> &'static str {
            "example"
        }

        fn metadata(&self) -> ModuleMeta {
            ModuleMeta {
                display_name: "Example".to_string(),
                emoji: "\u{2699}".to_string(),
                description: "test".to_string(),
                version: "1.0.0".to_string(),
            }
        }
    }

    #[test]
    fn registry_register_get_iter() {
        let mut registry = ModuleRegistry::new();
        assert!(registry.is_empty());
        registry.register(Box::new(Example));
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("example").map(Module::name), Some("example"));
        assert!(registry.get("missing").is_none());
        assert_eq!(registry.iter().count(), 1);
    }
}
