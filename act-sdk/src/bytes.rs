//! `Bytes` — a binary field that travels as a CBOR byte string (major type 2),
//! projected to/from the canonical `{"$bytes":"<base64>"}` envelope on JSON
//! transports. See the native binary encoding design (§3, §5.1).

/// A binary value. Serializes to a CBOR byte string and deserializes **only**
/// from a CBOR byte string — i.e. the `{"$bytes":"<base64>"}` envelope on JSON
/// transports. A bare string is rejected: a string is text, not bytes.
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

impl<'de> serde::Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BytesVisitor;

        impl serde::de::Visitor<'_> for BytesVisitor {
            type Value = Bytes;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a CBOR byte string (a {\"$bytes\":…} envelope on JSON)")
            }

            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Bytes, E> {
                Ok(Bytes(v.to_vec()))
            }

            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Bytes, E> {
                Ok(Bytes(v))
            }
        }

        // deserialize_any dispatches a byte string to visit_bytes/visit_byte_buf;
        // any other type (text, number, …) hits the Visitor default and errors.
        deserializer.deserialize_any(BytesVisitor)
    }
}

impl schemars::JsonSchema for Bytes {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Bytes".into()
    }

    // Always inline so a `Bytes` field shows the envelope shape directly in the
    // tool's parameters-schema, rather than a `$ref` into `$defs`.
    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Binary travels only as the canonical {"$bytes":"<base64>"} envelope.
        schemars::json_schema!({
            "type": "object",
            "description": "Binary value as a base64 byte-string envelope.",
            "properties": {
                "$bytes": { "type": "string", "contentEncoding": "base64" }
            },
            "required": ["$bytes"],
            "additionalProperties": false
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
    fn rejects_text_string() {
        // A bare string is text, not bytes — Bytes accepts only a byte string.
        let cbor = cbor_of(&ciborium::value::Value::Text("aGVsbG8=".into()));
        let r: Result<Bytes, _> = ciborium::from_reader(&cbor[..]);
        assert!(r.is_err());
    }

    #[test]
    fn schema_is_bytes_envelope() {
        let schema = schemars::schema_for!(Bytes);
        let v = serde_json::to_value(&schema).unwrap();
        assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("object"));
        assert_eq!(
            v["properties"]["$bytes"]["contentEncoding"].as_str(),
            Some("base64")
        );
        assert_eq!(v["required"][0].as_str(), Some("$bytes"));
    }

    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct Params {
        data: Bytes,
    }

    #[test]
    fn bytes_field_composes_with_derive() {
        let schema = schemars::schema_for!(Params);
        let v = serde_json::to_value(&schema).unwrap();
        // The field's schema is the inlined $bytes envelope (no $ref).
        assert_eq!(
            v["properties"]["data"]["properties"]["$bytes"]["contentEncoding"],
            "base64"
        );

        // Decode: a CBOR map {"data": h'6869'} → Params { data: Bytes(b"hi") }.
        let cbor = cbor_of(&ciborium::value::Value::Map(vec![(
            ciborium::value::Value::Text("data".into()),
            ciborium::value::Value::Bytes(b"hi".to_vec()),
        )]));
        let p: Params = ciborium::from_reader(&cbor[..]).unwrap();
        assert_eq!(p.data.0, b"hi");
    }
}
