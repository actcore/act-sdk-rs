use std::collections::{BTreeMap, HashMap};

use crate::cbor;

// ── LocalizedString ──

/// A localizable text value, matching the WIT `localized-string` variant.
///
/// - `Plain` — a single string in the component's `default-language`.
/// - `Localized` — a map of BCP 47 language tags to text.
#[derive(Debug, Clone)]
pub enum LocalizedString {
    /// A single string assumed to be in the component's `default-language`.
    Plain(String),
    /// Language tag → text map. MUST include the component's `default-language`.
    Localized(HashMap<String, String>),
}

impl Default for LocalizedString {
    fn default() -> Self {
        Self::Plain(String::new())
    }
}

impl LocalizedString {
    /// Create a plain (non-localized) string.
    pub fn plain(text: impl Into<String>) -> Self {
        Self::Plain(text.into())
    }

    /// Create a localized string with a single language entry.
    pub fn new(lang: impl Into<String>, text: impl Into<String>) -> Self {
        let mut map = HashMap::new();
        map.insert(lang.into(), text.into());
        Self::Localized(map)
    }

    /// Look up text for a specific language tag.
    ///
    /// For `Plain`, always returns the text (it is assumed to match any language).
    /// For `Localized`, performs exact key lookup.
    pub fn get(&self, lang: &str) -> Option<&str> {
        match self {
            Self::Plain(text) => Some(text.as_str()),
            Self::Localized(map) => map.get(lang).map(|s| s.as_str()),
        }
    }

    /// Resolve to text for the given language, with fallback chain.
    ///
    /// - `Plain` → returns the plain string (assumed to be in `default_language`).
    /// - `Localized` → exact match → prefix match → any entry.
    pub fn resolve(&self, lang: &str) -> &str {
        match self {
            Self::Plain(text) => text.as_str(),
            Self::Localized(map) => {
                // 1. Exact match
                if let Some(text) = map.get(lang) {
                    return text.as_str();
                }
                // 2. Prefix match (e.g. "zh" matches "zh-Hans")
                if let Some(text) = map
                    .iter()
                    .find(|(tag, _)| tag.starts_with(lang) || lang.starts_with(tag.as_str()))
                    .map(|(_, text)| text.as_str())
                {
                    return text;
                }
                // 3. Any entry
                map.values().next().map(|s| s.as_str()).unwrap_or("")
            }
        }
    }

    /// Get some text, regardless of language.
    /// Useful when you don't have the default language available.
    pub fn any_text(&self) -> &str {
        match self {
            Self::Plain(text) => text.as_str(),
            Self::Localized(map) => map.values().next().map(|s| s.as_str()).unwrap_or(""),
        }
    }
}

impl From<String> for LocalizedString {
    fn from(s: String) -> Self {
        Self::Plain(s)
    }
}

impl From<&str> for LocalizedString {
    fn from(s: &str) -> Self {
        Self::Plain(s.to_string())
    }
}

impl From<Vec<(String, String)>> for LocalizedString {
    fn from(v: Vec<(String, String)>) -> Self {
        Self::Localized(v.into_iter().collect())
    }
}

impl From<HashMap<String, String>> for LocalizedString {
    fn from(map: HashMap<String, String>) -> Self {
        Self::Localized(map)
    }
}

// ── Metadata ──

/// Key → value metadata, stored as JSON values internally.
///
/// Converts to/from WIT `list<tuple<string, list<u8>>>` (CBOR) at the boundary.
#[derive(Debug, Clone, Default)]
pub struct Metadata(HashMap<String, serde_json::Value>);

impl Metadata {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Insert a value. Overwrites any existing entry for the key.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) {
        self.0.insert(key.into(), value.into());
    }

    /// Get a value by key.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.0.get(key)
    }

    /// Get a value by key, deserializing into a typed value.
    pub fn get_as<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.0
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Check if a key exists.
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// Returns true if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &serde_json::Value)> {
        self.0.iter()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Merge all entries from `other` into `self`. Entries in `other` overwrite existing keys.
    pub fn extend(&mut self, other: Metadata) {
        self.0.extend(other.0);
    }
}

/// Convert from a JSON object value. Non-object values produce empty metadata.
impl From<serde_json::Value> for Metadata {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Object(map) => Self(map.into_iter().collect()),
            _ => Self::new(),
        }
    }
}

/// Convert to a JSON object value (consuming).
impl From<Metadata> for serde_json::Value {
    fn from(m: Metadata) -> Self {
        serde_json::Value::Object(m.0.into_iter().collect())
    }
}

/// Convert from WIT metadata (CBOR-encoded values).
impl From<Vec<(String, Vec<u8>)>> for Metadata {
    fn from(v: Vec<(String, Vec<u8>)>) -> Self {
        Self(
            v.into_iter()
                .filter_map(|(k, cbor_bytes)| {
                    let val = cbor::cbor_to_json(&cbor_bytes).ok()?;
                    Some((k, val))
                })
                .collect(),
        )
    }
}

