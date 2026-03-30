//! Read and merge `act.toml` manifest with Cargo.toml fallbacks.

use std::collections::BTreeMap;
use std::path::Path;

/// Deserialized `act.toml` structure.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ActManifest {
    #[serde(default)]
    pub component: Option<ComponentSection>,
    #[serde(default)]
    pub capabilities: BTreeMap<String, toml::Value>,
}

/// The `[component]` section of `act.toml`.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ComponentSection {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "default-language")]
    pub default_language: Option<String>,
}

/// Read `act.toml` from the given path. Returns `None` if the file doesn't exist.
pub fn read_manifest(path: &Path) -> Result<Option<ActManifest>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let manifest: ActManifest =
        toml::from_str(&content).map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
    Ok(Some(manifest))
}

/// Attribute overrides from `#[act_component(...)]`.
pub struct Overrides {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub default_language: Option<String>,
}

/// Build `ComponentInfo` by merging: attribute overrides > act.toml > Cargo.toml env vars.
pub fn build_component_info(
    manifest: Option<ActManifest>,
    overrides: Overrides,
) -> act_types::ComponentInfo {
    let comp = manifest.as_ref().and_then(|m| m.component.as_ref());

    // Resolution: override > manifest > Cargo.toml
    let name = overrides
        .name
        .or_else(|| comp.and_then(|c| c.name.clone()))
        .unwrap_or_else(|| std::env::var("CARGO_PKG_NAME").unwrap_or_default());
    let version = overrides
        .version
        .or_else(|| comp.and_then(|c| c.version.clone()))
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default());
    let description = overrides
        .description
        .or_else(|| comp.and_then(|c| c.description.clone()))
        .unwrap_or_else(|| std::env::var("CARGO_PKG_DESCRIPTION").unwrap_or_default());
    let default_language = overrides
        .default_language
        .or_else(|| comp.and_then(|c| c.default_language.clone()))
        .or_else(|| Some("en".to_string()));

    let mut info = act_types::ComponentInfo::new(name, version, description);
    info.default_language = default_language;

    // Parse capabilities from manifest
    if let Some(manifest) = manifest {
        info.capabilities = parse_capabilities(manifest.capabilities);
    }

    info
}

/// Convert TOML capabilities map into the typed `Capabilities` struct.
fn parse_capabilities(caps: BTreeMap<String, toml::Value>) -> act_types::Capabilities {
    use act_types::constants::*;

    let mut result = act_types::Capabilities::default();
    for (key, value) in caps {
        match key.as_str() {
            CAP_HTTP => {
                result.http = Some(
                    value
                        .try_into()
                        .unwrap_or_else(|e| panic!("invalid wasi:http params: {e}")),
                );
            }
            CAP_FILESYSTEM => {
                result.filesystem = Some(
                    value
                        .try_into()
                        .unwrap_or_else(|e| panic!("invalid wasi:filesystem params: {e}")),
                );
            }
            CAP_SOCKETS => {
                result.sockets = Some(
                    value
                        .try_into()
                        .unwrap_or_else(|e| panic!("invalid wasi:sockets params: {e}")),
                );
            }
            _ => {
                let json = toml_to_json(value);
                result.other.insert(key, json);
            }
        }
    }
    result
}

/// Convert a `toml::Value` to a `serde_json::Value`.
fn toml_to_json(val: toml::Value) -> serde_json::Value {
    match val {
        toml::Value::String(s) => serde_json::Value::String(s),
        toml::Value::Integer(i) => serde_json::json!(i),
        toml::Value::Float(f) => serde_json::json!(f),
        toml::Value::Boolean(b) => serde_json::Value::Bool(b),
        toml::Value::Datetime(d) => serde_json::Value::String(d.to_string()),
        toml::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(toml_to_json).collect())
        }
        toml::Value::Table(tbl) => {
            let map = tbl.into_iter().map(|(k, v)| (k, toml_to_json(v))).collect();
            serde_json::Value::Object(map)
        }
    }
}
