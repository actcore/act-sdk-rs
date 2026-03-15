//! ACT-HTTP protocol types for request/response serialization.
//!
//! All types derive both `Serialize` and `Deserialize` so they can be used
//! by servers (act-host), clients (act-bridge), and SDKs alike.

use serde::{Deserialize, Serialize};

/// ACT-HTTP protocol version.
pub const PROTOCOL_VERSION: &str = "0.2";

/// HTTP header name for the protocol version.
pub const HEADER_PROTOCOL_VERSION: &str = "ACT-Protocol-Version";

/// Server metadata returned by `GET /info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub default_language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<Capability>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// A server capability declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Tool definition returned in `ListToolsResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Response from `POST /tools`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResponse {
    pub tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Request body for `POST /metadata-schema`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetadataSchemaRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Request body for `POST /tools` and `QUERY /tools`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetadataRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Request body for `POST /tools/{name}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub arguments: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// A content part in a tool response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPart {
    pub data: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Response from `POST /tools/{name}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResponse {
    pub content: Vec<ContentPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Error object in error responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolError {
    pub kind: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Wrapper for error responses (`{"error": ...}`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: ToolError,
}

/// Resource info returned by `POST /resources`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Response from `POST /resources`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourcesResponse {
    pub resources: Vec<ResourceInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Map an ACT error kind to an HTTP status code per ACT-HTTP spec.
pub fn error_kind_to_status(kind: &str) -> u16 {
    use crate::constants::*;
    match kind {
        ERR_NOT_FOUND => 404,
        ERR_INVALID_ARGS => 422,
        ERR_TIMEOUT => 504,
        ERR_CAPABILITY_DENIED => 403,
        _ => 500,
    }
}