/// Convert to WIT metadata (CBOR-encoded values).
impl From<Metadata> for Vec<(String, Vec<u8>)> {
    fn from(m: Metadata) -> Self {
        m.0.into_iter()
            .map(|(k, v)| (k, cbor::to_cbor(&v)))
            .collect()
    }
}

use crate::constants::*;

// ── Component info (act:component custom section) ──

/// Parameters for the `wasi:filesystem` capability.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FilesystemCap {
    /// Internal WASM root path for all host mounts (default: `/`).
    #[serde(
        rename = "mount-root",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub mount_root: Option<String>,
}

/// Parameters for the `wasi:http` capability.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct HttpCap {}

/// Parameters for the `wasi:sockets` capability.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct SocketsCap {}

/// Capability declarations from the `std:capabilities` map in `act:component`.
///
/// Well-known capabilities have typed fields. Unknown third-party capabilities
/// are collected in `other`. Serializes as a CBOR/JSON map keyed by capability ID.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Capabilities {
    /// `wasi:filesystem` — filesystem access.
    #[serde(rename = "wasi:filesystem")]
    pub filesystem: Option<FilesystemCap>,
    /// `wasi:http` — outbound HTTP requests.
    #[serde(rename = "wasi:http")]
    pub http: Option<HttpCap>,
    /// `wasi:sockets` — outbound TCP/UDP connections.
    #[serde(rename = "wasi:sockets")]
    pub sockets: Option<SocketsCap>,
    /// Third-party capabilities keyed by identifier.
    #[serde(flatten)]
    pub other: BTreeMap<String, serde_json::Value>,
}

impl Capabilities {
    /// True if no capabilities are declared.
    pub fn is_empty(&self) -> bool {
        self.http.is_none()
            && self.filesystem.is_none()
            && self.sockets.is_none()
            && self.other.is_empty()
    }

    /// Check if a capability is declared by its string identifier.
    pub fn has(&self, id: &str) -> bool {
        match id {
            CAP_HTTP => self.http.is_some(),
            CAP_FILESYSTEM => self.filesystem.is_some(),
            CAP_SOCKETS => self.sockets.is_some(),
            other => self.other.contains_key(other),
        }
    }

    /// Get the `mount-root` parameter from the `wasi:filesystem` capability.
    pub fn fs_mount_root(&self) -> Option<&str> {
        self.filesystem.as_ref()?.mount_root.as_deref()
    }
}

/// Component metadata stored in the `act:component` WASM custom section (CBOR-encoded).
///
/// Used by SDK macros (serialization) and host (deserialization).
/// Also deserializable from `act.toml` manifest via `alias` attributes.
///
/// Standard WASM metadata fields (`version`, `description`) may also be read
/// from their respective custom sections as fallback.
///
/// Extra keys (not matching `std:*` fields) are collected into `metadata`.
#[non_exhaustive]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ComponentInfo {
    #[serde(rename = "std:name", alias = "name", default)]
    pub name: String,
    #[serde(rename = "std:version", alias = "version", default)]
    pub version: String,
    #[serde(rename = "std:description", alias = "description", default)]
    pub description: String,
    #[serde(
        rename = "std:default-language",
        alias = "default-language",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub default_language: Option<String>,
    #[serde(
        rename = "std:capabilities",
        alias = "capabilities",
        default,
        skip_serializing_if = "Capabilities::is_empty"
    )]
    pub capabilities: Capabilities,
    /// Extra metadata keys not matching well-known `std:*` fields.
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ComponentInfo {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: description.into(),
            ..Default::default()
        }
    }
}

// ── Error type ──

/// Error type mapping to ACT `tool-error`.
#[derive(Debug, Clone)]
pub struct ActError {
    pub kind: String,
    pub message: String,
}

impl ActError {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ERR_NOT_FOUND, message)
    }

    pub fn invalid_args(message: impl Into<String>) -> Self {
        Self::new(ERR_INVALID_ARGS, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ERR_INTERNAL, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ERR_TIMEOUT, message)
    }

    pub fn capability_denied(message: impl Into<String>) -> Self {
        Self::new(ERR_CAPABILITY_DENIED, message)
    }
}

impl std::fmt::Display for ActError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ActError {}

