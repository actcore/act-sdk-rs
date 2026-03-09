pub mod cbor;
pub mod context;
pub mod response;
pub mod types;

pub use act_sdk_macros::{act_component, act_tool};
pub use context::ActContext;
pub use response::IntoResponse;
pub use types::{ActError, ActResult};

pub mod prelude {
    pub use crate::{act_component, act_tool};
    pub use crate::{ActContext, ActError, ActResult, IntoResponse};
    pub use schemars::JsonSchema;
    pub use serde::Deserialize;
}

// Re-export dependencies that generated code needs
#[doc(hidden)]
pub mod __private {
    pub use ciborium;
    pub use schemars;
    pub use serde;
    pub use serde_json;
    pub use wit_bindgen;
}
