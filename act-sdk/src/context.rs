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
            Some("text/plain".to_string()),
            vec![],
        );
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