/// Result type for ACT operations.
pub type ActResult<T> = Result<T, ActError>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn localized_string_plain() {
        let ls = LocalizedString::plain("hello");
        assert_eq!(ls.resolve("en"), "hello");
        assert_eq!(ls.any_text(), "hello");
    }

    #[test]
    fn localized_string_from_str() {
        let ls = LocalizedString::from("hello");
        assert_eq!(ls.any_text(), "hello");
    }

    #[test]
    fn localized_string_default() {
        let ls = LocalizedString::default();
        assert_eq!(ls.any_text(), "");
    }

    #[test]
    fn localized_string_resolve_by_lang() {
        let mut map = std::collections::HashMap::new();
        map.insert("en".to_string(), "hello".to_string());
        map.insert("ru".to_string(), "привет".to_string());
        let ls = LocalizedString::Localized(map);
        assert_eq!(ls.resolve("ru"), "привет");
        assert_eq!(ls.resolve("en"), "hello");
        // Unknown lang falls back to some entry
        assert!(!ls.resolve("fr").is_empty());
    }

    #[test]
    fn localized_string_resolve_prefix() {
        let mut map = HashMap::new();
        map.insert("zh-Hans".to_string(), "你好".to_string());
        map.insert("en".to_string(), "hello".to_string());
        let ls = LocalizedString::Localized(map);
        assert_eq!(ls.resolve("zh"), "你好");
    }

    #[test]
    fn localized_string_get() {
        let ls = LocalizedString::new("en", "hello");
        assert_eq!(ls.get("en"), Some("hello"));
        assert_eq!(ls.get("ru"), None);
    }

    #[test]
    fn localized_string_from_vec() {
        let v = vec![("en".to_string(), "hi".to_string())];
        let ls = LocalizedString::from(v);
        assert_eq!(ls.resolve("en"), "hi");
    }

    #[test]
    fn metadata_insert_and_get() {
        let mut m = Metadata::new();
        m.insert("std:read-only", true);
        assert_eq!(m.get("std:read-only"), Some(&json!(true)));
        assert_eq!(m.get_as::<bool>("std:read-only"), Some(true));
    }

    #[test]
    fn metadata_to_json_empty() {
        let json: serde_json::Value = Metadata::new().into();
        assert_eq!(json, json!({}));
    }

    #[test]
    fn metadata_to_json_with_values() {
        let mut m = Metadata::new();
        m.insert("std:read-only", true);
        let json: serde_json::Value = m.into();
        assert_eq!(json["std:read-only"], json!(true));
    }

    #[test]
    fn metadata_from_vec() {
        let v = vec![("key".to_string(), cbor::to_cbor(&42u32))];
        let m = Metadata::from(v);
        assert_eq!(m.get("key"), Some(&json!(42)));
        assert_eq!(m.get_as::<u32>("key"), Some(42));
    }

    #[test]
    fn capabilities_cbor_roundtrip() {
        let mut info = ComponentInfo::new("test", "0.1.0", "test component");
        info.capabilities.http = Some(HttpCap {});
        info.capabilities.filesystem = Some(FilesystemCap {
            mount_root: Some("/data".to_string()),
        });

        let mut buf = Vec::new();
        ciborium::into_writer(&info, &mut buf).unwrap();

        let decoded: ComponentInfo = ciborium::from_reader(&buf[..]).unwrap();
        assert!(decoded.capabilities.http.is_some());
        assert!(decoded.capabilities.filesystem.is_some());
        assert!(decoded.capabilities.sockets.is_none());
        assert_eq!(decoded.capabilities.fs_mount_root(), Some("/data"));
    }

    #[test]
    fn capabilities_empty_roundtrip() {
        let info = ComponentInfo::new("test", "0.1.0", "test");

        let mut buf = Vec::new();
        ciborium::into_writer(&info, &mut buf).unwrap();

        let decoded: ComponentInfo = ciborium::from_reader(&buf[..]).unwrap();
        assert!(decoded.capabilities.is_empty());
    }

    #[test]
    fn capabilities_fs_no_params_roundtrip() {
        let mut info = ComponentInfo::new("test", "0.1.0", "test");
        info.capabilities.filesystem = Some(FilesystemCap::default());

        let mut buf = Vec::new();
        ciborium::into_writer(&info, &mut buf).unwrap();

        let decoded: ComponentInfo = ciborium::from_reader(&buf[..]).unwrap();
        assert!(decoded.capabilities.filesystem.is_some());
        assert_eq!(decoded.capabilities.fs_mount_root(), None);
    }

    #[test]
    fn capabilities_unknown_preserved() {
        let mut info = ComponentInfo::new("test", "0.1.0", "test");
        info.capabilities
            .other
            .insert("acme:gpu".to_string(), json!({"cores": 8}));

        let mut buf = Vec::new();
        ciborium::into_writer(&info, &mut buf).unwrap();

        let decoded: ComponentInfo = ciborium::from_reader(&buf[..]).unwrap();
        assert!(decoded.capabilities.has("acme:gpu"));
        assert_eq!(decoded.capabilities.other["acme:gpu"]["cores"], 8);
    }
}
