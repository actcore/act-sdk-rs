/// Encode a serializable value as CBOR bytes.
pub fn to_cbor<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf).expect("CBOR serialization failed");
    buf
}

/// Decode CBOR bytes into a deserializable value.
pub fn from_cbor<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    ciborium::from_reader(bytes).map_err(|e| format!("CBOR deserialization failed: {e}"))
}
