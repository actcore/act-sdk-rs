use crate::context::RawStreamEvent;

/// Trait for types that can be converted into stream events.
pub trait IntoResponse {
    fn into_stream_events(self, default_language: &str) -> Vec<RawStreamEvent>;
}

impl IntoResponse for String {
    fn into_stream_events(self, _default_language: &str) -> Vec<RawStreamEvent> {
        vec![RawStreamEvent::Content {
            data: self.into_bytes(),
            mime_type: Some("text/plain".to_string()),
            metadata: vec![],
        }]
    }
}

impl IntoResponse for &str {
    fn into_stream_events(self, default_language: &str) -> Vec<RawStreamEvent> {
        self.to_string().into_stream_events(default_language)
    }
}

impl IntoResponse for () {
    fn into_stream_events(self, _default_language: &str) -> Vec<RawStreamEvent> {
        vec![]
    }
}

impl IntoResponse for Vec<u8> {
    fn into_stream_events(self, _default_language: &str) -> Vec<RawStreamEvent> {
        vec![RawStreamEvent::Content {
            data: self,
            mime_type: Some("application/octet-stream".to_string()),
            metadata: vec![],
        }]
    }
}

/// Wrapper that serializes the inner value as JSON with `application/json` MIME type.
pub struct Json<T>(pub T);

impl<T: serde::Serialize> IntoResponse for Json<T> {
    fn into_stream_events(self, _default_language: &str) -> Vec<RawStreamEvent> {
        vec![RawStreamEvent::Content {
            data: serde_json::to_vec(&self.0).unwrap_or_default(),
            mime_type: Some("application/json".to_string()),
            metadata: vec![],
        }]
    }
}

/// Wrapper for returning content with an explicit MIME type.
///
/// The first field is the MIME type string, the second is the raw data.
pub struct Content(pub &'static str, pub Vec<u8>);

impl IntoResponse for Content {
    fn into_stream_events(self, _default_language: &str) -> Vec<RawStreamEvent> {
        vec![RawStreamEvent::Content {
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
) -> Vec<RawStreamEvent> {
    let mut buf = Vec::new();
    ciborium::into_writer(val, &mut buf).expect("CBOR serialization should not fail");
    vec![RawStreamEvent::Content {
        data: buf,
        mime_type: Some("application/cbor".to_string()),
        metadata: vec![],
    }]
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn extract_content(events: Vec<RawStreamEvent>) -> (Vec<u8>, Option<String>) {
        match events.into_iter().next().unwrap() {
            RawStreamEvent::Content {
                data, mime_type, ..
            } => (data, mime_type),
            _ => panic!("expected Content event"),
        }
    }

    #[test]
    fn json_wrapper_produces_json_mime() {
        let value = json!({"rows": [1, 2, 3]});
        let events = Json(value.clone()).into_stream_events("en");
        let (data, mime) = extract_content(events);
        assert_eq!(mime.as_deref(), Some("application/json"));
        let parsed: serde_json::Value = serde_json::from_slice(&data).unwrap();
        assert_eq!(parsed, value);
    }

    #[test]
    fn content_wrapper_explicit_mime() {
        let png_header = vec![0x89, 0x50, 0x4E, 0x47];
        let events = Content("image/png", png_header.clone()).into_stream_events("en");
        let (data, mime) = extract_content(events);
        assert_eq!(mime.as_deref(), Some("image/png"));
        assert_eq!(data, png_header);
    }

    #[test]
    fn cbor_encode_produces_cbor_mime() {
        let input = vec![1u32, 2, 3];
        let events = cbor_encode_response(&input, "en");
        let (data, mime) = extract_content(events);
        assert_eq!(mime.as_deref(), Some("application/cbor"));
        let decoded: Vec<u32> = ciborium::from_reader(&data[..]).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn vec_u8_produces_octet_stream() {
        let events = vec![1u8, 2, 3].into_stream_events("en");
        let (_data, mime) = extract_content(events);
        assert_eq!(mime.as_deref(), Some("application/octet-stream"));
    }

    #[test]
    fn string_still_produces_text_plain() {
        let events = "hello".to_string().into_stream_events("en");
        let (data, mime) = extract_content(events);
        assert_eq!(mime.as_deref(), Some("text/plain"));
        assert_eq!(data, b"hello");
    }
}
