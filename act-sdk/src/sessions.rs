//! Session lifecycle helpers for components that export
//! `act:sessions/session-provider`.
//!
//! [`SessionRegistry`] is the runtime state container — a map from
//! component-allocated session-ids to user-defined per-session state
//! `T`. It is intentionally small; the macro layer
//! (`#[act_session_provider]`, planned) generates the WIT exports on
//! top of it.
//!
//! Components are single-threaded under wasm32-wasip2, so this module
//! uses interior mutability (`RefCell`) without any locking. The
//! registry is `!Sync` and `!Send` by design — the macro emits a
//! `thread_local!` static.
//!
//! # Manual usage
//!
//! ```ignore
//! use act_sdk::sessions::SessionRegistry;
//!
//! pub struct CounterSession { value: u64 }
//!
//! thread_local! {
//!     static SESSIONS: SessionRegistry<CounterSession> =
//!         SessionRegistry::new("ctr");
//! }
//!
//! // open-session impl:
//! let id = SESSIONS.with(|r| r.insert(CounterSession { value: 0 }));
//!
//! // call-tool impl: read counter for an opaque session-id
//! let v = SESSIONS.with(|r| r.with(&session_id, |s| s.value));
//! ```
//!
//! # Helpers
//!
//! - [`session_id_from_metadata`] — extract `std:session-id` from a
//!   `metadata` list (CBOR-decoded), per ACT-CONSTANTS.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// Per-component map of session-id → user state `T`.
///
/// `T` is whatever the component wants to keep around for a session
/// (a database connection, a counter, a parsed config, …). `T` is
/// stored by value; if it owns expensive resources, drop in
/// [`SessionRegistry::remove`] runs your `Drop` impl.
///
/// Ids are allocated as `"<prefix>_<counter>"` where `prefix` is set
/// at construction. Components SHOULD use a short, recognisable
/// prefix; per-component uniqueness is what matters since the host
/// scopes session-ids to one component instance.
pub struct SessionRegistry<T> {
    inner: RefCell<HashMap<String, T>>,
    next_id: Cell<u64>,
    prefix: &'static str,
}

impl<T> SessionRegistry<T> {
    /// Create an empty registry. `prefix` becomes part of every
    /// allocated session-id (e.g. `"ctr"` → `"ctr_0"`, `"ctr_1"`, …).
    pub fn new(prefix: &'static str) -> Self {
        Self {
            inner: RefCell::new(HashMap::new()),
            next_id: Cell::new(0),
            prefix,
        }
    }

    /// Insert a fresh session, returning its allocated id.
    pub fn insert(&self, value: T) -> String {
        let n = self.next_id.get();
        self.next_id.set(n + 1);
        let id = format!("{}_{}", self.prefix, n);
        self.inner.borrow_mut().insert(id.clone(), value);
        id
    }

    /// Look up a session by id and apply `f` to a shared reference.
    /// Returns `None` if the id is unknown.
    pub fn with<R>(&self, id: &str, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.inner.borrow().get(id).map(f)
    }

    /// Look up a session by id and apply `f` to a mutable reference.
    /// Returns `None` if the id is unknown.
    pub fn with_mut<R>(&self, id: &str, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        self.inner.borrow_mut().get_mut(id).map(f)
    }

    /// Remove a session by id. The dropped `T` is returned (if any).
    pub fn remove(&self, id: &str) -> Option<T> {
        self.inner.borrow_mut().remove(id)
    }

    /// Number of currently-open sessions.
    pub fn len(&self) -> usize {
        self.inner.borrow().len()
    }

    /// Whether the registry has no open sessions.
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }
}

impl<T> Default for SessionRegistry<T> {
    fn default() -> Self {
        Self::new("sid")
    }
}

/// Extract `std:session-id` from a metadata list (the WIT
/// `metadata = list<tuple<string, list<u8>>>` shape host calls deliver).
///
/// CBOR-decodes the value; returns `None` if the key is absent or
/// the value isn't a string.
pub fn session_id_from_metadata(metadata: &[(String, Vec<u8>)]) -> Option<String> {
    for (key, value) in metadata {
        if key == act_types::constants::META_SESSION_ID
            && let Ok(serde_json::Value::String(s)) =
                ciborium::from_reader::<serde_json::Value, _>(value.as_slice())
        {
            return Some(s);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_allocated_in_sequence() {
        let r = SessionRegistry::<u64>::new("test");
        assert_eq!(r.insert(0), "test_0");
        assert_eq!(r.insert(1), "test_1");
        assert_eq!(r.insert(2), "test_2");
    }

    #[test]
    fn lookup_returns_value() {
        let r = SessionRegistry::<u64>::new("c");
        let id = r.insert(42);
        assert_eq!(r.with(&id, |v| *v), Some(42));
    }

    #[test]
    fn mutate_updates_value() {
        let r = SessionRegistry::<u64>::new("c");
        let id = r.insert(0);
        r.with_mut(&id, |v| *v += 1);
        assert_eq!(r.with(&id, |v| *v), Some(1));
    }

    #[test]
    fn remove_returns_value_and_clears_entry() {
        let r = SessionRegistry::<String>::new("c");
        let id = r.insert("hello".to_string());
        assert_eq!(r.remove(&id), Some("hello".to_string()));
        assert_eq!(r.with(&id, |s| s.clone()), None);
    }

    #[test]
    fn unknown_id_returns_none() {
        let r = SessionRegistry::<u64>::new("c");
        assert!(r.with("c_999", |_| ()).is_none());
        assert!(r.with_mut("c_999", |_| ()).is_none());
        assert!(r.remove("c_999").is_none());
    }

    #[test]
    fn session_id_extraction_decodes_cbor_string() {
        let mut buf = Vec::new();
        ciborium::into_writer(&"abc-123", &mut buf).unwrap();
        let meta = vec![("std:session-id".to_string(), buf)];
        assert_eq!(session_id_from_metadata(&meta), Some("abc-123".to_string()));
    }

    #[test]
    fn session_id_missing_returns_none() {
        let meta: Vec<(String, Vec<u8>)> = vec![];
        assert_eq!(session_id_from_metadata(&meta), None);
    }
}
