use act_types::cbor::to_cbor;
use act_types::constants::*;
use crate::types::ActResult;

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

/// Context passed to tool functions. Provides config access and stream writing.
#[allow(dead_code)]
pub struct ActContext<C = ()> {
    config: C,
    events: Vec<RawStreamEvent>,
    default_language: String,
}

impl<C> ActContext<C> {
    #[doc(hidden)]
    pub fn __new(config: C, default_language: String) -> Self {
        Self {
            config,
            events: Vec::new(),
            default_language,
        }
    }

    /// Access the deserialized config.
    pub fn config(&self) -> &C {
        &self.config
    }

    /// Send a text content event.
    pub async fn send_text(&mut self, text: impl Into<String>) -> ActResult<()> {
        self.events.push(RawStreamEvent::Content {
            data: text.into().into_bytes(),
            mime_type: Some("text/plain".to_string()),
            metadata: vec![],
        });
        Ok(())
    }

    /// Send a content part with explicit data, MIME type, and metadata.
    pub async fn send_content(
        &mut self,
        data: Vec<u8>,
        mime_type: Option<String>,
        metadata: Vec<(String, Vec<u8>)>,
    ) -> ActResult<()> {
        self.events.push(RawStreamEvent::Content {
            data,
            mime_type,
            metadata,
        });
        Ok(())
    }

    /// Send a text event with progress metadata.
    pub async fn send_progress(
        &mut self,
        current: u64,
        total: u64,
        text: impl Into<String>,
    ) -> ActResult<()> {
        self.events.push(RawStreamEvent::Content {
            data: text.into().into_bytes(),
            mime_type: Some("text/plain".to_string()),
            metadata: vec![
                (META_PROGRESS.to_string(), to_cbor(&current)),
                (META_PROGRESS_TOTAL.to_string(), to_cbor(&total)),
            ],
        });
        Ok(())
    }

    /// Drain all buffered events. Called by generated code.
    #[doc(hidden)]
    pub fn __take_events(&mut self) -> Vec<RawStreamEvent> {
        std::mem::take(&mut self.events)
    }
}
