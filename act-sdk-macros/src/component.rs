use darling::FromMeta;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Item, ItemMod};

use crate::tool::{self, ToolAttrs, ToolInfo};

/// Read and parse an `include!("path")` macro item's file path.
/// Returns the parsed items from the file, or None if not a recognizable `include!`.
///
/// NOTE: Rust's `include!` resolves paths relative to the including source file.
/// Since proc macros don't have direct access to the caller's file path, we look
/// in both `CARGO_MANIFEST_DIR/src/<path>` and `CARGO_MANIFEST_DIR/<path>`.
fn expand_include_item(mac_item: &syn::ItemMacro) -> Option<Vec<Item>> {
    if !mac_item.mac.path.is_ident("include") {
        return None;
    }
    // Parse the string literal argument: include!("path/to/file.rs")
    let lit: syn::LitStr = syn::parse2(mac_item.mac.tokens.clone()).ok()?;
    let file_path_str = lit.value();

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let manifest_path = std::path::Path::new(&manifest_dir);

    // Try src/<path> first (most common: include!("modules/task.rs") from src/lib.rs)
    let in_src = manifest_path.join("src").join(&file_path_str);
    // Fallback: path directly relative to CARGO_MANIFEST_DIR
    let at_root = manifest_path.join(&file_path_str);

    let full_path = if in_src.exists() {
        in_src
    } else if at_root.exists() {
        at_root
    } else {
        return None;
    };

    // Read the file
    let content = std::fs::read_to_string(&full_path).ok()?;

    // Parse as a file (sequence of items)
    let file: syn::File = syn::parse_str(&content).ok()?;
    Some(file.items)
}

