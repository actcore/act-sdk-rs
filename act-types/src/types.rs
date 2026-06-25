use std::collections::HashMap;

use crate::cbor;

// ── LocalizedString ──

/// A localizable text value, matching the WIT `localized-string` variant.
///
/// - `Plain` — a single string in the component's `default-language`.
/// - `Localized` — a map of BCP 47 language tags to text.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
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

use crate::capability::Capabilities;
use crate::constants::*;

// ── Component info (act:component custom section) ──

/// One path × mode entry in a `[std.capabilities."wasi:filesystem"].allow` array.
/// Both fields are required.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FilesystemAllow {
    /// Glob pattern (matches the user-policy `allow` / `deny` shape).
    pub path: String,
    /// Access mode the component requests.
    pub mode: FsMode,
}

/// Filesystem access mode a component declares for a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FsMode {
    /// Read-only.
    Ro,
    /// Read-write.
    Rw,
}

/// One entry in a `[std.capabilities."wasi:http"].allow` array.
///
/// `host` is required (exact match, `*.suffix` wildcard, or `*` for any).
/// Other fields are optional narrowers. Declarations never carry `cidr`,
/// `except_ports`, or `deny` — those are user-policy concerns.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HttpAllow {
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub methods: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ports: Option<Vec<u16>>,
}

/// One entry in a `[std.capabilities."wasi:sockets"].allow` array.
///
/// Exactly one of `host` or `cidr` is required. `ports` is optional: omit it
/// (or set it absent) to declare a ceiling over **any port**; provide a
/// non-empty list to restrict to specific ports. `protocols` defaults to
/// `["tcp", "udp"]` (both).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SocketsAllow {
    /// Exact host, `*.suffix` wildcard, or `*` for any. Mutually
    /// exclusive with `cidr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// CIDR (IPv4 or IPv6). Mutually exclusive with `host`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cidr: Option<String>,
    /// Ports this rule applies to. `None` (omitted) means **any port**.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ports: Option<Vec<u16>>,
    /// Protocols this rule applies to. Defaults to both.
    #[serde(
        default = "default_socket_protocols",
        skip_serializing_if = "is_default_protocols"
    )]
    pub protocols: Vec<SocketProtocol>,
}

fn default_socket_protocols() -> Vec<SocketProtocol> {
    vec![SocketProtocol::Tcp, SocketProtocol::Udp]
}

fn is_default_protocols(v: &[SocketProtocol]) -> bool {
    v == [SocketProtocol::Tcp, SocketProtocol::Udp]
}

/// Raw socket protocol — TCP or UDP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SocketProtocol {
    Tcp,
    Udp,
}

/// Component metadata stored in the `act:component` WASM custom section (CBOR-encoded).
///
/// Used by SDK macros (serialization) and host (deserialization).
/// Also deserializable from `act.toml` manifest via `alias` attributes.
///
/// Extra namespaces (not `std`) are collected into `extra`.
#[non_exhaustive]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ComponentInfo {
    /// Well-known component metadata.
    #[serde(default)]
    pub std: StdComponentInfo,
    /// Extra namespaces (third-party extensions).
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Well-known component metadata under the `std` namespace.
#[non_exhaustive]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StdComponentInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(
        rename = "default-language",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub default_language: Option<String>,
    #[serde(default, skip_serializing_if = "Capabilities::is_empty")]
    pub capabilities: Capabilities,
}

impl ComponentInfo {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            std: StdComponentInfo {
                name: name.into(),
                version: version.into(),
                description: description.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    // Convenience accessors for backward compatibility.
    pub fn name(&self) -> &str {
        &self.std.name
    }
    pub fn version(&self) -> &str {
        &self.std.version
    }
    pub fn description(&self) -> &str {
        &self.std.description
    }
}

/// Mount kind for a `wasi:filesystem` `params.mounts` entry (topology only).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountType {
    /// Bind one host directory to a guest path. Requires `host`.
    #[default]
    Bind,
    /// Expose the platform root(s) at a guest path. `host` is forbidden.
    Root,
}

