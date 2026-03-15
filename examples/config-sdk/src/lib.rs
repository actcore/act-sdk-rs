use act_sdk::prelude::*;

#[derive(Deserialize, JsonSchema)]
struct AppConfig {
    /// API key for authentication
    api_key: String,
    /// Optional greeting prefix
    #[serde(default = "default_prefix")]
    prefix: String,
}

fn default_prefix() -> String {
    "Hello".to_string()
}

#[act_component(
    name = "config-example",
    version = "0.1.0",
    description = "Example component with config"
)]
mod component {
    use super::*;

    #[act_tool(description = "Greet with configured prefix")]
    fn greet(
        /// Name of the person to greet
        name: String,
        ctx: &mut ActContext<AppConfig>,
    ) -> ActResult<String> {
        let meta = ctx.metadata();
        Ok(format!(
            "{}, {}! (key: {}...)",
            meta.prefix,
            name,
            &meta.api_key[..3.min(meta.api_key.len())]
        ))
    }
}
