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
            mime_type: None,
            metadata: vec![],
        }]
    }
}

impl IntoResponse for serde_json::Value {
    fn into_stream_events(self, _default_language: &str) -> Vec<RawStreamEvent> {
        vec![RawStreamEvent::Content {
            data: serde_json::to_vec(&self).unwrap_or_default(),
            mime_type: Some("application/json".to_string()),
            metadata: vec![],
        }]
    }
}
