use crate::context::RawToolEvent;

/// Trait for types that can be converted into stream events.
pub trait IntoResponse {
    fn into_tool_events(self, default_language: &str) -> Vec<RawToolEvent>;
}

impl IntoResponse for String {
    fn into_tool_events(self, _default_language: &str) -> Vec<RawToolEvent> {
        vec![RawToolEvent::Content {
            data: self.into_bytes(),
            mime_type: Some(crate::constants::MIME_TEXT.to_string()),
            metadata: vec![],
        }]
    }
}

impl IntoResponse for &str {
    fn into_tool_events(self, default_language: &str) -> Vec<RawToolEvent> {
        self.to_string().into_tool_events(default_language)
    }
}

impl IntoResponse for () {
    fn into_tool_events(self, _default_language: &str) -> Vec<RawToolEvent> {
        vec![]
    }
}

impl IntoResponse for Vec<u8> {
    fn into_tool_events(self, _default_language: &str) -> Vec<RawToolEvent> {
        vec![RawToolEvent::Content {
            data: self,
            mime_type: Some(crate::constants::MIME_OCTET_STREAM.to_string()),
            metadata: vec![],
        }]
    }
}

/// Wrapper that serializes the inner value as JSON with `application/json` MIME type.
pub struct Json<T>(pub T);

impl<T: serde::Serialize> IntoResponse for Json<T> {
    fn into_tool_events(self, _default_language: &str) -> Vec<RawToolEvent> {
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

impl IntoResponse for Content {
    fn into_tool_events(self, _default_language: &str) -> Vec<RawToolEvent> {
        vec![RawToolEvent::Content {
            data: self.1,
            mime_type: Some(self.0.to_string()),
            metadata: vec![],
        }]
    }
}

/// Encode a serializable value as CBOR and wrap it as a stream event.
///
/// Used by the `#[act_tool]` macro for automatic CBOR serialization of
/// structured return types.
#[doc(hidden)]
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

    fn extract_content(events: Vec<RawToolEvent>) -> (Vec<u8>, Option<String>) {
        match events.into_iter().next().unwrap() {
            RawToolEvent::Content {
                data, mime_type, ..
            } => (data, mime_type),
            _ => panic!("expected Content event"),
        }
    }

    #[test]
    fn json_wrapper_produces_json_mime() {
        let value = json!({"rows": [1, 2, 3]});
        let events = Json(value.clone()).into_tool_events("en");
        let (data, mime) = extract_content(events);
        assert_eq!(mime.as_deref(), Some(crate::constants::MIME_JSON));
        let parsed: serde_json::Value = serde_json::from_slice(&data).unwrap();
        assert_eq!(parsed, value);
    }

    #[test]
    fn content_wrapper_explicit_mime() {
        let png_header = vec![0x89, 0x50, 0x4E, 0x47];
        let events = Content("image/png", png_header.clone()).into_tool_events("en");
        let (data, mime) = extract_content(events);
        assert_eq!(mime.as_deref(), Some("image/png"));
        assert_eq!(data, png_header);
    }

    #[test]
    fn cbor_encode_produces_cbor_mime() {
        let input = vec![1u32, 2, 3];
        let events = cbor_encode_response(&input, "en");
        let (data, mime) = extract_content(events);
        assert_eq!(mime.as_deref(), Some(crate::constants::MIME_CBOR));
        let decoded: Vec<u32> = ciborium::from_reader(&data[..]).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn vec_u8_produces_octet_stream() {
        let events = vec![1u8, 2, 3].into_tool_events("en");
        let (_data, mime) = extract_content(events);
        assert_eq!(mime.as_deref(), Some(crate::constants::MIME_OCTET_STREAM));
    }

    #[test]
    fn string_still_produces_text_plain() {
        let events = "hello".to_string().into_tool_events("en");
        let (data, mime) = extract_content(events);
        assert_eq!(mime.as_deref(), Some(crate::constants::MIME_TEXT));
        assert_eq!(data, b"hello");
    }
}
