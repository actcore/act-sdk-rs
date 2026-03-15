use darling::FromMeta;
use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};
use syn::{Item, ItemMod};

use crate::tool::{self, ToolAttrs, ToolInfo};

/// Attributes parsed from #[act_component(...)].
#[derive(Debug, FromMeta)]
pub struct ComponentAttrs {
    pub name: String,
    pub version: String,
    pub description: String,
    #[darling(default)]
    pub default_language: Option<String>,
}

/// Main code generation for #[act_component].
pub fn generate(attrs: ComponentAttrs, module: &ItemMod) -> syn::Result<TokenStream> {
    // Extract tool functions from the module
    let tools = extract_tools(module)?;

    // Collect user items from the module (excluding #[act_tool] attrs but keeping fn bodies)
    let user_items = collect_user_items(module);

    let comp_name = &attrs.name;
    let comp_version = &attrs.version;
    let comp_description = &attrs.description;
    let default_lang = attrs.default_language.as_deref().unwrap_or("en");

    // Generate CBOR-encoded `act:component` custom section at compile time.
    let act_component_cbor = gen_component_section_cbor(
        comp_name,
        comp_version,
        comp_description,
        &attrs.default_language,
    );
    let cbor_len = act_component_cbor.len();
    let cbor_literal = Literal::byte_string(&act_component_cbor);

    // Standard WASM metadata sections (plain UTF-8 strings).
    let version_len = comp_version.len();
    let version_literal = Literal::byte_string(comp_version.as_bytes());
    let description_len = comp_description.len();
    let description_literal = Literal::byte_string(comp_description.as_bytes());

    // Generate tool definition entries for list_tools
    let tool_defs = tools.iter().map(|t| gen_tool_definition(t, default_lang));

    // Generate call_tool match arms
    let call_arms = tools.iter().map(|t| gen_call_arm(t, default_lang));

    // Generate hidden arg structs for tools with individual params (not #[args])
    let arg_structs = tools
        .iter()
        .filter(|t| t.struct_args.is_none() && !t.args.is_empty())
        .map(gen_arg_struct);

    // Generate metadata schema expression
    let metadata_schema_expr =
        if let Some(metadata_type) = tools.iter().find_map(|t| t.metadata_type.as_ref()) {
            quote! {
                Some(::act_sdk::__private::serde_json::to_string(
                    &::act_sdk::__private::schemars::schema_for!(#metadata_type)
                ).unwrap_or_else(|_| r#"{"type":"object"}"#.to_string()))
            }
        } else {
            quote! { None }
        };

    // Generate the complete output
    let output = quote! {
        // WIT bindings generation
        wit_bindgen::generate!({
            path: "wit",
            world: "component-world",
            generate_all,
        });

        // Standard WASM metadata custom sections (OCI annotations).
        // Compatible with wasm-tools, wkg, wa.dev, and the WASM component ecosystem.
        // SAFETY: link_section places data in named WASM custom sections; no executable code.
        #[unsafe(link_section = "version")]
        #[used]
        static __ACT_VERSION_SECTION: [u8; #version_len] = *#version_literal;

        #[unsafe(link_section = "description")]
        #[used]
        static __ACT_DESCRIPTION_SECTION: [u8; #description_len] = *#description_literal;

        // `act:component` custom section — CBOR-encoded ACT-specific metadata.
        // Contains fields not covered by standard WASM metadata (e.g. std:default-language).
        #[unsafe(link_section = "act:component")]
        #[used]
        static __ACT_COMPONENT_SECTION: [u8; #cbor_len] = *#cbor_literal;

        // User-defined items from the module body
        #(#user_items)*

        // Generated hidden arg structs
        #(#arg_structs)*

        /// Convert a RawStreamEvent to a WIT StreamEvent.
        fn __raw_to_wit(raw: ::act_sdk::context::RawStreamEvent) -> act::core::types::StreamEvent {
            match raw {
                ::act_sdk::context::RawStreamEvent::Content { data, mime_type, metadata } => {
                    act::core::types::StreamEvent::Content(act::core::types::ContentPart {
                        data,
                        mime_type,
                        metadata,
                    })
                }
                ::act_sdk::context::RawStreamEvent::Error { kind, message, default_language: _ } => {
                    act::core::types::StreamEvent::Error(act::core::types::ToolError {
                        kind,
                        message: act::core::types::LocalizedString::Plain(message),
                        metadata: vec![],
                    })
                }
            }
        }

        struct __ActComponent;

        export!(__ActComponent);

        impl exports::act::core::tool_provider::Guest for __ActComponent {
            async fn get_metadata_schema(
                _metadata: Vec<(String, Vec<u8>)>,
            ) -> Option<String> {
                #metadata_schema_expr
            }

            async fn list_tools(
                _metadata: Vec<(String, Vec<u8>)>,
            ) -> Result<act::core::types::ListToolsResponse, act::core::types::ToolError> {
                Ok(act::core::types::ListToolsResponse {
                    metadata: vec![],
                    tools: vec![
                        #(#tool_defs),*
                    ],
                })
            }

            async fn call_tool(
                call: act::core::types::ToolCall,
            ) -> wit_bindgen::rt::async_support::StreamReader<act::core::types::StreamEvent> {
                let (mut __wit_writer, reader) = wit_stream::new::<act::core::types::StreamEvent>();

                // Two channels: buffered (unbounded) and direct (zero-capacity backpressure)
                let (__buffered_tx, __buffered_rx) = ::act_sdk::__private::async_channel::unbounded::<::act_sdk::context::RawStreamEvent>();
                let (__direct_tx, __direct_rx) = ::act_sdk::__private::async_channel::bounded::<::act_sdk::context::RawStreamEvent>(0);

                // Bridge task: reads from both channels, writes to WIT stream
                wit_bindgen::spawn(async move {
                    loop {
                        // Prioritize direct (backpressure) channel, fall back to buffered
                        let event = ::act_sdk::__private::futures_lite::future::or(
                            __direct_rx.recv(),
                            __buffered_rx.recv(),
                        ).await;
                        match event {
                            Ok(raw) => {
                                let wit_event = __raw_to_wit(raw);
                                let _ = __wit_writer.write_all(vec![wit_event]).await;
                            }
                            Err(_) => break, // Both channels closed
                        }
                    }
                });

                // Tool task: runs user function
                wit_bindgen::spawn(async move {
                    let __default_lang = #default_lang;
                    match call.name.as_str() {
                        #(#call_arms)*
                        __other => {
                            let _ = __buffered_tx.try_send(
                                ::act_sdk::context::RawStreamEvent::Error {
                                    kind: ::act_sdk::constants::ERR_NOT_FOUND.to_string(),
                                    message: format!("Tool '{}' not found", __other),
                                    default_language: __default_lang.to_string(),
                                }
                            );
                        }
                    }
                });

                reader
            }
        }
    };

    Ok(output)
}

/// Extract ToolInfo for each #[act_tool] function in the module.
fn extract_tools(module: &ItemMod) -> syn::Result<Vec<ToolInfo>> {
    let mut tools = Vec::new();

    if let Some((_, items)) = &module.content {
        for item in items {
            if let Item::Fn(func) = item {
                // Find #[act_tool] attribute
                let tool_attr = func.attrs.iter().find(|a| a.path().is_ident("act_tool"));

                if let Some(attr) = tool_attr {
                    let attrs = ToolAttrs::from_meta(&attr.meta).map_err(syn::Error::from)?;
                    let info = tool::parse_tool_fn(func, attrs)?;
                    tools.push(info);
                }
            }
        }
    }

    Ok(tools)
}

/// Collect user items from the module, stripping #[act_tool] attributes from functions.
/// Also rewrites `use super::*` to `use crate::*` since the module body is flattened to top level,
/// and strips #[doc] attributes from function parameters (not allowed by rustc).
fn collect_user_items(module: &ItemMod) -> Vec<TokenStream> {
    let mut items = Vec::new();

    if let Some((_, mod_items)) = &module.content {
        for item in mod_items {
            match item {
                Item::Fn(func) => {
                    // Strip #[act_tool] attribute but keep the function
                    let mut clean_func = func.clone();
                    clean_func.attrs.retain(|a| !a.path().is_ident("act_tool"));
                    // Strip #[doc] and #[args] attributes from function parameters
                    for input in &mut clean_func.sig.inputs {
                        if let syn::FnArg::Typed(pat_type) = input {
                            pat_type.attrs.retain(|a| {
                                !a.path().is_ident("doc") && !a.path().is_ident("args")
                            });
                        }
                    }
                    items.push(quote! { #clean_func });
                }
                Item::Use(u) => {
                    // Rewrite `use super::*` and `use super::Foo` to nothing,
                    // since the module is flattened and parent items are at the same level.
                    if is_super_use(u) {
                        // Skip — the items from "super" are already at crate level
                        continue;
                    }
                    items.push(quote! { #u });
                }
                other => {
                    items.push(quote! { #other });
                }
            }
        }
    }

    items
}

/// Check if a `use` item refers to `super::`.
fn is_super_use(u: &syn::ItemUse) -> bool {
    fn tree_starts_with_super(tree: &syn::UseTree) -> bool {
        match tree {
            syn::UseTree::Path(p) => p.ident == "super",
            syn::UseTree::Group(g) => g.items.iter().any(tree_starts_with_super),
            _ => false,
        }
    }
    tree_starts_with_super(&u.tree)
}

/// Generate a ToolDefinition expression for list_tools.
fn gen_tool_definition(tool: &ToolInfo, _default_lang: &str) -> TokenStream {
    let name = &tool.tool_name;
    let desc = &tool.description;

    // Generate JSON Schema for parameters
    let schema_expr = if let Some(struct_type) = &tool.struct_args {
        // #[args] param: use its type directly
        quote! {
            {
                let schema = ::act_sdk::__private::schemars::schema_for!(#struct_type);
                ::act_sdk::__private::serde_json::to_string(&schema)
                    .unwrap_or_else(|_| r#"{"type":"object"}"#.to_string())
            }
        }
    } else if tool.args.is_empty() {
        quote! { r#"{"type":"object","properties":{}}"#.to_string() }
    } else {
        // Individual params: use generated hidden struct
        let struct_name = gen_args_struct_ident(&tool.fn_ident);
        quote! {
            {
                let schema = ::act_sdk::__private::schemars::schema_for!(#struct_name);
                ::act_sdk::__private::serde_json::to_string(&schema)
                    .unwrap_or_else(|_| r#"{"type":"object"}"#.to_string())
            }
        }
    };

    // Generate metadata entries
    let mut metadata_entries = Vec::new();

    if tool.read_only {
        metadata_entries.push(quote! {
            (::act_sdk::constants::META_READ_ONLY.to_string(), ::act_sdk::cbor::to_cbor(&true))
        });
    }
    if tool.idempotent {
        metadata_entries.push(quote! {
            (::act_sdk::constants::META_IDEMPOTENT.to_string(), ::act_sdk::cbor::to_cbor(&true))
        });
    }
    if tool.destructive {
        metadata_entries.push(quote! {
            (::act_sdk::constants::META_DESTRUCTIVE.to_string(), ::act_sdk::cbor::to_cbor(&true))
        });
    }
    if tool.streaming {
        metadata_entries.push(quote! {
            (::act_sdk::constants::META_STREAMING.to_string(), ::act_sdk::cbor::to_cbor(&true))
        });
    }
    if let Some(ms) = tool.timeout_ms {
        metadata_entries.push(quote! {
            (::act_sdk::constants::META_TIMEOUT_MS.to_string(), ::act_sdk::cbor::to_cbor(&#ms))
        });
    }

    quote! {
        act::core::types::ToolDefinition {
            name: #name.to_string(),
            description: act::core::types::LocalizedString::Plain(#desc.to_string()),
            parameters_schema: #schema_expr,
            metadata: vec![#(#metadata_entries),*],
        }
    }
}

/// Generate the match arm for call_tool dispatch.
fn gen_call_arm(tool: &ToolInfo, _default_lang: &str) -> TokenStream {
    let tool_name = &tool.tool_name;
    let fn_ident = &tool.fn_ident;

    // Determine how to deserialize and call
    let (deser_code, call_expr) = if let Some(struct_type) = &tool.struct_args {
        // #[args] param: deserialize directly into the struct type
        let deser = quote! {
            let __args: #struct_type = match ::act_sdk::cbor::from_cbor(&call.arguments) {
                Ok(v) => v,
                Err(e) => {
                    let _ = __buffered_tx.try_send(
                        ::act_sdk::context::RawStreamEvent::Error {
                            kind: ::act_sdk::constants::ERR_INVALID_ARGS.to_string(),
                            message: format!("Failed to deserialize arguments: {}", e),
                            default_language: __default_lang.to_string(),
                        }
                    );
                    return;
                }
            };
        };

        let call = if tool.has_context {
            quote! { #fn_ident(__args, &mut __ctx) }
        } else {
            quote! { #fn_ident(__args) }
        };

        (deser, call)
    } else if tool.args.is_empty() {
        // No args
        let call = if tool.has_context {
            quote! { #fn_ident(&mut __ctx) }
        } else {
            quote! { #fn_ident() }
        };
        (quote! {}, call)
    } else {
        // Individual params: deserialize into hidden struct, extract fields
        let struct_name = gen_args_struct_ident(fn_ident);
        let field_names: Vec<_> = tool
            .args
            .iter()
            .map(|a| format_ident!("{}", a.name))
            .collect();

        let deser = quote! {
            let __args_struct: #struct_name = match ::act_sdk::cbor::from_cbor(&call.arguments) {
                Ok(v) => v,
                Err(e) => {
                    let _ = __buffered_tx.try_send(
                        ::act_sdk::context::RawStreamEvent::Error {
                            kind: ::act_sdk::constants::ERR_INVALID_ARGS.to_string(),
                            message: format!("Failed to deserialize arguments: {}", e),
                            default_language: __default_lang.to_string(),
                        }
                    );
                    return;
                }
            };
        };

        let call = if tool.has_context {
            quote! { #fn_ident(#(__args_struct.#field_names),*, &mut __ctx) }
        } else {
            quote! { #fn_ident(#(__args_struct.#field_names),*) }
        };

        (deser, call)
    };

    // Wrap call with .await if async
    let awaited_call = if tool.is_async {
        quote! { #call_expr.await }
    } else {
        quote! { #call_expr }
    };

    // Context creation (if needed)
    let ctx_setup = if tool.has_context {
        if let Some(metadata_type) = &tool.metadata_type {
            quote! {
                let __metadata_val: #metadata_type = {
                    let mut __map = ::act_sdk::__private::serde_json::Map::new();
                    for (k, v) in &call.metadata {
                        if let Ok(val) = ::act_sdk::cbor::from_cbor::<::act_sdk::__private::serde_json::Value>(v) {
                            __map.insert(k.clone(), val);
                        }
                    }
                    let __metadata_json = ::act_sdk::__private::serde_json::Value::Object(__map);
                    match ::act_sdk::__private::serde_json::from_value::<#metadata_type>(__metadata_json) {
                        Ok(v) => v,
                        Err(e) => {
                            let _ = __buffered_tx.try_send(
                                ::act_sdk::context::RawStreamEvent::Error {
                                    kind: ::act_sdk::constants::ERR_INVALID_ARGS.to_string(),
                                    message: format!("Failed to deserialize metadata: {}", e),
                                    default_language: __default_lang.to_string(),
                                }
                            );
                            return;
                        }
                    }
                };
                let mut __ctx = ::act_sdk::ActContext::__new(__metadata_val, __buffered_tx.clone(), __direct_tx.clone());
            }
        } else {
            quote! {
                let mut __ctx = ::act_sdk::ActContext::__new((), __buffered_tx.clone(), __direct_tx.clone());
            }
        }
    } else {
        quote! {}
    };

    // Post-call: send result events via buffered channel
    let post_call = quote! {
        match __result {
            Ok(__val) => {
                use ::act_sdk::IntoResponse;
                for event in __val.into_stream_events(__default_lang) {
                    let _ = __buffered_tx.try_send(event);
                }
            }
            Err(__err) => {
                let _ = __buffered_tx.try_send(
                    ::act_sdk::context::RawStreamEvent::Error {
                        kind: __err.kind.clone(),
                        message: __err.message.clone(),
                        default_language: __default_lang.to_string(),
                    }
                );
            }
        }
    };

    quote! {
        #tool_name => {
            #ctx_setup
            #deser_code
            let __result = #awaited_call;
            #post_call
        }
    }
}

/// Generate the hidden args struct name from a function ident.
fn gen_args_struct_ident(fn_ident: &syn::Ident) -> syn::Ident {
    let pascal = fn_ident
        .to_string()
        .split('_')
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<String>();
    format_ident!("__{}Args", pascal)
}

/// CBOR-serializable struct for the `act:component` custom section.
///
/// Name, version, description are stored in standard WASM metadata sections
/// (component-name, version, description) — not duplicated here.
#[derive(serde::Serialize)]
struct ActComponentSection {
    #[serde(rename = "std:name")]
    name: String,
    #[serde(rename = "std:version")]
    version: String,
    #[serde(rename = "std:description")]
    description: String,
    #[serde(
        rename = "std:default-language",
        skip_serializing_if = "Option::is_none"
    )]
    default_language: Option<String>,
}

/// Generate CBOR bytes for the `act:component` custom section.
fn gen_component_section_cbor(
    name: &str,
    version: &str,
    description: &str,
    default_language: &Option<String>,
) -> Vec<u8> {
    let section = ActComponentSection {
        name: name.to_string(),
        version: version.to_string(),
        description: description.to_string(),
        default_language: default_language.clone(),
    };
    let mut buf = Vec::new();
    ciborium::into_writer(&section, &mut buf).expect("CBOR encoding failed");
    buf
}

/// Generate a hidden #[derive(Deserialize, JsonSchema)] struct for individual-params tools.
fn gen_arg_struct(tool: &ToolInfo) -> TokenStream {
    let struct_name = gen_args_struct_ident(&tool.fn_ident);

    let fields: Vec<TokenStream> = tool
        .args
        .iter()
        .map(|arg| {
            let name = format_ident!("{}", arg.name);
            let ty = &arg.ty;
            if let Some(doc) = &arg.doc {
                quote! {
                    #[doc = #doc]
                    pub #name: #ty,
                }
            } else {
                quote! {
                    pub #name: #ty,
                }
            }
        })
        .collect();

    quote! {
        #[derive(::act_sdk::__private::serde::Deserialize, ::act_sdk::__private::schemars::JsonSchema)]
        #[allow(non_camel_case_types)]
        struct #struct_name {
            #(#fields)*
        }
    }
}
