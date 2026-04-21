use darling::FromMeta;
use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};
use syn::{Item, ItemMod};

use crate::tool::{self, ToolAttrs, ToolInfo};

/// Attributes parsed from #[act_component(...)].
/// All fields are optional — defaults are taken from Cargo.toml via env!().
#[derive(Debug, FromMeta)]
pub struct ComponentAttrs {
    /// Path to manifest file relative to crate root (default: `"act.toml"`).
    #[darling(default)]
    pub manifest: Option<String>,
    // Override fields (take precedence over act.toml and Cargo.toml)
    #[darling(default)]
    pub name: Option<String>,
    #[darling(default)]
    pub version: Option<String>,
    #[darling(default)]
    pub description: Option<String>,
    #[darling(default)]
    pub default_language: Option<String>,
}

/// Main code generation for #[act_component].
pub fn generate(attrs: ComponentAttrs, module: &ItemMod) -> syn::Result<TokenStream> {
    // Extract tool functions from the module
    let tools = extract_tools(module)?;

    // Collect user items from the module (excluding #[act_tool] attrs but keeping fn bodies)
    let user_items = collect_user_items(module);

    // Read act.toml manifest (if present) and merge with attribute overrides + Cargo.toml.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let manifest_file = attrs.manifest.as_deref().unwrap_or("act.toml");
    let manifest_path = std::path::Path::new(&manifest_dir).join(manifest_file);

    let manifest = crate::manifest::read_manifest(&manifest_path).unwrap_or_else(|e| panic!("{e}"));

    let overrides = crate::manifest::Overrides {
        name: attrs.name,
        version: attrs.version,
        description: attrs.description,
        default_language: attrs.default_language,
    };

    let info = crate::manifest::build_component_info(manifest, overrides);
    let default_lang = info.std.default_language.as_deref().unwrap_or("en");
    let comp_version = info.std.version.clone();
    let comp_description = info.std.description.clone();

    // Generate CBOR-encoded `act:component` custom section at compile time.
    let mut cbor_buf = Vec::new();
    ciborium::into_writer(&info, &mut cbor_buf).expect("CBOR encoding failed");
    let act_component_cbor = cbor_buf;
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

    // Track act.toml so cargo rebuilds when it changes.
    let manifest_tracking = if manifest_path.exists() {
        let path_str = manifest_path.to_string_lossy().to_string();
        quote! {
            const _: &[u8] = include_bytes!(#path_str);
        }
    } else {
        quote! {}
    };

    // Generate the complete output
    let output = quote! {
        // Make serde/schemars visible for trait bounds in generated code,
        // even if the component doesn't depend on them directly.
        use ::act_sdk::__private::serde;
        use ::act_sdk::__private::schemars;

        // WIT bindings generation
        wit_bindgen::generate!({
            path: "wit",
            world: "component-world",
            generate_all,
        });

        // Track act.toml for cargo rebuild.
        #manifest_tracking

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

        /// Convert a RawToolEvent to a WIT StreamEvent.
        fn __raw_to_wit(raw: ::act_sdk::context::RawToolEvent) -> exports::act::tools::tool_provider::ToolEvent {
            match raw {
                ::act_sdk::context::RawToolEvent::Content { data, mime_type, metadata } => {
                    exports::act::tools::tool_provider::ToolEvent::Content(exports::act::tools::tool_provider::ContentPart {
                        data,
                        mime_type,
                        metadata,
                    })
                }
                ::act_sdk::context::RawToolEvent::Error { kind, message, default_language: _ } => {
                    exports::act::tools::tool_provider::ToolEvent::Error(exports::act::tools::tool_provider::Error {
                        kind,
                        message: exports::act::tools::tool_provider::LocalizedString::Plain(message),
                        metadata: vec![],
                    })
                }
            }
        }

        struct __ActComponent;

        export!(__ActComponent);

        impl exports::act::tools::tool_provider::Guest for __ActComponent {
            async fn list_tools(
                _metadata: Vec<(String, Vec<u8>)>,
            ) -> Result<exports::act::tools::tool_provider::ListToolsResponse, exports::act::tools::tool_provider::Error> {
                Ok(exports::act::tools::tool_provider::ListToolsResponse {
                    metadata: vec![],
                    tools: vec![
                        #(#tool_defs),*
                    ],
                })
            }

            async fn call_tool(
                __name: String,
                __arguments: Vec<u8>,
                __metadata: Vec<(String, Vec<u8>)>,
            ) -> exports::act::tools::tool_provider::ToolResult {
                let __default_lang = #default_lang;
                match __name.as_str() {
                    #(#call_arms)*
                    __other => exports::act::tools::tool_provider::ToolResult::Immediate(vec![
                        exports::act::tools::tool_provider::ToolEvent::Error(exports::act::tools::tool_provider::Error {
                            kind: ::act_sdk::constants::ERR_NOT_FOUND.to_string(),
                            message: exports::act::tools::tool_provider::LocalizedString::Plain(format!("Tool '{}' not found", __other)),
                            metadata: vec![],
                        })
                    ])
                }
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
        exports::act::tools::tool_provider::ToolDefinition {
            name: #name.to_string(),
            description: exports::act::tools::tool_provider::LocalizedString::Plain(#desc.to_string()),
            parameters_schema: #schema_expr,
            metadata: vec![#(#metadata_entries),*],
        }
    }
}

/// Check if a return type has a direct `IntoResponse` impl (no auto-CBOR needed).
///
/// Known types: `()`, `String`, `&str`, `Vec<u8>`, `Json<_>`, `Content`,
/// `serde_json::Value`, `Value`.
fn has_direct_into_response(ty: &syn::Type) -> bool {
    let s = quote!(#ty).to_string().replace(' ', "");
    s == "()"
        || s == "String"
        || s == "&str"
        || s == "Vec<u8>"
        || s.starts_with("Json<")
        || s.starts_with("Content")
}

/// Generate the match arm for call_tool dispatch.
///
/// Non-streaming tools (no `ActContext` parameter) return `ToolResult::Immediate`
/// directly. Streaming tools (with `ActContext`) spawn a writer task and return
/// `ToolResult::Streaming`.
fn gen_call_arm(tool: &ToolInfo, _default_lang: &str) -> TokenStream {
    let tool_name = &tool.tool_name;
    let fn_ident = &tool.fn_ident;

    // Determine how to deserialize and call
    let (deser_code, call_expr) = if let Some(struct_type) = &tool.struct_args {
        // #[args] param: deserialize directly into the struct type
        let deser = quote! {
            let __args: #struct_type = match ::act_sdk::cbor::from_cbor(&__arguments) {
                Ok(v) => v,
                Err(e) => {
                    return exports::act::tools::tool_provider::ToolResult::Immediate(vec![
                        exports::act::tools::tool_provider::ToolEvent::Error(exports::act::tools::tool_provider::Error {
                            kind: ::act_sdk::constants::ERR_INVALID_ARGS.to_string(),
                            message: exports::act::tools::tool_provider::LocalizedString::Plain(format!("Failed to deserialize arguments: {}", e)),
                            metadata: vec![],
                        })
                    ]);
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
            let __args_struct: #struct_name = match ::act_sdk::cbor::from_cbor(&__arguments) {
                Ok(v) => v,
                Err(e) => {
                    return exports::act::tools::tool_provider::ToolResult::Immediate(vec![
                        exports::act::tools::tool_provider::ToolEvent::Error(exports::act::tools::tool_provider::Error {
                            kind: ::act_sdk::constants::ERR_INVALID_ARGS.to_string(),
                            message: exports::act::tools::tool_provider::LocalizedString::Plain(format!("Failed to deserialize arguments: {}", e)),
                            metadata: vec![],
                        })
                    ]);
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

    // Context creation (if needed). For streaming tools this must happen inside
    // the spawned task, so we emit it there; for non-streaming it's unused.
    let metadata_parse = if let Some(metadata_type) = &tool.metadata_type {
        quote! {
            let __metadata_val: #metadata_type = {
                let mut __map = ::act_sdk::__private::serde_json::Map::new();
                for (k, v) in &__metadata {
                    if let Ok(val) = ::act_sdk::cbor::from_cbor::<::act_sdk::__private::serde_json::Value>(v) {
                        __map.insert(k.clone(), val);
                    }
                }
                let __metadata_json = ::act_sdk::__private::serde_json::Value::Object(__map);
                match ::act_sdk::__private::serde_json::from_value::<#metadata_type>(__metadata_json) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = __wit_writer.write_all(vec![
                            exports::act::tools::tool_provider::ToolEvent::Error(exports::act::tools::tool_provider::Error {
                                kind: ::act_sdk::constants::ERR_INVALID_ARGS.to_string(),
                                message: exports::act::tools::tool_provider::LocalizedString::Plain(format!("Failed to deserialize metadata: {}", e)),
                                metadata: vec![],
                            })
                        ]).await;
                        return;
                    }
                }
            };
            let mut __ctx = ::act_sdk::ActContext::__new(__metadata_val);
        }
    } else {
        quote! {
            let mut __ctx = ::act_sdk::ActContext::__new(());
        }
    };

    // Decide whether to use IntoResponse trait or auto-CBOR encoding.
    // Known IntoResponse types use the trait directly; everything else gets CBOR.
    let use_into_response = tool
        .inner_return_type
        .as_ref()
        .is_none_or(has_direct_into_response);

    let ok_response = if use_into_response {
        quote! {
            use ::act_sdk::IntoResponse;
            let __response_events = __val.into_tool_events(__default_lang);
        }
    } else {
        quote! {
            let __response_events = ::act_sdk::response::cbor_encode_response(&__val, __default_lang);
        }
    };

    if tool.has_context {
        // Streaming arm: spawn a writer task, return Streaming(reader).
        // Arguments are deserialized up-front (outside the spawn) so that
        // parse errors become an immediate error without starting a stream.
        quote! {
            #tool_name => {
                #deser_code
                let (mut __wit_writer, __reader) = wit_stream::new::<exports::act::tools::tool_provider::ToolEvent>();
                wit_bindgen::spawn(async move {
                    #metadata_parse
                    let __result = #awaited_call;
                    let __ctx_events = __ctx.__take_events();
                    let mut __wit_events: Vec<exports::act::tools::tool_provider::ToolEvent> = __ctx_events
                        .into_iter()
                        .map(|e| __raw_to_wit(e))
                        .collect();
                    match __result {
                        Ok(__val) => {
                            #ok_response
                            __wit_events.extend(__response_events.into_iter().map(|e| __raw_to_wit(e)));
                        }
                        Err(__err) => {
                            __wit_events.push(exports::act::tools::tool_provider::ToolEvent::Error(exports::act::tools::tool_provider::Error {
                                kind: __err.kind.clone(),
                                message: exports::act::tools::tool_provider::LocalizedString::Plain(__err.message.clone()),
                                metadata: vec![],
                            }));
                        }
                    }
                    if !__wit_events.is_empty() {
                        let _ = __wit_writer.write_all(__wit_events).await;
                    }
                });
                exports::act::tools::tool_provider::ToolResult::Streaming(__reader)
            }
        }
    } else {
        // Immediate arm: compute result synchronously, return Immediate(events).
        quote! {
            #tool_name => {
                #deser_code
                let __result = #awaited_call;
                match __result {
                    Ok(__val) => {
                        #ok_response
                        let __wit_events: Vec<exports::act::tools::tool_provider::ToolEvent> = __response_events
                            .into_iter()
                            .map(|e| __raw_to_wit(e))
                            .collect();
                        exports::act::tools::tool_provider::ToolResult::Immediate(__wit_events)
                    }
                    Err(__err) => exports::act::tools::tool_provider::ToolResult::Immediate(vec![
                        exports::act::tools::tool_provider::ToolEvent::Error(exports::act::tools::tool_provider::Error {
                            kind: __err.kind.clone(),
                            message: exports::act::tools::tool_provider::LocalizedString::Plain(__err.message.clone()),
                            metadata: vec![],
                        })
                    ])
                }
            }
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
        #[serde(crate = "::act_sdk::__private::serde")]
        #[schemars(crate = "::act_sdk::__private::schemars")]
        #[allow(non_camel_case_types)]
        struct #struct_name {
            #(#fields)*
        }
    }
}
