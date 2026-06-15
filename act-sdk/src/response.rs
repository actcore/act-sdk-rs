use crate::context::RawToolEvent;

/// Trait for types that can be converted into stream events.
///
/// Specific impls use a by-value `self` receiver so that method resolution
/// prefers them over the [`IntoToolResponseViaSerialize`] blanket (which uses
/// `&self`). This is the stable "autoref specialization" pattern.
///
/// To return a custom content type, implement this trait on your type with a
/// by-value receiver. Do NOT implement [`IntoToolResponseViaSerialize`] — the
/// blanket impl already covers every [`serde::Serialize`] type (→ CBOR).
pub trait IntoToolResponse {
    fn into_tool_response(self, default_language: &str) -> Vec<RawToolEvent>;
}

impl IntoToolResponse for String {
    fn into_tool_response(self, _default_language: &str) -> Vec<RawToolEvent> {
        vec![RawToolEvent::Content {
            data: self.into_bytes(),
            mime_type: Some(crate::constants::MIME_TEXT.to_string()),
            metadata: vec![],
        }]
    }
}

impl IntoToolResponse for &str {
    fn into_tool_response(self, default_language: &str) -> Vec<RawToolEvent> {
        self.to_string().into_tool_response(default_language)
    }
}

impl IntoToolResponse for () {
    fn into_tool_response(self, _default_language: &str) -> Vec<RawToolEvent> {
        vec![]
    }
}

impl IntoToolResponse for Vec<u8> {
    fn into_tool_response(self, _default_language: &str) -> Vec<RawToolEvent> {
        vec![RawToolEvent::Content {
            data: self,
            mime_type: Some(crate::constants::MIME_OCTET_STREAM.to_string()),
            metadata: vec![],
        }]
    }
}

/// Wrapper that serializes the inner value as JSON with `application/json` MIME type.
pub struct Json<T>(pub T);

impl<T: serde::Serialize> IntoToolResponse for Json<T> {
    fn into_tool_response(self, _default_language: &str) -> Vec<RawToolEvent> {
        vec![RawToolEvent::Content {
            data: serde_json::to_vec(&self.0).unwrap_or_default(),
            mime_type: Some(crate::constants::MIME_JSON.to_string()),
            metadata: vec![],
        }]
    }
}

/// Wrapper for returning content with an explicit MIME type.
///
/// The first field is the MIME type string, the second is the raw data.
pub struct Content(pub &'static str, pub Vec<u8>);

impl IntoToolResponse for Content {
    fn into_tool_response(self, _default_language: &str) -> Vec<RawToolEvent> {
        vec![RawToolEvent::Content {
            data: self.1,
            mime_type: Some(self.0.to_string()),
            metadata: vec![],
        }]
    }
}

/// Autoref-specialization fallback: any `Serialize` value that has no specific
/// `IntoToolResponse` impl is CBOR-encoded. The `&self` receiver makes method
/// resolution prefer a by-value `IntoToolResponse` impl when one exists.
#[doc(hidden)]
pub trait IntoToolResponseViaSerialize {
    #[allow(clippy::wrong_self_convention)]
    fn into_tool_response(&self, default_language: &str) -> Vec<RawToolEvent>;
}

impl<T: serde::Serialize> IntoToolResponseViaSerialize for T {
    fn into_tool_response(&self, default_language: &str) -> Vec<RawToolEvent> {
        cbor_encode_response(self, default_language)
    }
}

/// Encode a serializable value as CBOR and wrap it as a stream event.
///
/// Called by [`IntoToolResponseViaSerialize`]'s blanket impl and available
/// as a direct helper for generated code.
pub fn cbor_encode_response<T: serde::Serialize>(
    val: &T,
    _default_language: &str,
) -> Vec<RawToolEvent> {
    let mut buf = Vec::new();
    ciborium::into_writer(val, &mut buf).expect("CBOR serialization should not fail");
    vec![RawToolEvent::Content {
        data: buf,
        mime_type: Some(crate::constants::MIME_CBOR.to_string()),
        metadata: vec![],
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn extract(events: Vec<RawToolEvent>) -> (Vec<u8>, Option<String>) {
        match events.into_iter().next().unwrap() {
            RawToolEvent::Content {
                data, mime_type, ..
            } => (data, mime_type),
            _ => panic!("expected Content event"),
        }
    }

    #[test]
    fn string_is_text_plain() {
        let (data, mime) = extract("hello".to_string().into_tool_response("en"));
        assert_eq!(mime.as_deref(), Some(crate::constants::MIME_TEXT));
        assert_eq!(data, b"hello");
    }

    #[test]
    fn str_ref_is_text_plain() {
        let s: &str = "hello";
        let (data, mime) = extract(s.into_tool_response("en"));
        assert_eq!(mime.as_deref(), Some(crate::constants::MIME_TEXT));
        assert_eq!(data, b"hello");
    }

    #[test]
    fn unit_is_empty() {
        assert!(().into_tool_response("en").is_empty());
    }

    #[test]
    fn vec_u8_is_octet_stream() {
        let (_d, mime) = extract(vec![1u8, 2, 3].into_tool_response("en"));
        assert_eq!(mime.as_deref(), Some(crate::constants::MIME_OCTET_STREAM));
    }

    #[test]
    fn content_keeps_explicit_mime() {
        let (data, mime) = extract(Content("image/png", vec![0x89]).into_tool_response("en"));
        assert_eq!(mime.as_deref(), Some("image/png"));
        assert_eq!(data, vec![0x89]);
    }

    #[test]
    fn json_wrapper_is_json_mime() {
        let value = json!({"rows": [1, 2, 3]});
        let (data, mime) = extract(Json(value.clone()).into_tool_response("en"));
        assert_eq!(mime.as_deref(), Some(crate::constants::MIME_JSON));
        let parsed: serde_json::Value = serde_json::from_slice(&data).unwrap();
        assert_eq!(parsed, value);
    }

    #[test]
    fn serialize_struct_falls_back_to_cbor() {
        #[derive(serde::Serialize)]
        struct S {
            n: u32,
        }
        let (data, mime) = extract(S { n: 7 }.into_tool_response("en"));
        assert_eq!(mime.as_deref(), Some(crate::constants::MIME_CBOR));
        let v: ciborium::value::Value = ciborium::from_reader(&data[..]).unwrap();
        assert_eq!(
            v,
            ciborium::value::Value::Map(vec![(
                ciborium::value::Value::Text("n".into()),
                ciborium::value::Value::Integer(7.into())
            )])
        );
    }

    #[test]
    fn bytes_falls_back_to_cbor_byte_string() {
        let (data, mime) = extract(crate::bytes::Bytes(b"hi".to_vec()).into_tool_response("en"));
        assert_eq!(mime.as_deref(), Some(crate::constants::MIME_CBOR));
        let v: ciborium::value::Value = ciborium::from_reader(&data[..]).unwrap();
        assert_eq!(v, ciborium::value::Value::Bytes(b"hi".to_vec()));
    }
}