/// Main code generation for #[act_component].
pub fn generate(module: &ItemMod) -> syn::Result<TokenStream> {
    // Extract tool functions from the module
    let tools = extract_tools(module)?;

    // Extract optional session-provider hooks
    let session_hooks = extract_session_hooks(module)?;

    // Collect user items from the module (excluding #[act_tool] attrs but keeping fn bodies)
    let user_items = collect_user_items(module);

    // Runtime response-encoding language. Component metadata (incl. the real
    // default-language) is embedded by `act-build pack`, not the macro.
    let default_lang = "en";

    // Generate tool definition entries for list_tools
    let tool_defs = tools.iter().map(|t| gen_tool_definition(t, default_lang));

    // Generate call_tool match arms
    let call_arms = tools.iter().map(|t| gen_call_arm(t, default_lang));

    // Generate hidden arg structs for tools with individual params (not #[args])
    let arg_structs = tools
        .iter()
        .filter(|t| t.struct_args.is_none() && !t.args.is_empty())
        .map(gen_arg_struct);

    // Generate session-provider Guest impl, if hooks are present.
    let session_provider_impl = match &session_hooks {
        Some(h) => gen_session_provider_impl(h),
        None => quote! {},
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

        // User-defined items from the module body
        #(#user_items)*

        // Generated hidden arg structs
        #(#arg_structs)*

        /// Convert a RawToolEvent to a WIT StreamEvent.
        fn __raw_to_wit(raw: ::act_sdk::context::RawToolEvent) -> act::tools::types::ToolEvent {
            match raw {
                ::act_sdk::context::RawToolEvent::Content { data, mime_type, metadata } => {
                    act::tools::types::ToolEvent::Content(act::tools::types::ContentPart {
                        data,
                        mime_type,
                        metadata,
                    })
                }
                ::act_sdk::context::RawToolEvent::Error { kind, message, default_language: _ } => {
                    act::tools::types::ToolEvent::Error(act::tools::types::Error {
                        kind,
                        message: act::core::types::LocalizedString::Plain(message),
                        metadata: vec![],
                    })
                }
            }
        }

        struct __ActComponent;

        export!(__ActComponent);

        #session_provider_impl

        impl exports::act::tools::tool_provider::Guest for __ActComponent {
            async fn list_tools(
                _metadata: Vec<(String, Vec<u8>)>,
            ) -> Result<act::tools::types::ListToolsResponse, act::tools::types::Error> {
                Ok(act::tools::types::ListToolsResponse {
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
                        act::tools::types::ToolEvent::Error(act::tools::types::Error {
                            kind: ::act_sdk::constants::ERR_NOT_FOUND.to_string(),
                            message: act::core::types::LocalizedString::Plain(format!("Tool '{}' not found", __other)),
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
            extract_tools_from_item(item, &mut tools)?;
        }
    }

    Ok(tools)
}

fn extract_tools_from_item(item: &Item, tools: &mut Vec<ToolInfo>) -> syn::Result<()> {
    match item {
        Item::Fn(func) => {
            // Find #[act_tool] attribute
            let tool_attr = func.attrs.iter().find(|a| a.path().is_ident("act_tool"));
            if let Some(attr) = tool_attr {
                let attrs = ToolAttrs::from_meta(&attr.meta).map_err(syn::Error::from)?;
                let info = tool::parse_tool_fn(func, attrs)?;
                tools.push(info);
            }
        }
        Item::Macro(mac_item) => {
            // Expand include!("path") and recurse
            if let Some(expanded_items) = expand_include_item(mac_item) {
                for sub_item in &expanded_items {
                    extract_tools_from_item(sub_item, tools)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Collect user items from the module, stripping #[act_tool] attributes from functions.
/// Also rewrites `use super::*` to `use crate::*` since the module body is flattened to top level,
/// and strips #[doc] attributes from function parameters (not allowed by rustc).
/// Expands `include!("path")` macros inline so that tools in separate files are included.
fn collect_user_items(module: &ItemMod) -> Vec<TokenStream> {
    let mut items = Vec::new();

    if let Some((_, mod_items)) = &module.content {
        for item in mod_items {
            collect_user_item(item, &mut items);
        }
    }

    items
}

fn collect_user_item(item: &Item, items: &mut Vec<TokenStream>) {
    match item {
        Item::Fn(func) => {
            // Strip #[act_tool], #[session_open], and #[session_close]
            // attributes but keep the function bodies.
            let mut clean_func = func.clone();
            clean_func.attrs.retain(|a| {
                !a.path().is_ident("act_tool")
                    && !a.path().is_ident("session_open")
                    && !a.path().is_ident("session_close")
            });
            // Strip #[doc] and #[args] attributes from function parameters
            for input in &mut clean_func.sig.inputs {
                if let syn::FnArg::Typed(pat_type) = input {
                    pat_type
                        .attrs
                        .retain(|a| !a.path().is_ident("doc") && !a.path().is_ident("args"));
                }
            }
            items.push(quote! { #clean_func });
        }
        Item::Use(u) => {
            // Rewrite `use super::*` and `use super::Foo` to nothing,
            // since the module is flattened and parent items are at the same level.
            if is_super_use(u) {
                // Skip — the items from "super" are already at crate level
                return;
            }
            items.push(quote! { #u });
        }
        Item::Macro(mac_item) => {
            // Expand include!("path") inline — this is the key mechanism that
            // allows per-module files to define #[act_tool] functions.
            if let Some(expanded_items) = expand_include_item(mac_item) {
                for sub_item in &expanded_items {
                    collect_user_item(sub_item, items);
                }
            } else {
                items.push(quote! { #mac_item });
            }
        }
        other => {
            items.push(quote! { #other });
        }
    }
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
        act::tools::types::ToolDefinition {
            name: #name.to_string(),
            description: act::core::types::LocalizedString::Plain(#desc.to_string()),
            parameters_schema: #schema_expr,
            metadata: vec![#(#metadata_entries),*],
        }
    }
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
                        act::tools::types::ToolEvent::Error(act::tools::types::Error {
                            kind: ::act_sdk::constants::ERR_INVALID_ARGS.to_string(),
                            message: act::core::types::LocalizedString::Plain(format!("Failed to deserialize arguments: {}", e)),
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
                        act::tools::types::ToolEvent::Error(act::tools::types::Error {
                            kind: ::act_sdk::constants::ERR_INVALID_ARGS.to_string(),
                            message: act::core::types::LocalizedString::Plain(format!("Failed to deserialize arguments: {}", e)),
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
                            act::tools::types::ToolEvent::Error(act::tools::types::Error {
                                kind: ::act_sdk::constants::ERR_INVALID_ARGS.to_string(),
                                message: act::core::types::LocalizedString::Plain(format!("Failed to deserialize metadata: {}", e)),
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

    let ok_response = quote! {
        use ::act_sdk::response::{IntoToolResponse as _, IntoToolResponseViaSerialize as _};
        let __response_events = __val.into_tool_response(__default_lang);
    };

    if tool.has_context {
        // Streaming arm: spawn a writer task, return Streaming(reader).
        // Arguments are deserialized up-front (outside the spawn) so that
        // parse errors become an immediate error without starting a stream.
        quote! {
            #tool_name => {
                #deser_code
                let (mut __wit_writer, __reader) = wit_stream::new::<act::tools::types::ToolEvent>();
                wit_bindgen::spawn_local(async move {
                    #metadata_parse
                    let __result = #awaited_call;
                    let __ctx_events = __ctx.__take_events();
                    let mut __wit_events: Vec<act::tools::types::ToolEvent> = __ctx_events
                        .into_iter()
                        .map(|e| __raw_to_wit(e))
                        .collect();
                    match __result {
                        Ok(__val) => {
                            #ok_response
                            __wit_events.extend(__response_events.into_iter().map(|e| __raw_to_wit(e)));
                        }
                        Err(__err) => {
                            __wit_events.push(act::tools::types::ToolEvent::Error(act::tools::types::Error {
                                kind: __err.kind.clone(),
                                message: act::core::types::LocalizedString::Plain(__err.message.clone()),
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
                        let __wit_events: Vec<act::tools::types::ToolEvent> = __response_events
                            .into_iter()
                            .map(|e| __raw_to_wit(e))
                            .collect();
                        exports::act::tools::tool_provider::ToolResult::Immediate(__wit_events)
                    }
                    Err(__err) => exports::act::tools::tool_provider::ToolResult::Immediate(vec![
                        act::tools::types::ToolEvent::Error(act::tools::types::Error {
                            kind: __err.kind.clone(),
                            message: act::core::types::LocalizedString::Plain(__err.message.clone()),
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

// ── session-provider hook extraction ──────────────────────────────────────

/// Captured `#[session_open]` and `#[session_close]` functions.
struct SessionHooks {
    open_fn_ident: syn::Ident,
    open_args_ty: syn::Type,
    open_is_async: bool,
    close_fn_ident: syn::Ident,
    close_is_async: bool,
}

/// Scan the module body for `#[session_open]` and `#[session_close]`
/// markers and capture the function metadata needed to generate the
/// session-provider Guest impl.
///
/// Both hooks are required together. Returns `None` if neither is present;
/// returns an error if exactly one is present (mismatch is a user error).
fn extract_session_hooks(module: &ItemMod) -> syn::Result<Option<SessionHooks>> {
    let Some((_, items)) = &module.content else {
        return Ok(None);
    };

    let mut open: Option<(syn::Ident, syn::Type, bool)> = None;
    let mut close: Option<(syn::Ident, bool)> = None;

    for item in items {
        let Item::Fn(func) = item else { continue };

        let has_open = func.attrs.iter().any(|a| a.path().is_ident("session_open"));
        let has_close = func
            .attrs
            .iter()
            .any(|a| a.path().is_ident("session_close"));

        if has_open && has_close {
            return Err(syn::Error::new_spanned(
                &func.sig.ident,
                "function cannot be both #[session_open] and #[session_close]",
            ));
        }

        if has_open {
            if open.is_some() {
                return Err(syn::Error::new_spanned(
                    &func.sig.ident,
                    "only one #[session_open] function is allowed per component",
                ));
            }
            let (_, args_ty) = parse_open_signature(func)?;
            open = Some((
                func.sig.ident.clone(),
                args_ty,
                func.sig.asyncness.is_some(),
            ));
        }

        if has_close {
            if close.is_some() {
                return Err(syn::Error::new_spanned(
                    &func.sig.ident,
                    "only one #[session_close] function is allowed per component",
                ));
            }
            validate_close_signature(func)?;
            close = Some((func.sig.ident.clone(), func.sig.asyncness.is_some()));
        }
    }

    match (open, close) {
        (Some((oi, oa, o_async)), Some((ci, c_async))) => Ok(Some(SessionHooks {
            open_fn_ident: oi,
            open_args_ty: oa,
            open_is_async: o_async,
            close_fn_ident: ci,
            close_is_async: c_async,
        })),
        (Some((ident, _, _)), None) => Err(syn::Error::new_spanned(
            ident,
            "#[session_open] requires a paired #[session_close] in the same module",
        )),
        (None, Some((ident, _))) => Err(syn::Error::new_spanned(
            ident,
            "#[session_close] requires a paired #[session_open] in the same module",
        )),
        (None, None) => Ok(None),
    }
}

/// Parse `#[session_open]` signature: must be `fn open(args: T) -> ActResult<String>`.
/// Returns the args binding ident and its type.
fn parse_open_signature(func: &syn::ItemFn) -> syn::Result<(syn::Ident, syn::Type)> {
    let mut typed_inputs = func.sig.inputs.iter().filter_map(|i| match i {
        syn::FnArg::Typed(pt) => Some(pt),
        _ => None,
    });
    let Some(arg) = typed_inputs.next() else {
        return Err(syn::Error::new_spanned(
            &func.sig,
            "#[session_open] function must take one args parameter (e.g. `fn open(args: OpenArgs)`)",
        ));
    };
    if typed_inputs.next().is_some() {
        return Err(syn::Error::new_spanned(
            &func.sig,
            "#[session_open] function must take exactly one args parameter",
        ));
    }
    let ident = match arg.pat.as_ref() {
        syn::Pat::Ident(pi) => pi.ident.clone(),
        _ => syn::Ident::new("__args", proc_macro2::Span::call_site()),
    };
    Ok((ident, arg.ty.as_ref().clone()))
}

/// Validate `#[session_close]` signature: must be `fn close(session_id: String)`.
/// Must be sync because the WIT close-session is sync.
fn validate_close_signature(func: &syn::ItemFn) -> syn::Result<()> {
    if func.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            &func.sig,
            "#[session_close] function must be sync (WIT close-session is sync)",
        ));
    }
    let typed_count = func
        .sig
        .inputs
        .iter()
        .filter(|i| matches!(i, syn::FnArg::Typed(_)))
        .count();
    if typed_count != 1 {
        return Err(syn::Error::new_spanned(
            &func.sig,
            "#[session_close] function must take exactly one parameter (`session_id: String`)",
        ));
    }
    Ok(())
}

/// Generate the `act:sessions/session-provider` Guest impl.
fn gen_session_provider_impl(hooks: &SessionHooks) -> TokenStream {
    let open_ident = &hooks.open_fn_ident;
    let close_ident = &hooks.close_fn_ident;
    let open_args_ty = &hooks.open_args_ty;

    let open_call = if hooks.open_is_async {
        quote! { #open_ident(__args).await }
    } else {
        quote! { #open_ident(__args) }
    };

    let _ = hooks.close_is_async; // close must be sync; validated up-front
    let close_call = quote! { #close_ident(session_id) };

    quote! {
        impl exports::act::sessions::session_provider::Guest for __ActComponent {
            async fn get_open_session_args_schema(
                _metadata: Vec<(String, Vec<u8>)>,
            ) -> Result<String, exports::act::sessions::session_provider::Error> {
                let schema = ::act_sdk::__private::schemars::schema_for!(#open_args_ty);
                ::act_sdk::__private::serde_json::to_string(&schema).map_err(|e| {
                    exports::act::sessions::session_provider::Error {
                        kind: ::act_sdk::constants::ERR_INTERNAL.to_string(),
                        message: act::core::types::LocalizedString::Plain(
                            format!("schema serialization failed: {e}")
                        ),
                        metadata: vec![],
                    }
                })
            }

            async fn open_session(
                args: Vec<(String, Vec<u8>)>,
                _metadata: Vec<(String, Vec<u8>)>,
            ) -> Result<act::sessions::types::Session, exports::act::sessions::session_provider::Error> {
                // Re-shape the metadata-style args (Vec<(String, CBOR)>) into a
                // single CBOR map and decode into the user's args type.
                let args_map: ::std::collections::BTreeMap<String, ::act_sdk::__private::serde_json::Value> = args
                    .into_iter()
                    .filter_map(|(k, v)| {
                        ::act_sdk::cbor::from_cbor::<::act_sdk::__private::serde_json::Value>(&v)
                            .ok()
                            .map(|val| (k, val))
                    })
                    .collect();
                let args_json = ::act_sdk::__private::serde_json::to_value(&args_map).unwrap_or(
                    ::act_sdk::__private::serde_json::Value::Object(Default::default())
                );
                let __args: #open_args_ty = match ::act_sdk::__private::serde_json::from_value(args_json) {
                    Ok(v) => v,
                    Err(e) => {
                        return Err(exports::act::sessions::session_provider::Error {
                            kind: ::act_sdk::constants::ERR_INVALID_ARGS.to_string(),
                            message: act::core::types::LocalizedString::Plain(
                                format!("Failed to deserialize session args: {e}")
                            ),
                            metadata: vec![],
                        });
                    }
                };

                match #open_call {
                    Ok(id) => Ok(act::sessions::types::Session {
                        id,
                        metadata: vec![],
                    }),
                    Err(err) => Err(exports::act::sessions::session_provider::Error {
                        kind: err.kind.clone(),
                        message: act::core::types::LocalizedString::Plain(err.message.clone()),
                        metadata: vec![],
                    }),
                }
            }

            fn close_session(session_id: String) {
                let _ = #close_call;
            }
        }
    }
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
