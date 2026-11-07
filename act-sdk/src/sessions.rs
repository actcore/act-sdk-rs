//! Session lifecycle helpers for components that export
//! `act:sessions/session-provider`.
//!
//! [`SessionRegistry`] is the runtime state container — a map from
//! component-allocated session-ids to user-defined per-session state
//! `T`. It is intentionally small; the `#[session_open]` / `#[session_close]`
//! macros and the macro-generated session-provider exports build on top of it.
//!
//! The registry must live in a **plain `static`** (via [`LazyLock`]), not in
//! `thread_local!`. Under wasm32-wasip2 a `thread_local!` static degrades to
//! ordinary linear memory (there are no threads), but wasip3 guests get real
//! thread-local storage: the TLS pointer is carried per stack, and the host's
//! concurrent scheduler runs each request on its own fiber. A registry in
//! `thread_local!` is therefore repopulated from scratch on every tool call —
//! sessions opened in one call are unknown in the next. A `static` lives in
//! linear memory, which all fibers of the instance share. The interior
//! `Mutex` makes the registry `Sync` (a `static` requirement) and is safe
//! under the cooperative scheduler because critical sections never await —
//! a fiber can only switch at an await point, never inside a lock.
//!
//! # Manual usage
//!
//! ```ignore
//! use act_sdk::sessions::SessionRegistry;
//! use std::sync::LazyLock;
//!
//! pub struct CounterSession { value: u64 }
//!
//! static SESSIONS: LazyLock<SessionRegistry<CounterSession>> =
//!     LazyLock::new(|| SessionRegistry::new("ctr"));
//!
//! // open-session impl:
//! let id = SESSIONS.insert(CounterSession { value: 0 });
//!
//! // call-tool impl: read counter for an opaque session-id
//! let v = SESSIONS.with(&session_id, |s| s.value);
//! ```
//!
//! # Helpers
//!
//! - [`session_id_from_metadata`] — extract `std:session-id` from a
//!   `metadata` list (CBOR-decoded), per ACT-CONSTANTS.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

struct Inner<T> {
    sessions: HashMap<String, T>,
    next_id: u64,
}

/// Per-component map of session-id → user state `T`.
///
/// `T` is whatever the component wants to keep around for a session
/// (a database connection, a counter, a parsed config, …). `T` is
/// stored by value; if it owns expensive resources, drop in
/// [`SessionRegistry::remove`] runs your `Drop` impl.
///
/// Store the registry in a plain `static` via [`LazyLock`] — **not** in
/// `thread_local!` (see the module docs for why). This requires `T: Send`,
/// which session state (strings, connections, counters) satisfies.
///
/// Ids are allocated as `"<prefix>_<counter>"` where `prefix` is set
/// at construction. Components SHOULD use a short, recognisable
/// prefix; per-component uniqueness is what matters since the host
/// scopes session-ids to one component instance.
pub struct SessionRegistry<T> {
    inner: Mutex<Inner<T>>,
    prefix: &'static str,
}

impl<T: Send> SessionRegistry<T> {
    /// Create an empty registry. `prefix` becomes part of every
    /// allocated session-id (e.g. `"ctr"` → `"ctr_0"`, `"ctr_1"`, …).
    pub fn new(prefix: &'static str) -> Self {
        Self {
            inner: Mutex::new(Inner {
                sessions: HashMap::new(),
                next_id: 0,
            }),
            prefix,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner<T>> {
        // The only way this can fail is another fiber panicking while
        // holding the lock; no closure in this module panics mid-critical-
        // section, so the lock is only ever poisoned by guest code the
        // registry never calls back into. Unpoisoning via `unwrap_or_else`
        // keeps a session registry usable after an unrelated panic.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Insert a fresh session, returning its allocated id.
    pub fn insert(&self, value: T) -> String {
        let mut inner = self.lock();
        let id = format!("{}_{}", self.prefix, inner.next_id);
        inner.next_id += 1;
        inner.sessions.insert(id.clone(), value);
        id
    }

    /// Look up a session by id and apply `f` to a shared reference.
    /// Returns `None` if the id is unknown.
    pub fn with<R>(&self, id: &str, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.lock().sessions.get(id).map(f)
    }

    /// Look up a session by id and apply `f` to a mutable reference.
    /// Returns `None` if the id is unknown.
    pub fn with_mut<R>(&self, id: &str, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        self.lock().sessions.get_mut(id).map(f)
    }

    /// Remove a session by id. The dropped `T` is returned (if any).
    pub fn remove(&self, id: &str) -> Option<T> {
        self.lock().sessions.remove(id)
    }

    /// Number of currently-open sessions.
    pub fn len(&self) -> usize {
        self.lock().sessions.len()
    }

    /// Whether the registry has no open sessions.
    pub fn is_empty(&self) -> bool {
        self.lock().sessions.is_empty()
    }
}

// `Mutex<T>` is `Sync` for any `T: Send`, so the registry is `Sync` and can
// live in a `static`. Assert it so a future non-Sync interior fails to
// compile here rather than at every component that declares a `static`.
const _: () = {
    const fn assert_sync<T: Sync>() {}
    assert_sync::<SessionRegistry<u64>>();
};

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

    /// The registry must be usable from a plain `static`: session state
    /// written by one host fiber (or OS thread, here) has to be visible to
    /// every later call. Under wasm32-wasip3 a `thread_local!` static fails
    /// exactly this — each fiber gets its own TLS, so a session opened in
    /// one call is unknown to the next (search-brave, 2026-11-07). This test
    /// holds as long as [`SessionRegistry`] stays `Sync`.
    #[test]
    fn sessions_persist_across_threads() {
        static SESSIONS: std::sync::LazyLock<SessionRegistry<u64>> =
            std::sync::LazyLock::new(|| SessionRegistry::new("x"));

        let id = SESSIONS.insert(7);
        std::thread::scope(|s| {
            s.spawn(|| assert_eq!(SESSIONS.with(&id, |v| *v), Some(7)));
        });
        assert_eq!(SESSIONS.with(&id, |v| *v), Some(7));
    }
}
