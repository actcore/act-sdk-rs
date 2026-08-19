//! Reads a credential from the host store and reports its shape.
//!
//! This is the SDK's end-to-end proof for `act:credentials`. It deliberately
//! never returns the material: the point of the store is that the agent
//! driving this component cannot read what the component reads. The tool
//! reports the kind, the field names and the OAuth scopes — all of which the
//! agent may see — and there is no path here by which a field *value* reaches
//! a tool result.

use act_sdk::credentials::Secret;
use act_sdk::prelude::*;
use serde::Serialize;

#[act_component]
mod component {
    use super::*;

    thread_local! {
        static SESSIONS: SessionRegistry<()> = SessionRegistry::new("creds");
    }

    /// Args accepted by `open-session`. None: this component authenticates
    /// nothing at open time, on purpose — see `open` below.
    #[derive(Deserialize, JsonSchema)]
    #[schemars(crate = "act_sdk::__private::schemars")]
    #[serde(crate = "act_sdk::__private::serde")]
    pub struct OpenArgs {}

    /// Tool metadata: requires `std:session-id`.
    #[derive(Deserialize)]
    #[serde(crate = "act_sdk::__private::serde")]
    pub struct ToolMeta {
        #[serde(rename = "std:session-id")]
        session_id: String,
    }

    /// What the agent is allowed to learn about a stored credential.
    #[derive(Serialize, JsonSchema)]
    #[schemars(crate = "act_sdk::__private::schemars")]
    #[serde(crate = "act_sdk::__private::serde")]
    pub struct Description {
        /// The secret's kind, e.g. `std:opaque`.
        kind: String,
        /// Field names only. Never their values.
        fields: Vec<String>,
        /// Scopes, for a `std:oauth2` secret. Empty otherwise.
        scopes: Vec<String>,
    }

    #[session_open]
    fn open(_args: OpenArgs) -> ActResult<String> {
        // No get-secret here, deliberately. The host marks a session live
        // only *after* open-session returns, and the id is component-chosen,
        // so a fetch from inside names a session the host has never seen and
        // is refused with invalid-session (ACT-AUTH.md 1.1.4). Fetch lazily,
        // on the tool call. Do not "fix" this by moving it up.
        Ok(SESSIONS.with(|r| r.insert(())))
    }

    #[session_close]
    fn close(session_id: String) {
        SESSIONS.with(|r| {
            r.remove(&session_id);
        });
    }

    #[act_tool(description = "Describe the credential stored under `key`, without disclosing it.")]
    async fn describe(
        /// Store lookup key, within this component's own profile.
        key: String,
        ctx: &mut ActContext<ToolMeta>,
    ) -> ActResult<Description> {
        let session = ctx.metadata().session_id.clone();

        let want = crate::act::credentials::store::SecretRequest {
            key,
            kind: None,
            resource: None,
            scopes: vec![],
            hint: Some("describe the stored credential".into()),
        };

        let raw = crate::act::credentials::store::get_secret(session, want)
            .await
            .map_err(|e| ActError::internal(format!("credential store refused: {e:?}")))?;

        let secret = Secret::from_wit(raw.kind, raw.fields)
            .map_err(|e| ActError::internal(e.to_string()))?;

        Ok(Description {
            kind: secret.kind().to_string(),
            fields: secret.fields.keys().cloned().collect(),
            // The OAuth field, when there is one, is named by the component
            // that declared it — a stored record carries no types, so nothing
            // can find it by shape. This example uses "tok" by convention.
            scopes: secret
                .as_oauth2("tok")
                .map(|o| o.scopes)
                .unwrap_or_default(),
        })
    }
}
