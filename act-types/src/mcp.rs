//! MCP (Model Context Protocol) wire-format types.
//!
//! All types derive both `Serialize` and `Deserialize` so they can be used
//! by MCP servers (act-cli), MCP clients (mcp-bridge), and SDKs alike.
//!
//! Binary fields (`ImageContent.data`, `EmbeddedResource.blob`) are stored as
//! `Vec<u8>` and automatically base64-encoded/decoded via `serde_with`.
//!
//! JSON-RPC envelope types are re-exported from [`crate::jsonrpc`].

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{base64::Base64, serde_as, skip_serializing_none};

/// MCP protocol version supported by this crate.
pub const PROTOCOL_VERSION: &str = "2025-11-25";

// Re-export JSON-RPC types for convenience.
pub use crate::jsonrpc::{
    Body as JsonRpcBody, Error as JsonRpcError, Request as JsonRpcRequest,
    Response as JsonRpcResponse, Version as JsonRpcVersion,
};

// ── Initialize ──

/// Server info returned in the `initialize` response.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// Capabilities declared by the server in the `initialize` response.
#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(default)]
    pub tools: Option<Value>,
    #[serde(default)]
    pub resources: Option<Value>,
    #[serde(default)]
    pub prompts: Option<Value>,
}

/// The `result` payload of an `initialize` response.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub server_info: ServerInfo,
    #[serde(default)]
    pub capabilities: Option<ServerCapabilities>,
}

// ── Tools ──

/// MCP tool definition returned in `tools/list`.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_object_schema")]
    pub input_schema: Value,
    #[serde(default)]
    pub annotations: Option<ToolAnnotations>,
}

fn default_object_schema() -> Value {
    serde_json::json!({"type": "object"})
}

/// Tool annotations (behavioral hints).
#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    #[serde(default)]
    pub read_only_hint: Option<bool>,
    #[serde(default)]
    pub idempotent_hint: Option<bool>,
    #[serde(default)]
    pub destructive_hint: Option<bool>,
    #[serde(default)]
    pub open_world_hint: Option<bool>,
}

/// Response payload for `tools/list`.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResult {
    pub tools: Vec<ToolDefinition>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

/// Parameters for `tools/call`.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Option<Value>,
}

/// Response payload for `tools/call`.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    pub content: Vec<ContentItem>,
    #[serde(default)]
    pub is_error: Option<bool>,
}

// ── Content items ──

/// A content item in tool results.
///
/// Internally-tagged enum matching MCP's `type`-discriminated content items.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentItem {
    Text(TextContent),
    Image(ImageContent),
    Resource(ResourceContent),
}

/// Text content item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
}

/// Image content item.
///
/// `data` is stored as raw bytes and automatically base64-encoded on the wire.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
    pub mime_type: String,
}

/// Embedded resource content item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    pub resource: EmbeddedResource,
}

/// An embedded resource within a content item.
///
/// `blob` is stored as raw bytes and automatically base64-encoded on the wire.
#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedResource {
    pub uri: String,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde_as(as = "Option<Base64>")]
    #[serde(default)]
    pub blob: Option<Vec<u8>>,
}

// ── Error mapping ──

