use std::collections::HashMap;

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

    /// Convert to a JSON object. Returns `None` if empty.
    pub fn to_json(&self) -> Option<serde_json::Value> {
        if self.0.is_empty() {
            return None;
        }
        let map: serde_json::Map<String, serde_json::Value> =
            self.0.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        Some(serde_json::Value::Object(map))
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

// ── CBOR byte wrappers ──

/// Shared implementation for CBOR byte wrapper types.
macro_rules! cbor_wrapper {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        pub struct $name(Vec<u8>);

        impl $name {
            /// Encode a JSON value to CBOR.
            pub fn from_json(value: &serde_json::Value) -> Result<Self, cbor::CborError> {
                cbor::json_to_cbor(value).map(Self)
            }

            /// Convert from an optional JSON value.
            pub fn from_json_opt(value: &Option<serde_json::Value>) -> Result<Option<Self>, cbor::CborError> {
                match value {
                    Some(val) => Self::from_json(val).map(Some),
                    None => Ok(None),
                }
            }

            /// Get the raw CBOR bytes.
            pub fn as_bytes(&self) -> &[u8] {
                &self.0
            }

            /// Decode to a JSON value.
            pub fn to_json(&self) -> Result<serde_json::Value, cbor::CborError> {
                cbor::cbor_to_json(&self.0)
            }

            /// Deserialize into a typed value.
            pub fn deserialize<T: serde::de::DeserializeOwned>(&self) -> Result<T, cbor::CborError> {
                cbor::from_cbor(&self.0)
            }
        }

        impl From<Vec<u8>> for $name {
            fn from(v: Vec<u8>) -> Self {
                Self(v)
            }
        }

        impl From<$name> for Vec<u8> {
            fn from(w: $name) -> Self {
                w.0
            }
        }
    };
}

cbor_wrapper!(
    /// Tool arguments as CBOR bytes.
    Args
);

cbor_wrapper!(
    /// Component configuration as CBOR bytes.
    Config
);

use crate::constants::*;

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
        assert!(Metadata::new().to_json().is_none());
    }

    #[test]
    fn metadata_to_json_with_values() {
        let mut m = Metadata::new();
        m.insert("std:read-only", true);
        let json = m.to_json().unwrap();
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
    fn args_from_json_roundtrip() {
        let val = json!({"code": "2+2"});
        let args = Args::from_json(&val).unwrap();
        let decoded = args.to_json().unwrap();
        assert_eq!(val, decoded);
    }

    #[test]
    fn args_deserialize_typed() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct Params {
            code: String,
        }
        let val = json!({"code": "hello"});
        let args = Args::from_json(&val).unwrap();
        let params: Params = args.deserialize().unwrap();
        assert_eq!(params.code, "hello");
    }

    #[test]
    fn config_from_json_opt_none() {
        assert!(Config::from_json_opt(&None).unwrap().is_none());
    }

    #[test]
    fn config_from_json_opt_some() {
        let val = json!({"key": "value"});
        let config = Config::from_json_opt(&Some(val.clone())).unwrap().unwrap();
        let decoded = config.to_json().unwrap();
        assert_eq!(decoded, val);
    }
}
