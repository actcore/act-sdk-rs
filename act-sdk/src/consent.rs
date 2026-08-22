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
//! // Once per component, at module scope: wires the component's own
//! // generated `Decision` enum into `check` below.
//! act_sdk::impl_consent_decision!(crate::act::consent::types::Decision);
//!
//! let req = ConsentRequest {
//!     class: "db:drop".into(),
//!     key: database.clone(),
//!     summary: format!("Drop database \"{database}\""),
//!     args: Vec::new(),
//! };
//! let decision = act::consent::consent_authority::request(req, meta).await;
//! act_sdk::consent::check(decision, "db:drop", &database)?;
//! ```
//!
//! `check` takes anything implementing [`ConsentDecision`] rather than the
//! WIT `decision` directly, because `decision` is generated per component,
//! the same as `consent_authority::request` itself — this crate cannot name
//! it either, one level down from the wall that keeps an SDK-side
//! `require()` from existing at all. [`impl_consent_decision!`] bridges the
//! two crates without breaking that wall: it expands to a trait impl over a
//! path the caller names, not to a call across it.

use serde::Serialize;

/// Bridges a component's own generated `act:consent` `Decision` enum into
/// [`check`].
///
/// Implement it with [`impl_consent_decision!`] rather than reducing a
/// decision to `bool` by hand at the call site: a hand-written
/// `matches!(decision, Decision::Deny)` compiles and proceeds on a refusal
/// if the polarity is ever gotten backwards, and the corpus already has both
/// spellings in circulation — `ACT-CONSENT.md` Appendix A and the
/// `consent-canary` fixture both branch on `Deny`, while this module's
/// `check` requires `Allow`. One macro call generates the polarity once, so
/// every call site derived from it agrees.
pub trait ConsentDecision {
    /// `true` for the WIT `decision::allow` variant, `false` for `deny`.
    fn is_allowed(&self) -> bool;
}

/// Implements [`ConsentDecision`] for a component's own generated
/// `act:consent` `Decision` enum, one line at the component:
///
/// ```ignore
/// act_sdk::impl_consent_decision!(crate::act::consent::types::Decision);
/// ```
///
/// Orphan rules allow this even though neither this crate nor the
/// component's crate defines both halves: the enum named by `$ty` is local
/// to the component's own crate (`wit_bindgen::generate!` defines it there),
/// and `ConsentDecision` is the foreign trait — an impl of a foreign trait
/// for a local type is exactly what orphan rules permit. The macro only
/// ever expands to that trait impl over the path the caller names; it does
/// not call the host, so it stays on the SDK side of the wall this module's
/// doc describes.
#[macro_export]
macro_rules! impl_consent_decision {
    ($ty:path) => {
        impl $crate::consent::ConsentDecision for $ty {
            fn is_allowed(&self) -> bool {
                matches!(self, <$ty>::Allow)
            }
        }
    };
}

/// A refusal. Convert it into a tool error carrying `std:capability-denied`
/// (`ACT-CONSTANTS.md` §9) so the agent learns the action was refused rather
/// than that it failed — `From<Denied> for ActError` below does this
/// directly, with `?`.
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

/// The conversion `ACT-CONSENT.md` §6's SHOULD asks for: a refusal becomes
/// the SDK's tool error carrying `std:capability-denied`
/// (`ACT-CONSTANTS.md` §9). Without this, every call site hand-writes
/// `.map_err(|e| ActError::capability_denied(e.to_string()))?` and can get
/// the kind wrong; with it, `act_sdk::consent::check(decision, CLASS, &key)?`
/// is the whole call site.
impl From<Denied> for crate::ActError {
    fn from(d: Denied) -> Self {
        crate::ActError::capability_denied(d.to_string())
    }
}

/// Turn a decision into a `Result` so a refusal short-circuits with `?`.
///
/// Takes anything implementing [`ConsentDecision`] — in practice, the
/// component's own generated `Decision`, wired up with
/// [`impl_consent_decision!`]. This is the documented path: it fixes the
/// polarity once instead of trusting every call site to reduce the decision
/// to a `bool` correctly. See [`check_allowed`] for the boolean escape
/// hatch.
pub fn check<D: ConsentDecision>(decision: D, class: &str, key: &str) -> Result<(), Denied> {
    check_allowed(decision.is_allowed(), class, key)
}

