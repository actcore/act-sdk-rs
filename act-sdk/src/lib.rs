pub mod bytes;
pub mod cbor;
pub mod context;
pub mod response;
pub mod sessions;
pub mod types;

pub use act_sdk_macros::{act_component, act_tool, session_close, session_open};
pub use bytes::Bytes;
pub use context::ActContext;
pub use response::{Content, IntoToolResponse, Json};
pub use sessions::SessionRegistry;
pub use types::{ActError, ActResult};

pub mod prelude {
    pub use crate::{ActContext, ActError, ActResult, Bytes, Content, IntoToolResponse, Json};
    pub use crate::{SessionRegistry, sessions::session_id_from_metadata};
    pub use crate::{act_component, act_tool, session_close, session_open};
    pub use schemars::JsonSchema;
    pub use serde::Deserialize;
}

// Re-export act-types constants for use by generated code and consumers
pub use act_types::constants;

/// Spawn a concurrent task on the component's async runtime.
///
/// Re-exported from `wit-bindgen` so components do not need their own
/// `wit-bindgen` dependency. Use it when a tool has to produce data
/// concurrently with the call it is serving (e.g. filling a request-body
/// stream while awaiting the response).
pub use wit_bindgen::spawn_local;

// Re-export dependencies that generated code needs
#[doc(hidden)]
pub mod __private {
    pub use act_types::cbor as ciborium_compat;
    pub use schemars;
    pub use serde;
    pub use serde_json;
    pub use wit_bindgen;
}
