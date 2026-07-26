pub mod capability;
pub mod cbor;
pub mod constants;
pub mod http;
#[deprecated(
    since = "0.14.0",
    note = "hand-rolled JSON-RPC types existed only to serve `act_types::mcp`; \
            use `rmcp::model` for MCP wire types. Scheduled for removal in 0.15.0."
)]
pub mod jsonrpc;
#[deprecated(
    since = "0.14.0",
    note = "MCP wire types now come from `rmcp::model`, the reference SDK, which \
            tracks protocol revisions for us — these hand-rolled types are pinned \
            to MCP 2025-11-25 and do not model 2026-07-28. Scheduled for removal \
            in 0.15.0."
)]
pub mod mcp;
pub mod types;

pub use capability::{Capabilities, CapabilityRequest, Constraint};
pub use types::{
    ActError, ActResult, ComponentInfo, FilesystemAllow, FilesystemMount, FsMode, HttpAllow,
    LocalizedString, Metadata, MountType, SocketProtocol, SocketsAllow, StdComponentInfo,
    validate_mounts,
};