/// Map an ACT error kind to a JSON-RPC error code.
pub fn error_kind_to_jsonrpc_code(kind: &str) -> i32 {
    use crate::constants::*;
    match kind {
        ERR_NOT_FOUND => -32601,
        ERR_INVALID_ARGS => -32602,
        ERR_INTERNAL => -32603,
        _ => -32000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_definition_deserialize() {
        let json = json!({
            "name": "get_weather",
            "description": "Get weather",
            "inputSchema": {
                "type": "object",
                "properties": { "city": { "type": "string" } }
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false
            }
        });
        let tool: ToolDefinition = serde_json::from_value(json).unwrap();
        assert_eq!(tool.name, "get_weather");
        assert_eq!(tool.description.as_deref(), Some("Get weather"));
        let ann = tool.annotations.unwrap();
        assert_eq!(ann.read_only_hint, Some(true));
        assert_eq!(ann.destructive_hint, Some(false));
        assert_eq!(ann.idempotent_hint, None);
    }

    #[test]
    fn tool_definition_minimal() {
        let json = json!({ "name": "simple" });
        let tool: ToolDefinition = serde_json::from_value(json).unwrap();
        assert_eq!(tool.name, "simple");
        assert_eq!(tool.input_schema, json!({"type": "object"}));
        assert!(tool.annotations.is_none());
    }

    #[test]
    fn tool_definition_omits_none_fields() {
        let tool = ToolDefinition {
            name: "x".to_string(),
            description: None,
            input_schema: default_object_schema(),
            annotations: None,
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(!json.contains("\"description\""));
        assert!(!json.contains("\"annotations\""));
    }

    #[test]
    fn annotations_omits_none_hints() {
        let ann = ToolAnnotations {
            read_only_hint: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_string(&ann).unwrap();
        assert!(json.contains("readOnlyHint"));
        assert!(!json.contains("idempotentHint"));
        assert!(!json.contains("destructiveHint"));
        assert!(!json.contains("openWorldHint"));
    }

    #[test]
    fn content_item_text() {
        let item: ContentItem = serde_json::from_value(json!({
            "type": "text",
            "text": "hello"
        }))
        .unwrap();
        match item {
            ContentItem::Text(t) => assert_eq!(t.text, "hello"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn content_item_image_roundtrip() {
        let original = ImageContent {
            data: b"\x89PNG\r\n".to_vec(),
            mime_type: "image/png".to_string(),
        };
        let json = serde_json::to_value(&ContentItem::Image(original.clone())).unwrap();
        assert_eq!(json["data"], "iVBORw0K");
        assert_eq!(json["mimeType"], "image/png");

        let item: ContentItem = serde_json::from_value(json).unwrap();
        match item {
            ContentItem::Image(i) => {
                assert_eq!(i.data, b"\x89PNG\r\n");
                assert_eq!(i.mime_type, "image/png");
            }
            _ => panic!("expected image"),
        }
    }

    #[test]
    fn content_item_resource_text() {
        let item: ContentItem = serde_json::from_value(json!({
            "type": "resource",
            "resource": {
                "uri": "file:///tmp/test.txt",
                "text": "contents",
                "mimeType": "text/plain"
            }
        }))
        .unwrap();
        match item {
            ContentItem::Resource(r) => {
                assert_eq!(r.resource.uri, "file:///tmp/test.txt");
                assert_eq!(r.resource.text.as_deref(), Some("contents"));
                assert!(r.resource.blob.is_none());
            }
            _ => panic!("expected resource"),
        }
    }

    #[test]
    fn content_item_resource_blob_roundtrip() {
        let resource = EmbeddedResource {
            uri: "file:///tmp/data.bin".to_string(),
            mime_type: Some("application/octet-stream".to_string()),
            text: None,
            blob: Some(b"\x00\x01\x02".to_vec()),
        };
        let json = serde_json::to_value(&ResourceContent { resource }).unwrap();
        assert_eq!(json["resource"]["blob"], "AAEC");
        assert!(json["resource"].get("text").is_none());

        let item: ContentItem = serde_json::from_value(json!({
            "type": "resource",
            "resource": {
                "uri": "file:///tmp/data.bin",
                "blob": "AAEC",
                "mimeType": "application/octet-stream"
            }
        }))
        .unwrap();
        match item {
            ContentItem::Resource(r) => {
                assert_eq!(r.resource.blob.as_deref(), Some(b"\x00\x01\x02".as_slice()));
            }
            _ => panic!("expected resource"),
        }
    }

    #[test]
    fn call_tool_result_with_error() {
        let result: CallToolResult = serde_json::from_value(json!({
            "content": [{ "type": "text", "text": "oops" }],
            "isError": true
        }))
        .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(result.content.len(), 1);
    }

    #[test]
    fn call_tool_result_omits_is_error_when_none() {
        let result = CallToolResult {
            content: vec![],
            is_error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("isError"));
    }

    #[test]
    fn call_tool_params_serialize() {
        let params = CallToolParams {
            name: "test".to_string(),
            arguments: Some(json!({"key": "value"})),
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["arguments"]["key"], "value");
    }

    #[test]
    fn initialize_result_serialize() {
        let result = InitializeResult {
            protocol_version: "2025-11-25".to_string(),
            server_info: ServerInfo {
                name: "test".to_string(),
                version: Some("1.0".to_string()),
            },
            capabilities: Some(ServerCapabilities {
                tools: Some(json!({})),
                ..Default::default()
            }),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["protocolVersion"], "2025-11-25");
        assert_eq!(json["serverInfo"]["name"], "test");
        assert!(json["capabilities"]["tools"].is_object());
        assert!(json["capabilities"].get("resources").is_none());
    }
}
