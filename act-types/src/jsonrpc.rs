//! JSON-RPC 2.0 wire-format types.
//!
//! Protocol-agnostic JSON-RPC envelope types used by MCP and potentially
//! other JSON-RPC-based transports.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::skip_serializing_none;

/// JSON-RPC protocol version. Always `"2.0"`.
///
/// Single-variant enum: zero-size, always serializes as `"2.0"`,
/// rejects other values on deserialization.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Version {
    #[default]
    #[serde(rename = "2.0")]
    V2,
}

/// A JSON-RPC 2.0 request.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub jsonrpc: Version,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// A JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    #[serde(default)]
    pub jsonrpc: Version,
    pub id: Value,
    #[serde(flatten)]
    pub body: Body,
}

/// The result or error payload of a JSON-RPC response.
///
/// Uses `#[serde(flatten)]` with the response so exactly one of
/// `"result"` or `"error"` appears in the JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Body {
    Result(Value),
    Error(Error),
}

/// A JSON-RPC error object.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

impl Response {
    /// Create a successful response.
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: Version::V2,
            id,
            body: Body::Result(result),
        }
    }

    /// Create an error response.
    pub fn error(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: Version::V2,
            id,
            body: Body::Error(Error {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn response_success_roundtrip() {
        let resp = Response::success(json!(1), json!({"tools": []}));
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert!(json.get("result").is_some());
        assert!(json.get("error").is_none());
    }

    #[test]
    fn response_error_roundtrip() {
        let resp = Response::error(json!(1), -32601, "not found");
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["error"]["code"], -32601);
        assert_eq!(json["error"]["message"], "not found");
        assert!(json.get("result").is_none());
    }

    #[test]
    fn error_omits_null_data() {
        let resp = Response::error(json!(1), -32600, "bad");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("\"data\""));
    }

    #[test]
    fn request_omits_null_fields() {
        let req = Request {
            jsonrpc: Version::V2,
            id: None,
            method: "ping".to_string(),
            params: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("\"id\""));
        assert!(!json.contains("\"params\""));
    }

    #[test]
    fn version_rejects_invalid() {
        let result: Result<Version, _> = serde_json::from_value(json!("1.0"));
        assert!(result.is_err());
    }

    #[test]
    fn version_default() {
        assert_eq!(Version::default(), Version::V2);
    }
}
