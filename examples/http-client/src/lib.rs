wit_bindgen::generate!({
    path: "wit",
    world: "component-world",
    generate_all,
});

use exports::act::core::tool_provider::Guest;
use act::core::types::*;

use serde::Deserialize;
use std::collections::HashMap;
use wasip3::http::types::{ErrorCode, Fields, Method, Request, Response, Scheme};

#[derive(Deserialize)]
struct FetchArgs {
    url: String,
    #[serde(default = "default_method")]
    method: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

fn to_cbor(value: &serde_json::Value) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf).unwrap();
    buf
}

fn make_error(kind: &str, message: &str) -> StreamEvent {
    StreamEvent::Error(ToolError {
        kind: kind.to_string(),
        message: LocalizedString::Plain(message.to_string()),
        metadata: vec![],
    })
}

struct HttpClient;

export!(HttpClient);

impl Guest for HttpClient {
    fn get_info() -> ComponentInfo {
        ComponentInfo {
            name: "http-client".to_string(),
            version: "0.1.0".to_string(),
            default_language: "en".to_string(),
            description: LocalizedString::Plain("HTTP client ACT component".to_string()),
            capabilities: vec![Capability {
                id: "wasi:http/client".to_string(),
                required: true,
                description: Some(LocalizedString::Plain("Make outbound HTTP requests".to_string())),
                metadata: vec![],
            }],
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
                name: "fetch".to_string(),
                description: LocalizedString::Plain("Make an HTTP request".to_string()),
                parameters_schema: r#"{
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "URL to fetch" },
                        "method": { "type": "string", "description": "HTTP method (default GET)", "enum": ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "QUERY"] },
                        "headers": { "type": "object", "description": "Request headers as key-value pairs", "additionalProperties": { "type": "string" } },
                        "body": { "type": "string", "description": "Request body (for POST/PUT/PATCH)" }
                    },
                    "required": ["url"]
                }"#.to_string(),
                metadata: vec![],
            }],
        })
    }

    async fn call_tool(
        _config: Option<Vec<u8>>,
        call: ToolCall,
    ) -> wit_bindgen::rt::async_support::StreamReader<StreamEvent> {
        let (mut writer, reader) = wit_stream::new::<StreamEvent>();
        let arguments = call.arguments;
        let name = call.name;

        wit_bindgen::spawn(async move {
            let event = match name.as_str() {
                "fetch" => do_fetch(&arguments).await,
                other => make_error("std:not-found", &format!("Tool '{other}' not found")),
            };
            writer.write_all(vec![event]).await;
        });

        reader
    }
}

async fn do_fetch(arguments: &[u8]) -> StreamEvent {
    let args: FetchArgs = match ciborium::from_reader(arguments) {
        Ok(a) => a,
        Err(e) => return make_error("std:invalid-args", &format!("Invalid arguments: {e}")),
    };

    let method = match args.method.as_str() {
        "GET" => Method::Get,
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "DELETE" => Method::Delete,
        "PATCH" => Method::Patch,
        "HEAD" => Method::Head,
        "OPTIONS" => Method::Options,
        "QUERY" => Method::Other("QUERY".to_string()),
        other => return make_error("std:invalid-args", &format!("Unsupported method: {other}")),
    };

    let parsed = match url::Url::parse(&args.url) {
        Ok(u) => u,
        Err(e) => return make_error("std:invalid-args", &format!("Invalid URL: {e}")),
    };

    let scheme = match parsed.scheme() {
        "https" => Scheme::Https,
        "http" => Scheme::Http,
        other => return make_error("std:invalid-args", &format!("Unsupported scheme: {other}")),
    };

    // Build headers
    let header_list: Vec<(String, Vec<u8>)> = args
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.as_bytes().to_vec()))
        .collect();
    let headers = Fields::from_list(&header_list).unwrap();

    // Build body stream (using wasip3's wit_stream for u8)
    let body_contents = if let Some(body_str) = &args.body {
        let (mut body_writer, body_reader) = wasip3::wit_stream::new::<u8>();
        let body_bytes = body_str.as_bytes().to_vec();
        wit_bindgen::spawn(async move {
            body_writer.write_all(body_bytes).await;
        });
        Some(body_reader)
    } else {
        None
    };

    // Trailers: none (using wasip3's wit_future)
    let (trailers_writer, trailers_reader) =
        wasip3::wit_future::new::<Result<Option<Fields>, ErrorCode>>(|| Ok(None));
    drop(trailers_writer);

    let (request, _completion) = Request::new(headers, body_contents, trailers_reader, None);
    let _ = request.set_method(&method);
    let _ = request.set_scheme(Some(&scheme));

    let authority = match parsed.port() {
        Some(port) => format!("{}:{}", parsed.host_str().unwrap_or(""), port),
        None => parsed.host_str().unwrap_or("").to_string(),
    };
    let _ = request.set_authority(Some(&authority));

    let path_with_query = match parsed.query() {
        Some(q) => format!("{}?{q}", parsed.path()),
        None => parsed.path().to_string(),
    };
    let _ = request.set_path_with_query(Some(&path_with_query));

    // Send request
    let response = match wasip3::http::client::send(request).await {
        Ok(r) => r,
        Err(e) => return make_error("std:internal", &format!("HTTP error: {e:?}")),
    };

    let status = response.get_status_code();

    // Consume response body (using wasip3's wit_future for the result signal)
    let (result_writer, result_reader) =
        wasip3::wit_future::new::<Result<(), ErrorCode>>(|| Ok(()));
    drop(result_writer);

    let (mut body_stream, _trailers) = Response::consume_body(response, result_reader);

    let mut body_bytes = Vec::new();
    loop {
        let (result, chunk) = body_stream.read(Vec::with_capacity(16384)).await;
        match result {
            wasip3::wit_bindgen::StreamResult::Complete(_) => {
                body_bytes.extend_from_slice(&chunk);
            }
            wasip3::wit_bindgen::StreamResult::Dropped
            | wasip3::wit_bindgen::StreamResult::Cancelled => break,
        }
    }

    let body_text = String::from_utf8_lossy(&body_bytes);
    let result = serde_json::json!({
        "status": status,
        "body": body_text,
    });

    StreamEvent::Content(ContentPart {
        data: to_cbor(&result),
        mime_type: Some("application/json".to_string()),
        metadata: vec![],
    })
}
