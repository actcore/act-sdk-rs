wit_bindgen::generate!({
    path: "wit",
    world: "component-world",
    generate_all,
});

use exports::act::tools::tool_provider::*;

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
    async fn list_tools(_metadata: Vec<(String, Vec<u8>)>) -> Result<ListToolsResponse, Error> {
        Ok(ListToolsResponse {
            metadata: vec![],
            tools: vec![ToolDefinition {
                name: "count".to_string(),
                description: LocalizedString::Plain("Count from 1 to N, emitting each number as a separate event".to_string()),
                parameters_schema: r#"{"type":"object","properties":{"n":{"type":"integer","description":"Number to count to (default 5)"}}}"#.to_string(),
                metadata: vec![],
            }],
        })
    }

    async fn call_tool(
        name: String,
        arguments: Vec<u8>,
        _metadata: Vec<(String, Vec<u8>)>,
    ) -> ToolResult {
        let events = match name.as_str() {
            "count" => {
                let args = from_cbor(&arguments);
                let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

                (1..=n)
                    .map(|i| {
                        ToolEvent::Content(ContentPart {
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
            other => vec![ToolEvent::Error(Error {
                kind: "std:not-found".to_string(),
                message: LocalizedString::Plain(format!("Tool '{other}' not found")),
                metadata: vec![],
            })],
        };

        ToolResult::Immediate(events)
    }
}
