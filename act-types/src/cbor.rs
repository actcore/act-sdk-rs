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

/// Convert a JSON value to a CBOR value, decoding the canonical `{"$bytes":…}`
/// wrapper to a byte string and unescaping `$$`-prefixed map keys.
fn json_to_cbor_value(v: &serde_json::Value) -> Result<ciborium::value::Value, CborError> {
    use ciborium::value::Value as C;
    Ok(match v {
        serde_json::Value::Null => C::Null,
        serde_json::Value::Bool(b) => C::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                C::Integer(i.into())
            } else if let Some(u) = n.as_u64() {
                C::Integer(u.into())
            } else if let Some(f) = n.as_f64() {
                C::Float(f)
            } else {
                return Err(CborError("unrepresentable JSON number".into()));
            }
        }
        serde_json::Value::String(s) => C::Text(s.clone()),
        serde_json::Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(json_to_cbor_value(item)?);
            }
            C::Array(out)
        }
        serde_json::Value::Object(map) => {
            if let Some(bytes) = decode_bytes_wrapper(map)? {
                C::Bytes(bytes)
            } else {
                let mut entries = Vec::with_capacity(map.len());
                for (k, val) in map {
                    entries.push((C::Text(unescape_key(k)), json_to_cbor_value(val)?));
                }
                C::Map(entries)
            }
        }
    })
}

/// Convert a JSON value to CBOR bytes.
pub fn json_to_cbor(value: &serde_json::Value) -> Result<Vec<u8>, CborError> {
    let cbor = json_to_cbor_value(value)?;
    let mut buf = Vec::new();
    ciborium::into_writer(&cbor, &mut buf)
        .map_err(|e| CborError(format!("JSON→CBOR encode failed: {e}")))?;
    Ok(buf)
}

/// Convert a decoded CBOR value to a JSON value, wrapping byte strings as the
/// canonical `{"$bytes": "<base64>"}` and escaping literal `$`-prefixed map keys.
fn cbor_value_to_json(v: &ciborium::value::Value) -> Result<serde_json::Value, CborError> {
    use ciborium::value::Value as C;
    Ok(match v {
        C::Null => serde_json::Value::Null,
        C::Bool(b) => serde_json::Value::Bool(*b),
        C::Integer(i) => {
            let n: i128 = i128::from(*i);
            if let Ok(i) = i64::try_from(n) {
                serde_json::Value::Number(i.into())
            } else if let Ok(u) = u64::try_from(n) {
                serde_json::Value::Number(u.into())
            } else {
                return Err(CborError(format!(
                    "CBOR integer {n} is outside JSON-safe range"
                )));
            }
        }
        C::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .ok_or_else(|| CborError(format!("non-finite float cannot project to JSON: {f}")))?,
        C::Text(s) => serde_json::Value::String(s.clone()),
        C::Bytes(b) => {
            use base64::Engine as _;
            let mut obj = serde_json::Map::new();
            obj.insert(
                "$bytes".to_string(),
                serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(b)),
            );
            serde_json::Value::Object(obj)
        }
        C::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(cbor_value_to_json(item)?);
            }
            serde_json::Value::Array(out)
        }
        C::Map(entries) => {
            let mut obj = serde_json::Map::new();
            for (k, val) in entries {
                let key = match k {
                    C::Text(s) => escape_key(s),
                    _ => {
                        return Err(CborError(
                            "non-string CBOR map key cannot project to JSON".into(),
                        ));
                    }
                };
                obj.insert(key, cbor_value_to_json(val)?);
            }
            serde_json::Value::Object(obj)
        }
        // CBOR tags are stripped; the tagged value's content is preserved. TODO: handle known tags (e.g. datetime).
        C::Tag(_, inner) => cbor_value_to_json(inner)?,
        _ => return Err(CborError("unsupported CBOR value type".into())),
    })
}

/// Convert CBOR bytes to a JSON value.
pub fn cbor_to_json(bytes: &[u8]) -> Result<serde_json::Value, CborError> {
    let value: ciborium::value::Value = ciborium::from_reader(bytes)
        .map_err(|e| CborError(format!("CBOR→JSON decode failed: {e}")))?;
    cbor_value_to_json(&value)
}

