wit_bindgen::generate!({
    path: "wit",
    world: "component-world",
    generate_all,
});

use exports::act::tools::tool_provider::*;

/// Decode CBOR bytes to a JSON value.
fn from_cbor(bytes: &[u8]) -> serde_json::Value {
    ciborium::from_reader(bytes).unwrap_or_default()
}

struct HelloWorld;

export!(HelloWorld);

impl Guest for HelloWorld {
    async fn list_tools(_metadata: Vec<(String, Vec<u8>)>) -> Result<ListToolsResponse, Error> {
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
        name: String,
        arguments: Vec<u8>,
        _metadata: Vec<(String, Vec<u8>)>,
    ) -> ToolResult {
        let event = match name.as_str() {
            "greet" => {
                let args = from_cbor(&arguments);
                let who = args.get("name").and_then(|v| v.as_str()).unwrap_or("world");
                let greeting = format!("Hello, {who}!");

                ToolEvent::Content(ContentPart {
                    data: greeting.into_bytes(),
                    mime_type: Some("text/plain".to_string()),
                    metadata: vec![],
                })
            }
            other => ToolEvent::Error(Error {
                kind: "std:not-found".to_string(),
                message: LocalizedString::Plain(format!("Tool '{other}' not found")),
                metadata: vec![],
            }),
        };

        ToolResult::Immediate(vec![event])
    }
}