/// One entry in `params.mounts` of the `wasi:filesystem` capability.
///
/// Pure topology: it makes a host directory *nameable* at a guest path.
/// Authorization (which host paths, at which mode) stays in `constraints`
/// (`FilesystemAllow`); a mount carries no access mode.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FilesystemMount {
    /// Mount kind. Defaults to `bind`.
    #[serde(rename = "type", default)]
    pub kind: MountType,
    /// Guest mount point (POSIX-absolute). Required for `bind`; `root`
    /// defaults to "/" when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest: Option<String>,
    /// Host directory (bind only; `~`-expanded by the host). Required iff
    /// `bind`, forbidden iff `root`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

/// Validate a list of mounts. Rules are independent of the constraint set
/// (cross-checks like the drift lint live in act-build). Returns the first
/// violation as a human-readable string.
pub fn validate_mounts(mounts: &[FilesystemMount]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for (i, m) in mounts.iter().enumerate() {
        let guest = match m.kind {
            MountType::Bind => {
                let g = m
                    .guest
                    .as_deref()
                    .ok_or_else(|| format!("mounts[{i}]: bind mount requires `guest`"))?;
                if m.host.as_deref().is_none_or(str::is_empty) {
                    return Err(format!("mounts[{i}]: bind mount requires `host`"));
                }
                g
            }
            MountType::Root => {
                if m.host.is_some() {
                    return Err(format!("mounts[{i}]: root mount must not set `host`"));
                }
                m.guest.as_deref().unwrap_or("/")
            }
        };
        validate_guest(i, guest)?;
        if !seen.insert(guest.to_string()) {
            return Err(format!("mounts[{i}]: duplicate guest path `{guest}`"));
        }
    }
    Ok(())
}

