//! Uniform capability model (act:core §3.1).
//!
//! One envelope (`CapabilityRequest`) describes every capability class —
//! filesystem, http, sockets, inter-component, semantic, or plugin-provided.
//! The per-class difference lives in `constraints` (a provider-defined,
//! opaque-at-this-layer JSON predicate), not in separate Rust types.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::{LocalizedString, constants::CAP_FILESYSTEM};

/// A provider-defined constraint predicate, opaque at the `act:core` layer.
/// Each capability provider supplies the JSON Schema that validates it.
/// E.g. filesystem `{ "path": "...", "mode": "ro" }`, http `{ "host": "..." }`,
/// a semantic class `{ "database": "staging_*" }`.
pub type Constraint = Value;

/// Uniform capability request — one entry per capability class in the
/// `act:component` `std.capabilities` map.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CapabilityRequest {
    /// Human/LLM-facing rationale (drives the enrollment UI and audit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<LocalizedString>,
    /// Class-specific scalar parameters, e.g. filesystem `mount-root`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, Value>,
    /// The self-declared ceiling (allow-only; deny lives on the host grant).
    /// `allow` is accepted as an alias so existing `act.toml` parses.
    #[serde(default, alias = "allow", skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
}

impl CapabilityRequest {
    /// Parse this request's constraints into a typed constraint schema
    /// (e.g. `FilesystemAllow`). Used by host providers.
    pub fn constraints_as<T: serde::de::DeserializeOwned>(
        &self,
    ) -> Result<Vec<T>, serde_json::Error> {
        self.constraints
            .iter()
            .map(|c| serde_json::from_value::<T>(c.clone()))
            .collect()
    }
}

/// Capability declarations from the `std.capabilities` map in `act:component`.
/// Serializes transparently as a CBOR/JSON map keyed by capability id.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Capabilities(pub BTreeMap<String, CapabilityRequest>);

impl Capabilities {
    /// True if no capabilities are declared.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether a capability id is declared.
    pub fn has(&self, id: &str) -> bool {
        self.0.contains_key(id)
    }

    /// The request for a capability id, if declared.
    pub fn get(&self, id: &str) -> Option<&CapabilityRequest> {
        self.0.get(id)
    }

    /// The `mount-root` param of `wasi:filesystem`, if present.
    pub fn fs_mount_root(&self) -> Option<&str> {
        self.0
            .get(CAP_FILESYSTEM)?
            .params
            .get("mount-root")?
            .as_str()
    }

    /// Iterate over (id, request) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &CapabilityRequest)> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serde_skips_empty_and_aliases_allow() {
        let req = CapabilityRequest {
            constraints: vec![serde_json::json!({ "host": "*" })],
            ..Default::default()
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v, serde_json::json!({ "constraints": [{ "host": "*" }] }));

        // `allow` is accepted on input and lands in `constraints`.
        let from_allow: CapabilityRequest =
            serde_json::from_value(serde_json::json!({ "allow": [{ "host": "x" }] })).unwrap();
        assert_eq!(
            from_allow.constraints,
            vec![serde_json::json!({ "host": "x" })]
        );
    }

    #[test]
    fn constraints_as_parses_typed() {
        use crate::FilesystemAllow;
        let req = CapabilityRequest {
            constraints: vec![serde_json::json!({ "path": "/x/**", "mode": "rw" })],
            ..Default::default()
        };
        let parsed = req.constraints_as::<FilesystemAllow>().unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].path, "/x/**");
    }

    #[test]
    fn description_round_trips_as_bare_string() {
        // act.toml writes `description = "..."` — a bare string must parse into Plain
        // and serialize back to a bare string (not {"Plain": "..."}).
        let req: CapabilityRequest =
            serde_json::from_value(serde_json::json!({ "description": "hello" })).unwrap();
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v, serde_json::json!({ "description": "hello" }));
    }

    #[test]
    fn capabilities_cbor_is_map_keyed_by_id() {
        use crate::cbor;
        let mut caps = Capabilities::default();
        caps.0.insert(
            "wasi:filesystem".into(),
            CapabilityRequest {
                constraints: vec![serde_json::json!({ "path": "/data/**", "mode": "rw" })],
                ..Default::default()
            },
        );

        let bytes = cbor::to_cbor(&caps);
        let back: Capabilities = cbor::from_cbor(&bytes).unwrap();

        assert!(back.has("wasi:filesystem"));
        assert_eq!(
            back.get("wasi:filesystem").unwrap().constraints,
            vec![serde_json::json!({ "path": "/data/**", "mode": "rw" })]
        );
    }
}
