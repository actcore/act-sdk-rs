wit_bindgen::generate!({
    path: "wit",
    world: "component-world",
    generate_all,
});

use exports::act::core::tool_provider::Guest;
use act::core::types::*;

/// Create a response stream from a list of events.
///
/// Component model streams are unbuffered — writes block until a consumer reads.
/// Since the host can only register a consumer after `call_tool` returns the
/// StreamReader, writes must happen in a background task.
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
    fn get_info() -> ComponentInfo {
        ComponentInfo {
            name: "hello-world".to_string(),
            version: "0.1.0".to_string(),
            default_language: "en".to_string(),
            description: LocalizedString::Plain("A hello-world ACT component".to_string()),
            capabilities: vec![],
            metadata: vec![],
        }
    }

    fn get_config_schema() -> Option<String> {
        None
    }

    async fn list_tools(
        _config: Option<Vec<u8>>,
    ) -> Result<ListToolsResponse, ToolError> {
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
        _config: Option<Vec<u8>>,
        call: ToolCall,
    ) -> CallResponse {
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

        CallResponse {
            metadata: vec![],
            body: respond(vec![event]),
        }
    }
}