fn validate_guest(i: usize, guest: &str) -> Result<(), String> {
    if !guest.starts_with('/') {
        return Err(format!(
            "mounts[{i}]: guest `{guest}` must be POSIX-absolute (start with '/')"
        ));
    }
    if guest.contains('\\') || guest.contains(':') {
        return Err(format!(
            "mounts[{i}]: guest `{guest}` must not contain a drive letter or backslash"
        ));
    }
    if guest.split('/').any(|c| c == "." || c == "..") {
        return Err(format!(
            "mounts[{i}]: guest `{guest}` must not contain '.' or '..' components"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod mount_tests {
    use super::validate_mounts;
    use super::{FilesystemMount, MountType};

    fn bind(guest: &str, host: &str) -> FilesystemMount {
        FilesystemMount {
            kind: MountType::Bind,
            guest: Some(guest.into()),
            host: Some(host.into()),
        }
    }

    #[test]
    fn valid_bind_passes() {
        assert!(validate_mounts(&[bind("/ows", "~/.ows")]).is_ok());
    }

    #[test]
    fn bind_without_host_fails() {
        let m = FilesystemMount {
            kind: MountType::Bind,
            guest: Some("/ows".into()),
            host: None,
        };
        assert!(validate_mounts(&[m]).unwrap_err().contains("host"));
    }

    #[test]
    fn root_with_host_fails() {
        let m = FilesystemMount {
            kind: MountType::Root,
            guest: Some("/".into()),
            host: Some("/x".into()),
        };
        assert!(validate_mounts(&[m]).unwrap_err().contains("host"));
    }

    #[test]
    fn relative_guest_fails() {
        assert!(
            validate_mounts(&[bind("ows", "~/.ows")])
                .unwrap_err()
                .contains("absolute")
        );
    }

    #[test]
    fn bind_without_guest_fails() {
        let m = FilesystemMount {
            kind: MountType::Bind,
            guest: None,
            host: Some("~/.ows".into()),
        };
        assert!(validate_mounts(&[m]).unwrap_err().contains("guest"));
    }

    #[test]
    fn drive_letter_guest_fails() {
        assert!(
            validate_mounts(&[bind("/c:/x", "~/.ows")])
                .unwrap_err()
                .contains("drive letter or backslash")
        );
    }

    #[test]
    fn dotdot_guest_fails() {
        assert!(
            validate_mounts(&[bind("/ows/../etc", "~/.ows")])
                .unwrap_err()
                .contains("..")
        );
    }

    #[test]
    fn duplicate_guest_fails() {
        let e = validate_mounts(&[bind("/ows", "~/a"), bind("/ows", "~/b")]).unwrap_err();
        assert!(e.contains("duplicate"));
    }

    #[test]
    fn bind_is_the_default_type_and_round_trips() {
        let m: FilesystemMount =
            serde_json::from_value(serde_json::json!({ "guest": "/ows", "host": "~/.ows" }))
                .unwrap();
        assert_eq!(m.kind, MountType::Bind);
        assert_eq!(m.guest.as_deref(), Some("/ows"));
        assert_eq!(m.host.as_deref(), Some("~/.ows"));

        let v = serde_json::to_value(&m).unwrap();
        // `type` defaults to bind and is omitted only if we don't skip; we DO serialize it.
        assert_eq!(v["type"], "bind");
        assert_eq!(v["guest"], "/ows");
        assert_eq!(v["host"], "~/.ows");
    }

    #[test]
    fn root_parses_with_type_field_and_no_host() {
        let m: FilesystemMount =
            serde_json::from_value(serde_json::json!({ "type": "root", "guest": "/" })).unwrap();
        assert_eq!(m.kind, MountType::Root);
        assert_eq!(m.host, None);
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

    pub fn session_not_found(message: impl Into<String>) -> Self {
        Self::new(ERR_SESSION_NOT_FOUND, message)
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
    use std::collections::BTreeMap;

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
        use crate::CapabilityRequest;
        let mut info = ComponentInfo::new("test", "0.1.0", "test component");
        info.std
            .capabilities
            .0
            .insert("wasi:http".into(), CapabilityRequest::default());
        info.std.capabilities.0.insert(
            "wasi:filesystem".into(),
            CapabilityRequest {
                params: BTreeMap::from([("mount-root".into(), json!("/data"))]),
                ..Default::default()
            },
        );

        let mut buf = Vec::new();
        ciborium::into_writer(&info, &mut buf).unwrap();
        let decoded: ComponentInfo = ciborium::from_reader(&buf[..]).unwrap();

        assert!(decoded.std.capabilities.has("wasi:http"));
        assert!(decoded.std.capabilities.has("wasi:filesystem"));
        assert!(!decoded.std.capabilities.has("wasi:sockets"));
        assert_eq!(decoded.std.capabilities.fs_mount_root(), Some("/data"));
    }

    #[test]
    fn capabilities_empty_roundtrip() {
        let info = ComponentInfo::new("test", "0.1.0", "test");
        let mut buf = Vec::new();
        ciborium::into_writer(&info, &mut buf).unwrap();
        let decoded: ComponentInfo = ciborium::from_reader(&buf[..]).unwrap();
        assert!(decoded.std.capabilities.is_empty());
    }

    #[test]
    fn capabilities_fs_no_params_roundtrip() {
        use crate::CapabilityRequest;
        let mut info = ComponentInfo::new("test", "0.1.0", "test");
        info.std
            .capabilities
            .0
            .insert("wasi:filesystem".into(), CapabilityRequest::default());
        let mut buf = Vec::new();
        ciborium::into_writer(&info, &mut buf).unwrap();
        let decoded: ComponentInfo = ciborium::from_reader(&buf[..]).unwrap();
        assert!(decoded.std.capabilities.has("wasi:filesystem"));
        assert_eq!(decoded.std.capabilities.fs_mount_root(), None);
    }

    #[test]
    fn capabilities_unknown_preserved() {
        use crate::CapabilityRequest;
        let mut info = ComponentInfo::new("test", "0.1.0", "test");
        info.std.capabilities.0.insert(
            "acme:gpu".into(),
            CapabilityRequest {
                constraints: vec![json!({ "cores": 8 })],
                ..Default::default()
            },
        );
        let mut buf = Vec::new();
        ciborium::into_writer(&info, &mut buf).unwrap();
        let decoded: ComponentInfo = ciborium::from_reader(&buf[..]).unwrap();
        assert!(decoded.std.capabilities.has("acme:gpu"));
        assert_eq!(
            decoded
                .std
                .capabilities
                .get("acme:gpu")
                .unwrap()
                .constraints[0]["cores"],
            8
        );
    }

    #[test]
    fn filesystem_cap_with_allow_roundtrips() {
        let toml_input = r#"
[std.capabilities."wasi:filesystem"]
description = "test"

[[std.capabilities."wasi:filesystem".allow]]
path = "/etc/**"
mode = "ro"

[[std.capabilities."wasi:filesystem".allow]]
path = "/tmp/**"
mode = "rw"
"#;
        #[derive(serde::Deserialize)]
        struct Wrap {
            std: Std,
        }
        #[derive(serde::Deserialize)]
        struct Std {
            capabilities: Capabilities,
        }
        let w: Wrap = toml::from_str(toml_input).expect("parses");
        let fs = w
            .std
            .capabilities
            .get("wasi:filesystem")
            .expect("fs declared");
        let allow = fs
            .constraints_as::<crate::FilesystemAllow>()
            .expect("parse");
        assert_eq!(allow.len(), 2);
        assert_eq!(allow[0].path, "/etc/**");
        assert_eq!(allow[1].path, "/tmp/**");
    }

    #[test]
    fn filesystem_cap_requires_path_and_mode_on_each_entry() {
        // Missing `mode` → parse error at constraints_as time (FilesystemAllow requires mode).
        let toml_input = r#"
[std.capabilities."wasi:filesystem"]

[[std.capabilities."wasi:filesystem".allow]]
path = "/tmp/**"
"#;
        #[derive(serde::Deserialize)]
        struct Wrap {
            std: Std,
        }
        #[derive(serde::Deserialize)]
        struct Std {
            capabilities: Capabilities,
        }
        let w: Wrap = toml::from_str(toml_input).expect("toml parses");
        let fs = w
            .std
            .capabilities
            .get("wasi:filesystem")
            .expect("fs declared");
        assert!(
            fs.constraints_as::<FilesystemAllow>().is_err(),
            "missing mode must fail"
        );
    }

    #[test]
    fn http_cap_with_allow_roundtrips() {
        let toml_input = r#"
[std.capabilities."wasi:http"]
description = "Calls OpenAI + GitHub"

[[std.capabilities."wasi:http".allow]]
host = "api.openai.com"
scheme = "https"
methods = ["GET", "POST"]

[[std.capabilities."wasi:http".allow]]
host = "*.github.com"
scheme = "https"
"#;
        #[derive(serde::Deserialize)]
        struct Wrap {
            std: Std,
        }
        #[derive(serde::Deserialize)]
        struct Std {
            capabilities: Capabilities,
        }
        let w: Wrap = toml::from_str(toml_input).expect("parses");
        let http = w.std.capabilities.get("wasi:http").expect("http declared");
        let allow = http.constraints_as::<HttpAllow>().expect("parse");
        assert_eq!(allow.len(), 2);
        assert_eq!(allow[0].host, "api.openai.com");
        assert_eq!(allow[0].scheme.as_deref(), Some("https"));
        assert_eq!(
            allow[0].methods.as_deref(),
            Some(&["GET".to_string(), "POST".to_string()][..])
        );
        assert_eq!(allow[1].host, "*.github.com");
    }

    #[test]
    fn http_cap_requires_host_on_each_entry() {
        // Missing `host` → constraints_as::<HttpAllow> fails.
        let toml_input = r#"
[std.capabilities."wasi:http"]

[[std.capabilities."wasi:http".allow]]
scheme = "https"
"#;
        #[derive(serde::Deserialize)]
        struct Wrap {
            std: Std,
        }
        #[derive(serde::Deserialize)]
        struct Std {
            capabilities: Capabilities,
        }
        let w: Wrap = toml::from_str(toml_input).expect("toml parses");
        let http = w.std.capabilities.get("wasi:http").expect("http declared");
        assert!(
            http.constraints_as::<HttpAllow>().is_err(),
            "missing host must fail"
        );
    }

    #[test]
    fn http_cap_wildcard_host() {
        let toml_input = r#"
[[std.capabilities."wasi:http".allow]]
host = "*"
"#;
        #[derive(serde::Deserialize)]
        struct Wrap {
            std: Std,
        }
        #[derive(serde::Deserialize)]
        struct Std {
            capabilities: Capabilities,
        }
        let w: Wrap = toml::from_str(toml_input).expect("parses");
        let http = w.std.capabilities.get("wasi:http").expect("http declared");
        let allow = http.constraints_as::<HttpAllow>().expect("parse");
        assert_eq!(allow[0].host, "*");
    }

    #[test]
    fn sockets_cap_with_allow_roundtrips() {
        let toml_input = r#"
[std.capabilities."wasi:sockets"]

[[std.capabilities."wasi:sockets".allow]]
host = "vnc.example.com"
ports = [5900]
protocols = ["tcp"]

[[std.capabilities."wasi:sockets".allow]]
cidr = "10.0.0.0/8"
ports = [80, 443]
"#;
        #[derive(serde::Deserialize)]
        struct Wrap {
            std: Std,
        }
        #[derive(serde::Deserialize)]
        struct Std {
            capabilities: Capabilities,
        }
        let w: Wrap = toml::from_str(toml_input).expect("parses");
        let allow = w
            .std
            .capabilities
            .get("wasi:sockets")
            .expect("sockets declared")
            .constraints_as::<crate::SocketsAllow>()
            .expect("parse");
        assert_eq!(allow.len(), 2);
        let b = &allow[1];
        assert_eq!(b.host, None);
        assert_eq!(b.cidr.as_deref(), Some("10.0.0.0/8"));
        assert_eq!(b.ports, Some(vec![80, 443]));
        // `protocols` omitted on the cidr entry → default tcp+udp applies on parse.
        assert_eq!(b.protocols, vec![SocketProtocol::Tcp, SocketProtocol::Udp]);
    }

    #[test]
    fn sockets_cap_has_string() {
        use crate::CapabilityRequest;
        let mut c = Capabilities::default();
        assert!(!c.has(crate::constants::CAP_SOCKETS));
        c.0.insert(
            crate::constants::CAP_SOCKETS.into(),
            CapabilityRequest::default(),
        );
        assert!(c.has(crate::constants::CAP_SOCKETS));
    }

    #[test]
    fn sockets_allow_default_protocols_not_emitted() {
        // Manifest author omitted `protocols`: the default (tcp+udp) is
        // applied on deserialize but MUST NOT leak back out on re-serialize,
        // otherwise host-driven round-trips grow noise.
        let toml_input = r#"
[[allow]]
host = "vnc.example.com"
ports = [5900]
"#;
        #[derive(serde::Serialize, serde::Deserialize)]
        struct W {
            allow: Vec<SocketsAllow>,
        }
        let w: W = toml::from_str(toml_input).unwrap();
        assert_eq!(
            w.allow[0].protocols,
            vec![SocketProtocol::Tcp, SocketProtocol::Udp]
        );

        let re = toml::to_string(&w).unwrap();
        assert!(
            !re.contains("protocols"),
            "default protocols leaked into re-serialized output: {re}"
        );

        // And a second round-trip still parses cleanly.
        let w2: W = toml::from_str(&re).unwrap();
        assert_eq!(
            w2.allow[0].protocols,
            vec![SocketProtocol::Tcp, SocketProtocol::Udp]
        );
    }
}