/// The boolean form of [`check`], for a caller that has already reduced the
/// decision to a `bool` itself. Prefer `check` with
/// [`impl_consent_decision!`] instead — this function trusts the caller to
/// have gotten the polarity right, which is exactly the mistake `check`
/// exists to rule out. `allowed = true` means `Decision::Allow`.
pub fn check_allowed(allowed: bool, class: &str, key: &str) -> Result<(), Denied> {
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
    /// The value did not encode as a CBOR map with text keys — the same
    /// shape the host requires.
    ///
    /// Stricter here than the host's own fallback: `ACT-CONSENT.md` §2.2
    /// says a non-map `args` "carries no dimensions rather than being an
    /// error" once it reaches `consent-authority::request`. This function
    /// refuses it earlier instead, because a non-map value at this call
    /// site is almost always a mistake (a type that serializes as a string
    /// or a list, say) rather than a deliberate no-dimensions request, and
    /// surfacing that mistake here beats letting the host silently drop it.
    NotAMap,
    /// The value failed to serialize as CBOR, or the resulting bytes failed
    /// to decode back — typically a `Serialize` impl producing a value this
    /// crate's CBOR↔JSON bridge cannot project (a non-finite float, an
    /// integer outside the JSON-safe range), not a map/non-map shape
    /// problem.
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
/// the common case; pass `args: Vec::new()` directly at the call site for
/// that rather than calling this function. (`args(())` is not the same
/// thing: `()` encodes as CBOR null, not an empty map, and is rejected as
/// [`ArgsError::NotAMap`].) Note that only `key` is host-resolved: a
/// dimension here is one the component itself supplies, so an operator's
/// `allow` over it is advisory. See `ACT-CONSENT.md` §8.6.
///
/// Refuses a non-map `dimensions` with [`ArgsError::NotAMap`] — stricter
/// than the host's own fallback; see that variant's doc for why. The check
/// is `act_types::cbor::cbor_to_json(&buf) == Ok(Value::Object(_))`,
/// deliberately the same test the host applies
/// (`act-runtime`'s `args_to_attrs`), rather than a plain "is this a CBOR
/// map" check: a map with non-text keys, a non-finite float, or an integer
/// outside the JSON-safe range all encode as a CBOR map by the looser test
/// but are rejected by the host's, which would otherwise let this guard
/// pass a map the host silently drops.
pub fn args(dimensions: impl Serialize) -> Result<Vec<u8>, ArgsError> {
    let mut buf = Vec::new();
    ciborium::into_writer(&dimensions, &mut buf).map_err(|e| ArgsError::Encode(e.to_string()))?;
    if !matches!(
        act_types::cbor::cbor_to_json(&buf),
        Ok(serde_json::Value::Object(_))
    ) {
        return Err(ArgsError::NotAMap);
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    enum FakeDecision {
        Allow,
        Deny,
    }
    crate::impl_consent_decision!(FakeDecision);

    #[test]
    fn check_allows_on_the_allow_variant() {
        assert!(check(FakeDecision::Allow, "db:drop", "analytics").is_ok());
    }

    #[test]
    fn check_denies_on_the_deny_variant() {
        let e = check(FakeDecision::Deny, "db:drop", "analytics").unwrap_err();
        // The agent is told what was refused, because it may be able to proceed
        // differently — and the operator's audit line uses the same two values.
        assert_eq!(e.class(), "db:drop");
        assert_eq!(e.key(), "analytics");
    }

    #[test]
    fn denied_carries_the_class_and_key_it_refused() {
        let e = check_allowed(false, "db:drop", "analytics").unwrap_err();
        assert_eq!(e.class(), "db:drop");
        assert_eq!(e.key(), "analytics");
    }

    #[test]
    fn an_allowed_decision_is_not_an_error() {
        assert!(check_allowed(true, "db:drop", "analytics").is_ok());
    }

    #[test]
    fn denied_display_names_the_class_and_key() {
        let e = check_allowed(false, "db:drop", "analytics").unwrap_err();
        assert_eq!(e.to_string(), "db:drop on \"analytics\" was not authorized");
    }

    #[test]
    fn denied_display_with_an_empty_key_names_only_the_class() {
        // ACT-CONSENT.md §2.2: an empty key approves or refuses the whole
        // class, not one subject within it, so the message should say so
        // distinctly rather than printing an empty pair of quotes.
        let e = check_allowed(false, "db:drop", "").unwrap_err();
        assert_eq!(e.to_string(), "db:drop was not authorized");
    }

    #[test]
    fn denied_converts_into_a_capability_denied_tool_error() {
        // ACT-CONSENT.md §6's SHOULD: a refusal becomes a tool error carrying
        // `std:capability-denied`, not a generic failure.
        let e = check_allowed(false, "db:drop", "analytics").unwrap_err();
        let err: crate::ActError = e.into();
        assert_eq!(err.kind, act_types::constants::ERR_CAPABILITY_DENIED);
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

    #[test]
    fn args_rejects_a_map_with_non_text_keys() {
        // A map keyed by integers encodes as a CBOR map (ciborium's
        // `Value::is_map()` accepts it) but is not a JSON object, which is
        // what the host's own acceptance test (`args_to_attrs`, via
        // `cbor_to_json`) requires. Left unchecked, this passes the SDK's
        // guard and is then silently dropped by the host — precisely the
        // outcome `ArgsError::NotAMap`'s doc says the guard prevents.
        let dims: std::collections::BTreeMap<i32, &str> = [(1, "x")].into_iter().collect();
        assert!(args(dims).is_err());
    }
}
