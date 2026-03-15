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

/// Decode CBOR bytes to a JSON value.
fn from_cbor(bytes: &[u8]) -> serde_json::Value {
    ciborium::from_reader(bytes).unwrap_or_default()
}

struct HelloWorld;

export!(HelloWorld);

impl Guest for HelloWorld {
    async fn get_metadata_schema(_metadata: Vec<(String, Vec<u8>)>) -> Option<String> {
        None
    }

    async fn list_tools(_metadata: Vec<(String, Vec<u8>)>) -> Result<ListToolsResponse, ToolError> {
        Ok(ListToolsResponse {
            metadata: vec![],
            tools: vec![ToolDefinition {
                name: "greet".to_string(),
                description: LocalizedString::Plain("Say hello to someone".to_string()),
                parameters_schema: r#"{"type":"object","properties":{"name":{"type":"string","description":"Name to greet"}},"required":["name"]}"#.to_string(),
                metadata: vec![],
            }],
        })
    }

    async fn call_tool(
        call: ToolCall,
    ) -> wit_bindgen::rt::async_support::StreamReader<StreamEvent> {
        let event = match call.name.as_str() {
            "greet" => {
                let args = from_cbor(&call.arguments);
                let who = args.get("name").and_then(|v| v.as_str()).unwrap_or("world");
                let greeting = format!("Hello, {who}!");

                StreamEvent::Content(ContentPart {
                    data: greeting.into_bytes(),
                    mime_type: Some("text/plain".to_string()),
                    metadata: vec![],
                })
            }
            other => StreamEvent::Error(ToolError {
                kind: "std:not-found".to_string(),
                message: LocalizedString::Plain(format!("Tool '{other}' not found")),
                metadata: vec![],
            }),
        };

        respond(vec![event])
    }
}