/// Decode content-part data based on MIME type for JSON representation.
///
/// - `text/*`, `application/json`, `application/xml` — raw UTF-8 bytes → JSON string
/// - `application/cbor` — CBOR-decoded to JSON value
/// - everything else (image/*, octet-stream, etc.) — base64-encoded string
pub fn decode_content_data(data: &[u8], mime_type: Option<&str>) -> serde_json::Value {
    let mime = mime_type.unwrap_or("application/cbor");

    if mime.starts_with("text/") || mime == "application/json" || mime == "application/xml" {
        // Text-like: inline as string
        serde_json::Value::String(String::from_utf8_lossy(data).into_owned())
    } else if mime == "application/cbor" {
        // Structured: CBOR → JSON value
        cbor_to_json(data).unwrap_or_else(|_| {
            use base64::Engine as _;
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(data))
        })
    } else {
        // Binary (image/*, octet-stream, pdf, etc.): base64
        use base64::Engine as _;
        serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(data))
    }
}

/// Escape a literal CBOR map key that begins with `$` by prepending one more `$`.
/// Keeps the `$`-prefixed namespace reserved for ACT JSON-projection tokens.
fn escape_key(k: &str) -> String {
    if k.starts_with('$') {
        format!("${k}")
    } else {
        k.to_string()
    }
}

/// Inverse of `escape_key`: a key beginning with `$$` is unescaped by one `$`.
fn unescape_key(k: &str) -> String {
    if k.starts_with("$$") {
        k[1..].to_string()
    } else {
        k.to_string()
    }
}

