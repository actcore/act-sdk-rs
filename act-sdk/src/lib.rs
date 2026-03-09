pub use act_sdk_macros::{act_component, act_tool};

pub mod prelude {
    pub use crate::{act_component, act_tool};
    pub use serde::Deserialize;
    pub use schemars::JsonSchema;
}
