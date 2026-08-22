//! Worked example of a semantic-authorization call site.
//!
//! ACT enforces physical capability classes by interception: the host sits
//! on the `wasi:filesystem`/`wasi:http`/`wasi:sockets` boundary, so a
//! component can never reach what the operator did not grant, whatever its
//! code does. `DROP DATABASE analytics` has no such boundary to sit on — it
//! reaches Postgres over a socket the operator already granted. `act:consent`
//! closes that gap by inverting the initiative: the component names the
//! class of action it is about to take (`db:drop`, here) and the host
//! decides against the operator's policy, the same grants and audit trail as
//! any physical class (ACT-CONSENT.md §1-§2).
//!
//! This is the SDK's call-site documentation, not a database client — **it
//! drops nothing**. `drop_database` asks *immediately before* the point
//! where it would act, not at session open (ACT-CONSENT.md §6: a decision
//! taken far from the action it authorizes is a decision about a different
//! thing). On `allow` it reports what it would have executed; on `deny` it
//! returns a tool error carrying `std:capability-denied` (ACT-CONSTANTS.md
//! §9) so the agent learns the action was refused rather than that it
//! failed.
//!
//! See also the hand-rolled `consent-canary` fixture in `act-cli` (its
//! `tests/fixtures-src/consent-canary`), which drives the identical call
//! across a real component boundary for the host's own integration tests —
//! this example is the SDK-macro-based version of the same shape.

use act_sdk::prelude::*;
use serde::{Deserialize, Serialize};

#[act_component]
mod component {
    use super::*;

    /// Every metadata entry the call carried, captured generically rather
    /// than field-by-field.
    ///
    /// `consent-authority::request` wants the WIT `metadata` this call
    /// arrived with — ACT-CONSENT.md §7.1 requires passing it through so the
    /// host can anchor the decision (e.g. to a session), not a
    /// component-chosen subset of it. `#[act_tool]`'s typed `ActContext` is
    /// the only lane into a tool function, so `#[serde(flatten)]` catches
    /// every field that lane decodes rather than naming one. See
    /// `to_wit_metadata` below for the trip back.
    #[derive(Deserialize)]
    pub struct ToolMeta {
        #[serde(flatten)]
        fields: serde_json::Map<String, serde_json::Value>,
    }

    /// Re-encode metadata the macro already CBOR-decoded to JSON back into
    /// the WIT `metadata` wire shape (`list<tuple<string, cbor>>`).
    ///
    /// This is not a faithful round-trip of everything the call carried.
    /// `act-sdk-macros`'s `metadata_parse` step (`component.rs`, building
    /// this tool's `ActContext`) decodes each entry with
    /// `from_cbor::<serde_json::Value>` and silently drops any entry that
    /// fails to decode — before `#[serde(flatten)]` above ever sees it. A
    /// CBOR byte string is one such value: `std:forward`
    /// (`ACT-CONSTANTS.md` §6, "object (CBOR-encoded metadata)") is built
    /// from byte strings and is dropped this way, with no error and
    /// nothing in this function to recover it. The six Cross-Cutting
    /// Metadata keys (`ACT-CONSTANTS.md` §5 — `std:session-id` and its
    /// siblings) are all plain `string`s, so they do survive; that is what
    /// lets `ACT-CONSENT.md` §7.1's stated purpose — anchoring the
    /// decision to a session — still hold here. Key order is also not
    /// preserved (`serde_json::Map` is a `BTreeMap` in this workspace), but
    /// neither spec treats metadata order as significant.
    fn to_wit_metadata(
        fields: &serde_json::Map<String, serde_json::Value>,
    ) -> Vec<(String, Vec<u8>)> {
        fields
            .iter()
            .map(|(k, v)| (k.clone(), act_sdk::cbor::to_cbor(v)))
            .collect()
    }

    /// The one class this example ever asks for. Declared bare in
    /// `act.toml` — see ACT-CONSENT.md §3.1.
    const CLASS: &str = "db:drop";

    /// What `drop_database` reports instead of dropping anything.
    #[derive(Serialize, JsonSchema)]
    pub struct DropReport {
        /// The database named in the request.
        database: String,
        /// The statement this example would have executed, had it been a
        /// real database client rather than SDK documentation.
        would_execute: String,
    }

    #[act_tool(
        description = "Ask the operator to authorize dropping `database` under the db:drop \
                        semantic class, and report the decision. Never actually drops \
                        anything — this is a worked example for the SDK docs, not a \
                        database client."
    )]
    async fn drop_database(
        /// Name of the database that would be dropped.
        database: String,
        ctx: &mut ActContext<ToolMeta>,
    ) -> ActResult<DropReport> {
        // Ask immediately before the point where the action would happen —
        // not earlier, and not cached from an open-session step this
        // component doesn't have (ACT-CONSENT.md §6).
        let meta = to_wit_metadata(&ctx.metadata().fields);

        let req = crate::act::consent::types::ConsentRequest {
            class: CLASS.to_string(),
            // `key` is the plain database name — a bare identifier, per
            // ACT-CONSENT.md §8.2, not a path or a URL, so an operator's
            // glob pattern over it means what it appears to mean.
            key: database.clone(),
            summary: format!("Drop database \"{database}\""),
            // No dimension beyond `key` is worth declaring for this class;
            // `act_sdk::consent::args` exists for when one is.
            args: Vec::new(),
        };

        let decision = crate::act::consent::consent_authority::request(req, meta).await;

        act_sdk::consent::check(
            matches!(decision, crate::act::consent::types::Decision::Allow),
            CLASS,
            &database,
        )
        .map_err(|e| ActError::capability_denied(e.to_string()))?;

        Ok(DropReport {
            would_execute: format!("DROP DATABASE \"{database}\""),
            database,
        })
    }
}
