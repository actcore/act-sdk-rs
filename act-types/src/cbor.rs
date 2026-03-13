// CBOR <-> JSON/serde conversion utilities.
//
// Note: ciborium produces standard CBOR, not strict dCBOR (RFC 8949 §4.2).
// For JSON-originating data this is practically deterministic, but does not
// guarantee shortest integer encoding, sorted map keys, or preferred floats.

/// Encode a serializable value as CBOR bytes.
pub fn to_cbor<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf).expect("CBOR serialization should not fail");
    buf
}

/// Decode CBOR bytes into a deserializable value.
pub fn from_cbor<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, CborError> {
    ciborium::from_reader(bytes).map_err(|e| CborError(format!("CBOR decode failed: {e}")))
}

/// Convert a JSON value to CBOR bytes.
pub fn json_to_cbor(value: &serde_json::Value) -> Result<Vec<u8>, CborError> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf)
        .map_err(|e| CborError(format!("JSON→CBOR encode failed: {e}")))?;
    Ok(buf)
}

/// Convert CBOR bytes to a JSON value.
pub fn cbor_to_json(bytes: &[u8]) -> Result<serde_json::Value, CborError> {
    ciborium::from_reader(bytes).map_err(|e| CborError(format!("CBOR→JSON decode failed: {e}")))
}

/// Decode content-part data based on MIME type.
///
/// - `text/*` — raw UTF-8 bytes → JSON string
/// - everything else — CBOR-decoded to JSON, with base64 fallback for invalid CBOR
pub fn decode_content_data(data: &[u8], mime_type: Option<&str>) -> serde_json::Value {
    let mime = mime_type.unwrap_or("application/cbor");
    if mime.starts_with("text/") {
        serde_json::Value::String(String::from_utf8_lossy(data).into_owned())
    } else {
        cbor_to_json(data).unwrap_or_else(|_| {
            use base64::Engine as _;
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(data))
        })
    }
}

/// CBOR conversion error.
#[derive(Debug, Clone)]
pub struct CborError(pub String);

impl std::fmt::Display for CborError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CborError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip_object() {
        let input = json!({"a": 2, "b": 3});
        let cbor = json_to_cbor(&input).unwrap();
        let output = cbor_to_json(&cbor).unwrap();
        assert_eq!(input, output);
    }

    #[test]
    fn roundtrip_nested() {
        let input = json!({"config": {"api_key": "abc123"}, "values": [1, 2, 3]});
        let cbor = json_to_cbor(&input).unwrap();
        let output = cbor_to_json(&cbor).unwrap();
        assert_eq!(input, output);
    }

    #[test]
    fn roundtrip_null() {
        let input = json!(null);
        let cbor = json_to_cbor(&input).unwrap();
        let output = cbor_to_json(&cbor).unwrap();
        assert_eq!(input, output);
    }

    #[test]
    fn empty_bytes_is_error() {
        assert!(cbor_to_json(&[]).is_err());
    }

    #[test]
    fn generic_roundtrip() {
        let input = 42u64;
        let bytes = to_cbor(&input);
        let output: u64 = from_cbor(&bytes).unwrap();
        assert_eq!(input, output);
    }

    #[test]
    fn decode_text_content() {
        let data = b"hello world";
        let result = decode_content_data(data, Some("text/plain"));
        assert_eq!(result, json!("hello world"));
    }

    #[test]
    fn decode_cbor_content() {
        let data = to_cbor(&json!({"key": "value"}));
        let result = decode_content_data(&data, None);
        assert_eq!(result, json!({"key": "value"}));
    }

    #[test]
    fn decode_invalid_cbor_falls_back_to_base64() {
        let data = b"\xff\xfe";
        let result = decode_content_data(data, Some("application/octet-stream"));
        // Should be base64 string
        assert!(result.is_string());
    }
}
