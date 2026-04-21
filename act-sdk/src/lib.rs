pub mod cbor;
pub mod context;
pub mod response;
pub mod sessions;
pub mod types;

pub use act_sdk_macros::{act_component, act_tool, embed_skill, session_close, session_open};
pub use context::ActContext;
pub use response::{Content, IntoResponse, Json};
pub use sessions::SessionRegistry;
pub use types::{ActError, ActResult};

pub mod prelude {
    pub use crate::{ActContext, ActError, ActResult, Content, IntoResponse, Json};
    pub use crate::{SessionRegistry, sessions::session_id_from_metadata};
    pub use crate::{act_component, act_tool, session_close, session_open};
    pub use schemars::JsonSchema;
    pub use serde::Deserialize;
}

// Re-export act-types constants for use by generated code and consumers
pub use act_types::constants;

// Re-export dependencies that generated code needs
#[doc(hidden)]
pub mod __private {
    pub use act_types::cbor as ciborium_compat;
    pub use schemars;
    pub use serde;
    pub use serde_json;
    pub use wit_bindgen;
}
