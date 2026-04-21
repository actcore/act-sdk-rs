//! Stateful counter component built with `act-sdk`'s session-provider macros.
//!
//! Each session holds a `u64` counter. `read` returns it, `increment`
//! bumps it. The `std:session-id` metadata key identifies which counter
//! a tool call operates on.

use act_sdk::prelude::*;

#[act_component(
    name = "sessions-counter",
    version = "0.1.0",
    description = "Per-session counter — exercise act:sessions/session-provider"
)]
mod component {
    use super::*;

    pub struct Counter {
        value: u64,
    }

    thread_local! {
        static SESSIONS: SessionRegistry<Counter> = SessionRegistry::new("ctr");
    }

    /// Args accepted by `open-session`.
    #[derive(Deserialize, JsonSchema)]
    #[schemars(crate = "act_sdk::__private::schemars")]
    #[serde(crate = "act_sdk::__private::serde")]
    pub struct OpenArgs {
        /// Initial counter value.
        #[serde(default)]
        start: u64,
    }

    /// Tool metadata: requires `std:session-id`.
    #[derive(Deserialize)]
    #[serde(crate = "act_sdk::__private::serde")]
    pub struct ToolMeta {
        #[serde(rename = "std:session-id")]
        session_id: String,
    }

    #[session_open]
    fn open(args: OpenArgs) -> ActResult<String> {
        Ok(SESSIONS.with(|r| r.insert(Counter { value: args.start })))
    }

    #[session_close]
    fn close(session_id: String) {
        SESSIONS.with(|r| {
            r.remove(&session_id);
        });
    }

    #[act_tool(description = "Read the current counter value for this session.")]
    async fn read(ctx: &mut ActContext<ToolMeta>) -> ActResult<u64> {
        let id = ctx.metadata().session_id.clone();
        SESSIONS
            .with(|r| r.with(&id, |c| c.value))
            .ok_or_else(|| ActError::session_not_found(format!("Unknown session-id: {id}")))
    }

    #[act_tool(description = "Increment the counter by `by` and return the new value.")]
    async fn increment(
        /// Amount to add to the counter.
        by: u64,
        ctx: &mut ActContext<ToolMeta>,
    ) -> ActResult<u64> {
        let id = ctx.metadata().session_id.clone();
        SESSIONS
            .with(|r| {
                r.with_mut(&id, |c| {
                    c.value += by;
                    c.value
                })
            })
            .ok_or_else(|| ActError::session_not_found(format!("Unknown session-id: {id}")))
    }
}