/// If `map` is exactly the canonical byte-string wrapper `{"$bytes": "<base64>"}`,
/// return the decoded bytes. `Ok(None)` for any other object shape. Invalid base64
/// inside a single-key `$bytes` object is a hard error (the shape is reserved).
fn decode_bytes_wrapper(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<Vec<u8>>, CborError> {
    if map.len() != 1 {
        return Ok(None);
    }
    let Some(value) = map.get("$bytes") else {
        return Ok(None);
    };
    // Single-member `$bytes` object is the reserved byte-string wrapper; its value
    // MUST be a base64 string, otherwise the input is a malformed wrapper.
    let serde_json::Value::String(b64) = value else {
        return Err(CborError(
            "$bytes wrapper value must be a base64 string".into(),
        ));
    };
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| CborError(format!("invalid base64 in $bytes: {e}")))?;
    Ok(Some(bytes))
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
    fn escape_roundtrip_dollar_keys() {
        assert_eq!(escape_key("name"), "name");
        assert_eq!(escape_key("$bytes"), "$$bytes");
        assert_eq!(escape_key("$$x"), "$$$x");
        assert_eq!(unescape_key("name"), "name");
        assert_eq!(unescape_key("$$bytes"), "$bytes");
        assert_eq!(unescape_key("$$$x"), "$$x");
    }

    #[test]
    fn bytes_wrapper_detected() {
        let mut m = serde_json::Map::new();
        m.insert(
            "$bytes".into(),
            serde_json::Value::String("aGVsbG8=".into()),
        );
        assert_eq!(decode_bytes_wrapper(&m).unwrap(), Some(b"hello".to_vec()));

        let mut m2 = serde_json::Map::new();
        m2.insert(
            "$bytes".into(),
            serde_json::Value::String("aGVsbG8=".into()),
        );
        m2.insert("x".into(), serde_json::Value::Null);
        assert_eq!(decode_bytes_wrapper(&m2).unwrap(), None);

        let mut m3 = serde_json::Map::new();
        m3.insert("$bytes".into(), serde_json::Value::String("@@@".into()));
        assert!(decode_bytes_wrapper(&m3).is_err());
    }

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
    fn decode_json_content_as_text() {
        let data = br#"{"pets": [1, 2, 3]}"#;
        let result = decode_content_data(data, Some("application/json"));
        assert_eq!(result, json!(r#"{"pets": [1, 2, 3]}"#));
    }

    #[test]
    fn decode_invalid_cbor_falls_back_to_base64() {
        let data = b"\xff\xfe";
        let result = decode_content_data(data, Some("application/octet-stream"));
        // Should be base64 string
        assert!(result.is_string());
    }

    #[test]
    fn decode_image_content_to_base64() {
        let data = vec![0x89, 0x50, 0x4E, 0x47]; // PNG magic bytes
        let result = decode_content_data(&data, Some("image/png"));
        assert!(result.is_string());
        use base64::Engine as _;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(result.as_str().unwrap())
            .unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_octet_stream_to_base64() {
        let data = vec![0xFF, 0xFE, 0x00];
        let result = decode_content_data(&data, Some("application/octet-stream"));
        assert!(result.is_string());
        use base64::Engine as _;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(result.as_str().unwrap())
            .unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_html_as_text() {
        let data = b"<h1>Hello</h1>";
        let result = decode_content_data(data, Some("text/html"));
        assert_eq!(result, json!("<h1>Hello</h1>"));
    }

    #[test]
    fn decode_xml_as_text() {
        let data = b"<root><item/></root>";
        let result = decode_content_data(data, Some("application/xml"));
        assert_eq!(result, json!("<root><item/></root>"));
    }

    fn cbor_of(v: &ciborium::value::Value) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(v, &mut buf).unwrap();
        buf
    }

    #[test]
    fn cbor_bytes_projects_to_dollar_bytes() {
        let buf = cbor_of(&ciborium::value::Value::Bytes(b"hello".to_vec()));
        assert_eq!(cbor_to_json(&buf).unwrap(), json!({"$bytes": "aGVsbG8="}));
    }

    #[test]
    fn embedded_bytes_in_map_wrapped() {
        let v = ciborium::value::Value::Map(vec![
            (
                ciborium::value::Value::Text("name".into()),
                ciborium::value::Value::Text("x".into()),
            ),
            (
                ciborium::value::Value::Text("blob".into()),
                ciborium::value::Value::Bytes(vec![1, 2]),
            ),
        ]);
        assert_eq!(
            cbor_to_json(&cbor_of(&v)).unwrap(),
            json!({"name": "x", "blob": {"$bytes": "AQI="}})
        );
    }

    #[test]
    fn literal_dollar_key_is_escaped_on_output() {
        let v = ciborium::value::Value::Map(vec![(
            ciborium::value::Value::Text("$bytes".into()),
            ciborium::value::Value::Text("hello".into()),
        )]);
        assert_eq!(
            cbor_to_json(&cbor_of(&v)).unwrap(),
            json!({"$$bytes": "hello"})
        );
    }

    #[test]
    fn dollar_bytes_parses_to_cbor_bytes() {
        let cbor = json_to_cbor(&json!({"$bytes": "aGVsbG8="})).unwrap();
        let value: ciborium::value::Value = ciborium::from_reader(&cbor[..]).unwrap();
        assert_eq!(value, ciborium::value::Value::Bytes(b"hello".to_vec()));
    }

    #[test]
    fn bytes_roundtrip_through_json() {
        let original = cbor_of(&ciborium::value::Value::Bytes(vec![0u8, 1, 2, 255]));
        let json = cbor_to_json(&original).unwrap();
        let back = json_to_cbor(&json).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn escaped_key_roundtrip_through_json() {
        let v = ciborium::value::Value::Map(vec![(
            ciborium::value::Value::Text("$bytes".into()),
            ciborium::value::Value::Text("hello".into()),
        )]);
        let json = cbor_to_json(&cbor_of(&v)).unwrap();
        let back = json_to_cbor(&json).unwrap();
        let value: ciborium::value::Value = ciborium::from_reader(&back[..]).unwrap();
        assert_eq!(value, v);
    }

    #[test]
    fn content_cbor_embeds_dollar_bytes() {
        let data = cbor_of(&ciborium::value::Value::Map(vec![(
            ciborium::value::Value::Text("thumb".into()),
            ciborium::value::Value::Bytes(vec![1, 2]),
        )]));
        let result = decode_content_data(&data, Some("application/cbor"));
        assert_eq!(result, json!({"thumb": {"$bytes": "AQI="}}));
    }

    #[test]
    fn dollar_bytes_non_string_value_errors() {
        assert!(json_to_cbor(&json!({"$bytes": 42})).is_err());
    }

    #[test]
    fn non_finite_float_errors() {
        let cbor = cbor_of(&ciborium::value::Value::Float(f64::NAN));
        assert!(cbor_to_json(&cbor).is_err());
    }
}
