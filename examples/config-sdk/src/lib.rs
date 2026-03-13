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

#[derive(Deserialize, JsonSchema)]
struct GreetArgs {
    /// Name of the person to greet
    name: String,
}

#[act_component(
    name = "config-example",
    version = "0.1.0",
    description = "Example component with config"
)]
mod component {
    use super::*;

    #[act_tool(description = "Greet with configured prefix")]
    fn greet(args: GreetArgs, ctx: &mut ActContext<AppConfig>) -> ActResult<String> {
        let config = ctx.config();
        Ok(format!(
            "{}, {}! (key: {}...)",
            config.prefix,
            args.name,
            &config.api_key[..3.min(config.api_key.len())]
        ))
    }
}
