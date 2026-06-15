//! `Bytes` — a binary field that travels as a CBOR byte string (major type 2)
//! and advertises `contentEncoding: base64` in JSON Schema. See the native
//! binary encoding design (§3 M2, §5.1).

/// A binary value. Serializes to a CBOR byte string; deserializes from a CBOR
/// byte string **or** a base64-encoded text string (the bare-base64 leniency for
/// schema-typed fields on JSON transports). Advertises `contentEncoding: base64`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bytes(pub Vec<u8>);

impl From<Vec<u8>> for Bytes {
    fn from(v: Vec<u8>) -> Self {
        Bytes(v)
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(b: Bytes) -> Self {
        b.0
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl serde::Serialize for Bytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl crate::response::IntoResponse for Bytes {
    fn into_tool_events(self, default_language: &str) -> Vec<crate::context::RawToolEvent> {
        // Delegate to Vec<u8> → application/octet-stream (raw binary output).
        self.0.into_tool_events(default_language)
    }
}

impl<'de> serde::Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BytesVisitor;

        impl serde::de::Visitor<'_> for BytesVisitor {
            type Value = Bytes;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a CBOR byte string or a base64-encoded string")
            }

            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Bytes, E> {
                Ok(Bytes(v.to_vec()))
            }

            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Bytes, E> {
                Ok(Bytes(v))
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Bytes, E> {
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD
                    .decode(v)
                    .map(Bytes)
                    .map_err(|e| E::custom(format!("invalid base64 for Bytes: {e}")))
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Bytes, E> {
                self.visit_str(&v)
            }
        }

        deserializer.deserialize_any(BytesVisitor)
    }
}

impl schemars::JsonSchema for Bytes {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Bytes".into()
    }

    // Always inline so a `Bytes` field shows the marker directly in the tool's
    // parameters-schema, rather than a `$ref` into `$defs`.
    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "contentEncoding": "base64",
            "contentMediaType": "application/octet-stream"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cbor_of(v: &ciborium::value::Value) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(v, &mut buf).unwrap();
        buf
    }

    #[test]
    fn serializes_to_cbor_byte_string() {
        let mut buf = Vec::new();
        ciborium::into_writer(&Bytes(b"hello".to_vec()), &mut buf).unwrap();
        let value: ciborium::value::Value = ciborium::from_reader(&buf[..]).unwrap();
        assert_eq!(value, ciborium::value::Value::Bytes(b"hello".to_vec()));
    }

    #[test]
    fn deserializes_from_cbor_byte_string() {
        let cbor = cbor_of(&ciborium::value::Value::Bytes(b"hi".to_vec()));
        let b: Bytes = ciborium::from_reader(&cbor[..]).unwrap();
        assert_eq!(b.0, b"hi");
    }

    #[test]
    fn deserializes_from_base64_text() {
        let cbor = cbor_of(&ciborium::value::Value::Text("aGVsbG8=".into()));
        let b: Bytes = ciborium::from_reader(&cbor[..]).unwrap();
        assert_eq!(b.0, b"hello");
    }

    #[test]
    fn rejects_invalid_base64_text() {
        let cbor = cbor_of(&ciborium::value::Value::Text("@@@".into()));
        let r: Result<Bytes, _> = ciborium::from_reader(&cbor[..]);
        assert!(r.is_err());
    }

    #[test]
    fn rejects_unpadded_base64() {
        // "aGVsbG8" is "hello" base64 minus its "=" padding → rejected (strict canonical).
        let cbor = cbor_of(&ciborium::value::Value::Text("aGVsbG8".into()));
        let r: Result<Bytes, _> = ciborium::from_reader(&cbor[..]);
        assert!(r.is_err());
    }

    #[test]
    fn bytes_response_is_octet_stream() {
        use crate::response::IntoResponse;
        let events = Bytes(b"\x89PNG".to_vec()).into_tool_events("en");
        match events.into_iter().next().unwrap() {
            crate::context::RawToolEvent::Content {
                data, mime_type, ..
            } => {
                assert_eq!(
                    mime_type.as_deref(),
                    Some(crate::constants::MIME_OCTET_STREAM)
                );
                assert_eq!(data, b"\x89PNG");
            }
            _ => panic!("expected Content event"),
        }
    }

    #[test]
    fn schema_advertises_base64() {
        let schema = schemars::schema_for!(Bytes);
        let v = serde_json::to_value(&schema).unwrap();
        assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("string"));
        assert_eq!(
            v.get("contentEncoding").and_then(|x| x.as_str()),
            Some("base64")
        );
        assert_eq!(
            v.get("contentMediaType").and_then(|x| x.as_str()),
            Some("application/octet-stream")
        );
    }

    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct Params {
        data: Bytes,
    }

    #[test]
    fn bytes_field_composes_with_derive() {
        let schema = schemars::schema_for!(Params);
        let v = serde_json::to_value(&schema).unwrap();
        assert_eq!(v["properties"]["data"]["contentEncoding"], "base64");

        let cbor = cbor_of(&ciborium::value::Value::Map(vec![(
            ciborium::value::Value::Text("data".into()),
            ciborium::value::Value::Bytes(b"hi".to_vec()),
        )]));
        let p: Params = ciborium::from_reader(&cbor[..]).unwrap();
        assert_eq!(p.data.0, b"hi");
    }
}
