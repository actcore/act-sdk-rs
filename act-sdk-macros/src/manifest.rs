//! Read `act.toml` manifest and merge with attribute overrides and Cargo.toml fallbacks.
//!
//! `act.toml` deserializes directly into [`act_types::ComponentInfo`] (nested `[std]` table).

use std::path::Path;

/// Read `act.toml` from the given path. Returns `None` if the file doesn't exist.
pub fn read_manifest(path: &Path) -> Result<Option<act_types::ComponentInfo>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let info: act_types::ComponentInfo =
        toml::from_str(&content).map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
    Ok(Some(info))
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
    manifest: Option<act_types::ComponentInfo>,
    overrides: Overrides,
) -> act_types::ComponentInfo {
    let m = manifest.unwrap_or_default();
    let s = m.std;

    let name = overrides
        .name
        .or_else(|| non_empty(s.name.clone()))
        .unwrap_or_else(|| std::env::var("CARGO_PKG_NAME").unwrap_or_default());
    let version = overrides
        .version
        .or_else(|| non_empty(s.version.clone()))
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default());
    let description = overrides
        .description
        .or_else(|| non_empty(s.description.clone()))
        .unwrap_or_else(|| std::env::var("CARGO_PKG_DESCRIPTION").unwrap_or_default());
    let default_language = overrides
        .default_language
        .or(s.default_language)
        .or_else(|| Some("en".to_string()));

    let mut info = act_types::ComponentInfo::new(name, version, description);
    info.std.default_language = default_language;
    info.std.capabilities = s.capabilities;
    info.extra = m.extra;
    info
}

/// Return `None` for empty strings (so Cargo.toml fallback kicks in).
fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}
