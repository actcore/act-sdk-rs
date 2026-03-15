/// Raw stream event before conversion to WIT types.
/// Used internally by ActContext; the bridge task converts to WIT StreamEvent.
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

/// The direct stream writer type for backpressure streaming.
pub type StreamWriter = async_channel::Sender<RawStreamEvent>;

/// Context passed to tool functions. Provides metadata access and stream writing.
///
/// Two modes of sending events:
///
/// - **Buffered**: `send_text()`/`send_content()` write to an unbounded channel.
///   Events are delivered as fast as the bridge task can forward them to the
///   WIT stream. Never blocks the caller. Suitable for most tools.
/// - **Direct**: `writer().send(event).await` writes to a zero-capacity channel.
///   Blocks until the event is consumed by the WIT stream reader (backpressure).
///   Use for high-throughput streaming where memory control matters.
pub struct ActContext<C = ()> {
    metadata: C,
    buffered_tx: StreamWriter,
    direct_tx: StreamWriter,
}

impl<C> ActContext<C> {
    #[doc(hidden)]
    pub fn __new(metadata: C, buffered_tx: StreamWriter, direct_tx: StreamWriter) -> Self {
        Self {
            metadata,
            buffered_tx,
            direct_tx,
        }
    }

    /// Access the deserialized metadata.
    pub fn metadata(&self) -> &C {
        &self.metadata
    }

    /// Send a text content event (buffered, non-blocking).
    pub fn send_text(&self, text: impl Into<String>) {
        self.send_content(
            text.into().into_bytes(),
            Some("text/plain".to_string()),
            vec![],
        );
    }

    /// Send a content event with explicit data, MIME type, and metadata (buffered, non-blocking).
    pub fn send_content(
        &self,
        data: Vec<u8>,
        mime_type: Option<String>,
        metadata: Vec<(String, Vec<u8>)>,
    ) {
        let _ = self.buffered_tx.try_send(RawStreamEvent::Content {
            data,
            mime_type,
            metadata,
        });
    }

    /// Access the direct (unbuffered, backpressure) stream writer.
    ///
    /// Use `writer().send(event).await` for true streaming with backpressure.
    /// Each `send` blocks until the event is consumed by the WIT stream reader.
    ///
    /// **Note:** Only use inside `wit_bindgen::spawn()`. Using from a synchronous
    /// context will deadlock.
    pub fn writer(&self) -> &StreamWriter {
        &self.direct_tx
    }
}
