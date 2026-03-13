wit_bindgen::generate!({
    path: "wit",
    world: "component-world",
    generate_all,
});

use act::core::types::*;
use exports::act::core::tool_provider::Guest;

/// Create a response stream from a list of events.
fn respond(events: Vec<StreamEvent>) -> wit_bindgen::rt::async_support::StreamReader<StreamEvent> {
    let (mut writer, reader) = wit_stream::new::<StreamEvent>();
    wit_bindgen::spawn(async move {
        writer.write_all(events).await;
    });
    reader
}

/// Encode a value as CBOR bytes.
fn to_cbor(value: &serde_json::Value) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf).unwrap();
    buf
}

/// Decode CBOR bytes to a JSON value.
fn from_cbor(bytes: &[u8]) -> serde_json::Value {
    ciborium::from_reader(bytes).unwrap_or_default()
}

struct Counter;

export!(Counter);

impl Guest for Counter {
    fn get_info() -> ComponentInfo {
        ComponentInfo {
            name: "counter".to_string(),
            version: "0.1.0".to_string(),
            default_language: "en".to_string(),
            description: LocalizedString::Plain("A streaming counter ACT component".to_string()),
            capabilities: vec![],
            metadata: vec![],
        }
    }

    fn get_config_schema() -> Option<String> {
        None
    }

    async fn list_tools(_config: Option<Vec<u8>>) -> Result<ListToolsResponse, ToolError> {
        Ok(ListToolsResponse {
            metadata: vec![],
            tools: vec![ToolDefinition {
                name: "count".to_string(),
                description: LocalizedString::Plain("Count from 1 to N, emitting each number as a separate event".to_string()),
                parameters_schema: r#"{"type":"object","properties":{"n":{"type":"integer","description":"Number to count to (default 5)"}}}"#.to_string(),
                metadata: vec![
                    ("std:streaming".to_string(), to_cbor(&serde_json::Value::Bool(true))),
                ],
            }],
        })
    }

    async fn call_tool(
        _config: Option<Vec<u8>>,
        call: ToolCall,
    ) -> wit_bindgen::rt::async_support::StreamReader<StreamEvent> {
        let events = match call.name.as_str() {
            "count" => {
                let args = from_cbor(&call.arguments);
                let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

                (1..=n)
                    .map(|i| {
                        StreamEvent::Content(ContentPart {
                            data: format!("Count: {i}").into_bytes(),
                            mime_type: Some("text/plain".to_string()),
                            metadata: vec![
                                ("std:progress".to_string(), to_cbor(&serde_json::json!(i))),
                                (
                                    "std:progress-total".to_string(),
                                    to_cbor(&serde_json::json!(n)),
                                ),
                            ],
                        })
                    })
                    .collect()
            }
            other => vec![StreamEvent::Error(ToolError {
                kind: "std:not-found".to_string(),
                message: LocalizedString::Plain(format!("Tool '{other}' not found")),
                metadata: vec![],
            })],
        };

        respond(events)
    }
}
