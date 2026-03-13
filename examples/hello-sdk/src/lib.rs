use act_sdk::prelude::*;

#[derive(Deserialize, JsonSchema)]
struct GreetArgs {
    /// Name of the person to greet
    name: String,
}

#[act_component(
    name = "hello-sdk",
    version = "0.1.0",
    description = "Hello world using act-sdk"
)]
mod component {
    use super::*;

    #[act_tool(description = "Say hello to someone", read_only)]
    fn greet(args: GreetArgs) -> ActResult<String> {
        Ok(format!("Hello, {}!", args.name))
    }

    #[act_tool(description = "List supported greetings", read_only)]
    fn list_greetings() -> ActResult<String> {
        Ok("hello, hi, hey, greetings".to_string())
    }

    #[act_tool(description = "Count from 1 to N", streaming)]
    async fn count(
        #[doc = "Number to count to"] n: u32,
        ctx: &mut ActContext<()>,
    ) -> ActResult<()> {
        for i in 1..=n {
            ctx.send_progress(i as u64, n as u64, format!("Count: {i}"))
                .await?;
        }
        Ok(())
    }
}
