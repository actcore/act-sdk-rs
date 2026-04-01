/// Raw stream event before conversion to WIT types.
/// Used internally by ActContext; the generated code converts to WIT StreamEvent.
pub enum RawStreamEvent {
    Content {
        data: Vec<u8>,
        mime_type: Option<String>,
        metadata: Vec<(String, Vec<u8>)>,
    },
    Error {
        kind: String,
        message: String,
        default_language: String,
    },
}

/// Context passed to tool functions. Provides metadata access and stream writing.
///
/// Events are buffered in memory via `send_text()`/`send_content()` and
/// written to the WIT stream after the tool function returns.
pub struct ActContext<C = ()> {
    metadata: C,
    events: Vec<RawStreamEvent>,
}

impl<C> ActContext<C> {
    #[doc(hidden)]
    pub fn __new(metadata: C) -> Self {
        Self {
            metadata,
            events: Vec::new(),
        }
    }

    /// Access the deserialized metadata.
    pub fn metadata(&self) -> &C {
        &self.metadata
    }

    /// Send a text content event (buffered).
    pub fn send_text(&mut self, text: impl Into<String>) {
        self.send_content(
            text.into().into_bytes(),
            Some(crate::constants::MIME_TEXT.to_string()),
            vec![],
        );
    }

    /// Send a CBOR-encoded content event (buffered).
    pub fn send_cbor<T: serde::Serialize>(&mut self, value: &T) {
        let mut buf = Vec::new();
        ciborium::into_writer(value, &mut buf).expect("CBOR serialization should not fail");
        self.send_content(buf, Some(crate::constants::MIME_CBOR.to_string()), vec![]);
    }

    /// Send a JSON-encoded content event (buffered).
    pub fn send_json<T: serde::Serialize>(&mut self, value: &T) {
        let data = serde_json::to_vec(value).unwrap_or_default();
        self.send_content(data, Some(crate::constants::MIME_JSON.to_string()), vec![]);
    }

    /// Send a content event with explicit data, MIME type, and metadata (buffered).
    pub fn send_content(
        &mut self,
        data: Vec<u8>,
        mime_type: Option<String>,
        metadata: Vec<(String, Vec<u8>)>,
    ) {
        self.events.push(RawStreamEvent::Content {
            data,
            mime_type,
            metadata,
        });
    }

    /// Drain all buffered events. Called by generated code.
    #[doc(hidden)]
    pub fn __take_events(&mut self) -> Vec<RawStreamEvent> {
        std::mem::take(&mut self.events)
    }
}
