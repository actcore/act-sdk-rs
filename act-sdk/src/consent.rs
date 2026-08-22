//! Helpers for a semantic authorization call site.
//!
//! Deliberately pure: no I/O, and nothing here touches the WIT bindings.
//! `#[act_component]` expands `wit_bindgen::generate!` in the component's own
//! crate, so the generated `act::consent::consent_authority::request` exists
//! only there and this crate cannot name it. `act:credentials` settled the
//! same question a release ago — the component makes the call, the SDK
//! decodes and encodes around it.
//!
//! A call site therefore reads:
//!
//! ```ignore
//! let req = ConsentRequest {
//!     class: "db:drop".into(),
//!     key: database.clone(),
//!     summary: format!("Drop database \"{database}\""),
//!     args: Vec::new(),
//! };
//! let decision = act::consent::consent_authority::request(&req, meta).await;
//! act_sdk::consent::check(matches!(decision, Decision::Allow), "db:drop", &database)?;
//! ```
//!
//! `check` takes a `bool` rather than the WIT `decision` for the same reason
//! `Secret::from_wit` takes a `String` kind: the enum is a per-component
//! generated type this crate cannot name.

use serde::Serialize;

/// A refusal. Convert it into a tool error carrying `std:capability-denied`
/// (`ACT-CONSTANTS.md` §9) so the agent learns the action was refused rather
/// than that it failed.
#[derive(Debug, Clone)]
pub struct Denied {
    class: String,
    key: String,
}

impl Denied {
    pub fn class(&self) -> &str {
        &self.class
    }
    pub fn key(&self) -> &str {
        &self.key
    }
}

impl std::fmt::Display for Denied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.key.is_empty() {
            write!(f, "{} was not authorized", self.class)
        } else {
            write!(f, "{} on {:?} was not authorized", self.class, self.key)
        }
    }
}

impl std::error::Error for Denied {}

/// Turn a decision into a `Result` so a refusal short-circuits with `?`.
pub fn check(allowed: bool, class: &str, key: &str) -> Result<(), Denied> {
    if allowed {
        Ok(())
    } else {
        Err(Denied {
            class: class.to_string(),
            key: key.to_string(),
        })
    }
}

/// Why `args` could not be built.
#[derive(Debug)]
pub enum ArgsError {
    /// The value did not encode as a CBOR map. Only a map carries dimensions.
    NotAMap,
    Encode(String),
}

impl std::fmt::Display for ArgsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgsError::NotAMap => write!(f, "consent args must encode as a CBOR map"),
            ArgsError::Encode(e) => write!(f, "failed to encode consent args as CBOR: {e}"),
        }
    }
}

impl std::error::Error for ArgsError {}

/// Encode extra constraint dimensions as the `args` field's CBOR map.
///
/// Usually unnecessary — `key` carries most classes, and an empty `args` is
/// the common case. Note that only `key` is host-resolved: a dimension here is
/// one the component itself supplies, so an operator's `allow` over it is
/// advisory. See `ACT-CONSENT.md` §8.6.
pub fn args(dimensions: impl Serialize) -> Result<Vec<u8>, ArgsError> {
    let mut buf = Vec::new();
    ciborium::into_writer(&dimensions, &mut buf).map_err(|e| ArgsError::Encode(e.to_string()))?;
    let parsed: ciborium::value::Value =
        ciborium::from_reader(buf.as_slice()).map_err(|e| ArgsError::Encode(e.to_string()))?;
    if !parsed.is_map() {
        return Err(ArgsError::NotAMap);
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_carries_the_class_and_key_it_refused() {
        let e = check(false, "db:drop", "analytics").unwrap_err();
        // The agent is told what was refused, because it may be able to proceed
        // differently — and the operator's audit line uses the same two values.
        assert_eq!(e.class(), "db:drop");
        assert_eq!(e.key(), "analytics");
    }

    #[test]
    fn an_allowed_decision_is_not_an_error() {
        assert!(check(true, "db:drop", "analytics").is_ok());
    }

    #[test]
    fn args_encodes_a_map_as_cbor() {
        #[derive(serde::Serialize)]
        struct Dims<'a> {
            table: &'a str,
        }
        let bytes = args(Dims { table: "events" }).unwrap();
        let value: ciborium::value::Value = ciborium::from_reader(bytes.as_slice()).unwrap();
        assert!(value.is_map(), "args must encode as a CBOR map: {value:?}");
    }

    #[test]
    fn args_rejects_a_non_map() {
        // ACT-CONSENT §2.2: a non-map carries no dimensions. Encoding one is
        // almost certainly a mistake at the call site, so it is refused here
        // rather than silently ignored by the host.
        assert!(args("not a map").is_err());
    }
}
