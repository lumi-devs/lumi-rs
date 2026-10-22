//! Module system: the [`Module`] trait, its config schema model, and a registry.
//!
//! Command wiring lives in the bin (`src/commands`); the lib only carries the
//! metadata and config-schema surface a dashboard needs.

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serenity::all::{Context, FullEvent};

/// Static module metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleMeta {
    /// User-facing display name.
    pub display_name: String,
    /// Emoji icon for the module.
    pub emoji: String,
    /// Summary of module functionality.
    pub description: String,
    /// Semantic version string.
    pub version: String,
}

/// One module config field.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigField {
    /// Config key name.
    pub key: String,
    /// Form field label.
    pub label: String,
    /// Explanation of the setting.
    pub description: String,
    /// Expected data type and UI widget.
    pub kind: ConfigFieldKind,
    /// Default JSON value when unset.
    pub default: Value,
    /// Subfields for nested object arrays.
    #[serde(default)]
    pub subfields: Vec<Self>,
    /// Group header for dashboard layout.
    #[serde(default)]
    pub group: Option<String>,
}

/// Data types supported in module config schemas.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "values", rename_all = "snake_case")]
pub enum ConfigFieldKind {
    /// Text input.
    String,
    /// Numeric integer input.
    Integer,
    /// Toggle boolean input.
    Boolean,
    /// Discord channel select.
    Channel,
    /// Discord role select.
    Role,
    /// Multiple Discord role select.
    RoleArray,
    /// Discord user select.
    User,
    /// Time duration string.
    Duration,
    /// List of structured objects.
    ObjectArray,
    /// List of strings.
    StringArray,
    /// Enumerated choice select.
    Enum(Vec<String>),
}

/// A feature module. Commands are owned and declared directly by the module.
pub trait Module: Send + Sync + 'static {
    /// Unique internal module identifier.
    fn name(&self) -> &'static str;
    /// Metadata describing the module.
    fn metadata(&self) -> ModuleMeta;
    /// Slash and prefix commands declared by this module.
    fn commands(&self) -> Vec<crate::bot::Command> {
        Vec::new()
    }
    /// Schema of guild-configurable settings.
    fn config_schema(&self) -> Vec<ConfigField> {
        Vec::new()
    }
    /// `custom_id` prefix this module owns for component/modal interactions.
    fn component_prefix(&self) -> Option<&'static str> {
        None
    }
    /// Observe one gateway event; default no-op. Boxed so the trait stays object-safe.
    fn on_event<'a>(&'a self, _ctx: &'a Context, _event: &'a FullEvent) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

/// Ordered, name-addressable set of loaded modules.
///
/// Registering a module whose name is already present replaces the earlier
/// one (and logs), so a double registration cannot silently produce duplicate
/// command trees.
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
    /// Create an empty module registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a module implementation.
    pub fn register(&mut self, module: Box<dyn Module>) {
        let name = module.name();
        if let Some(existing) = self.modules.iter_mut().find(|m| m.name() == name) {
            tracing::warn!(module = name, "replacing already-registered module");
            *existing = module;
            return;
        }
        self.modules.push(module);
    }

    /// Look up a module by name.
    pub fn get(&self, name: &str) -> Option<&dyn Module> {
        self.modules
            .iter()
            .find(|module| module.name() == name)
            .map(|module| &**module)
    }

    /// Iterate over registered modules.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Module> {
        self.modules.iter().map(|module| &**module)
    }

    /// Every command supplied by every registered module, in registration order.
    pub fn commands(&self) -> Vec<crate::bot::Command> {
        self.iter().flat_map(Module::commands).collect()
    }

    /// The module owning a component/modal `custom_id`, by registered prefix.
    pub fn module_for_component(&self, custom_id: &str) -> Option<&dyn Module> {
        self.iter().find(|module| {
            module
                .component_prefix()
                .is_some_and(|prefix| custom_id.starts_with(prefix))
        })
    }

    /// Let every module observe one gateway event, in registration order.
    pub async fn dispatch_event(&self, ctx: &Context, event: &FullEvent) {
        for module in self.iter() {
            module.on_event(ctx, event).await;
        }
    }

    /// Number of registered modules.
    pub const fn len(&self) -> usize {
        self.modules.len()
    }

    /// Whether the registry is empty.
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Example(&'static str);

    impl Module for Example {
        fn name(&self) -> &'static str {
            self.0
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
        registry.register(Box::new(Example("example")));
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("example").map(Module::name), Some("example"));
        assert!(registry.get("missing").is_none());
        assert_eq!(registry.iter().count(), 1);
    }

    #[test]
    fn duplicate_registration_replaces() {
        let mut registry = ModuleRegistry::new();
        registry.register(Box::new(Example("dup")));
        registry.register(Box::new(Example("dup")));
        assert_eq!(registry.len(), 1);
    }

    struct PrefixModule;

    impl Module for PrefixModule {
        fn name(&self) -> &'static str {
            "prefixed"
        }

        fn metadata(&self) -> ModuleMeta {
            ModuleMeta {
                display_name: "Prefixed".to_string(),
                emoji: String::new(),
                description: String::new(),
                version: "1.0.0".to_string(),
            }
        }

        fn component_prefix(&self) -> Option<&'static str> {
            Some("p:")
        }
    }

    #[test]
    fn component_prefix_routes_to_module() {
        let mut registry = ModuleRegistry::new();
        registry.register(Box::new(PrefixModule));
        assert_eq!(
            registry.module_for_component("p:start").map(Module::name),
            Some("prefixed")
        );
        assert!(registry.module_for_component("other:start").is_none());
    }
}
