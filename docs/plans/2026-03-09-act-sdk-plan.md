# ACT Rust Guest SDK Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build `act-sdk`, a Rust guest SDK that lets component authors write ACT components with proc macros instead of manual WIT boilerplate.

**Architecture:** Two-crate workspace — `act-sdk-macros` (proc macro crate) generates `wit_bindgen::generate!()` + `Guest` impl from `#[act_component]`/`#[act_tool]` attributes; `act-sdk` (main crate) provides runtime types (`ActContext`, `ActError`, `IntoResponse` trait) and re-exports the macros. The macro inspects function signatures to determine args style, config presence, and streaming capability.

**Tech Stack:** Rust (nightly, wasm32-wasip2 target), wit-bindgen 0.53, ciborium 0.2, serde/serde_json, schemars 0.8, syn/quote/proc-macro2 for macros.

---

### Task 1: Scaffold workspace and crate structure

**Files:**
- Create: `act-component-sdk-rust/Cargo.toml` (workspace)
- Create: `act-component-sdk-rust/act-sdk/Cargo.toml`
- Create: `act-component-sdk-rust/act-sdk/src/lib.rs`
- Create: `act-component-sdk-rust/act-sdk-macros/Cargo.toml`
- Create: `act-component-sdk-rust/act-sdk-macros/src/lib.rs`
- Create: `act-component-sdk-rust/rust-toolchain.toml`
- Copy: `act-spec/wit/act-core.wit` → `act-component-sdk-rust/wit/act-core.wit`

**Step 1: Create workspace Cargo.toml**

```toml
# act-component-sdk-rust/Cargo.toml
[workspace]
members = ["act-sdk", "act-sdk-macros"]
resolver = "3"
```

**Step 2: Create act-sdk-macros Cargo.toml**

```toml
# act-component-sdk-rust/act-sdk-macros/Cargo.toml
[package]
name = "act-sdk-macros"
version = "0.1.0"
edition = "2024"

[lib]
proc-macro = true

[dependencies]
syn = { version = "2", features = ["full", "extra-traits"] }
quote = "1"
proc-macro2 = "1"
```

**Step 3: Create act-sdk-macros stub**

```rust
// act-component-sdk-rust/act-sdk-macros/src/lib.rs
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn act_component(attr: TokenStream, item: TokenStream) -> TokenStream {
    item // stub — pass through for now
}

#[proc_macro_attribute]
pub fn act_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    item // stub — pass through for now
}
```

**Step 4: Create act-sdk Cargo.toml**

```toml
# act-component-sdk-rust/act-sdk/Cargo.toml
[package]
name = "act-sdk"
version = "0.1.0"
edition = "2024"

[dependencies]
act-sdk-macros = { path = "../act-sdk-macros" }
wit-bindgen = { version = "0.53", features = ["async-spawn"] }
ciborium = "0.2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "0.8"
```

**Step 5: Create act-sdk src/lib.rs stub**

```rust
// act-component-sdk-rust/act-sdk/src/lib.rs
pub use act_sdk_macros::{act_component, act_tool};

pub mod prelude {
    pub use crate::{act_component, act_tool};
    pub use serde::Deserialize;
    pub use schemars::JsonSchema;
}
```

**Step 6: Create rust-toolchain.toml**

```toml
# act-component-sdk-rust/rust-toolchain.toml
[toolchain]
channel = "nightly-2026-03-01"
targets = ["wasm32-wasip2"]
```

**Step 7: Copy WIT file**

```bash
cp act-spec/wit/act-core.wit act-component-sdk-rust/wit/act-core.wit
```

**Step 8: Verify workspace builds**

Run: `cd act-component-sdk-rust && cargo check`
Expected: Compiles with no errors.

**Step 9: Commit**

```bash
git add act-component-sdk-rust/
git commit -m "feat(act-sdk): scaffold workspace with act-sdk and act-sdk-macros crates"
```

---

### Task 2: Implement runtime types in act-sdk

**Files:**
- Modify: `act-component-sdk-rust/act-sdk/src/lib.rs`
- Create: `act-component-sdk-rust/act-sdk/src/types.rs`
- Create: `act-component-sdk-rust/act-sdk/src/context.rs`
- Create: `act-component-sdk-rust/act-sdk/src/response.rs`
- Create: `act-component-sdk-rust/act-sdk/src/cbor.rs`

These types are used by the generated code from macros. They wrap the raw WIT types.

**Step 1: Create cbor.rs — CBOR encode/decode helpers**

```rust
// act-component-sdk-rust/act-sdk/src/cbor.rs

/// Encode a serializable value as CBOR bytes.
pub fn to_cbor<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf).expect("CBOR serialization failed");
    buf
}

/// Decode CBOR bytes into a deserializable value.
pub fn from_cbor<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    ciborium::from_reader(bytes).map_err(|e| format!("CBOR deserialization failed: {e}"))
}
```

**Step 2: Create types.rs — ActError and ActResult**

```rust
// act-component-sdk-rust/act-sdk/src/types.rs

/// Result type for ACT tool functions.
pub type ActResult<T> = Result<T, ActError>;

/// Error type mapping to WIT `tool-error`.
#[derive(Debug, Clone)]
pub struct ActError {
    pub kind: String,
    pub message: String,
}

impl ActError {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("std:not-found", message)
    }

    pub fn invalid_args(message: impl Into<String>) -> Self {
        Self::new("std:invalid-args", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("std:internal", message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new("std:timeout", message)
    }
}

impl std::fmt::Display for ActError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ActError {}
```

**Step 3: Create context.rs — ActContext with stream writing**

```rust
// act-component-sdk-rust/act-sdk/src/context.rs
use crate::types::{ActResult, ActError};
use crate::cbor::to_cbor;

/// Context passed to streaming tool functions.
/// Provides access to deserialized config and stream writing.
pub struct ActContext<C = ()> {
    config: C,
    // writer is stored as an Option so we can take() it when spawning
    // In practice, the generated code provides the writer after construction
    writer: Option<Box<dyn StreamWriter>>,
    default_language: String,
}

/// Internal trait to abstract over the WIT stream writer.
/// This avoids exposing wit_bindgen types in the public API.
pub trait StreamWriter: Send + 'static {
    fn write_event(&mut self, event: RawStreamEvent) -> impl std::future::Future<Output = ()> + Send;
}

/// Raw stream event before conversion to WIT types.
/// Used internally by ActContext; the generated code converts to WIT StreamEvent.
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

impl<C> ActContext<C> {
    /// Create a new context. Called by generated code only.
    #[doc(hidden)]
    pub fn __new(config: C, default_language: String) -> Self {
        Self {
            config,
            writer: None,
            default_language,
        }
    }

    /// Set the stream writer. Called by generated code only.
    #[doc(hidden)]
    pub fn __set_writer(&mut self, writer: Box<dyn StreamWriter>) {
        self.writer = Some(writer);
    }

    /// Access the deserialized config.
    pub fn config(&self) -> &C {
        &self.config
    }

    /// Send a text content event to the stream.
    pub async fn send_text(&mut self, text: impl Into<String>) -> ActResult<()> {
        let text = text.into();
        let data = to_cbor(&text);
        self.send_raw_content(data, Some("text/plain".to_string()), vec![]).await
    }

    /// Send a content part with explicit data, MIME type, and metadata.
    pub async fn send_content(
        &mut self,
        data: Vec<u8>,
        mime_type: Option<String>,
        metadata: Vec<(String, Vec<u8>)>,
    ) -> ActResult<()> {
        self.send_raw_content(data, mime_type, metadata).await
    }

    /// Set progress metadata on the next content event.
    pub async fn send_progress(&mut self, current: u64, total: u64, text: impl Into<String>) -> ActResult<()> {
        let text = text.into();
        let data = to_cbor(&text);
        let metadata = vec![
            ("std:progress".to_string(), to_cbor(&current)),
            ("std:progress-total".to_string(), to_cbor(&total)),
        ];
        self.send_raw_content(data, Some("text/plain".to_string()), metadata).await
    }

    async fn send_raw_content(
        &mut self,
        data: Vec<u8>,
        mime_type: Option<String>,
        metadata: Vec<(String, Vec<u8>)>,
    ) -> ActResult<()> {
        let writer = self.writer.as_mut().ok_or_else(|| {
            ActError::internal("Stream writer not available")
        })?;
        writer.write_event(RawStreamEvent::Content { data, mime_type, metadata }).await;
        Ok(())
    }
}
```

**Step 4: Create response.rs — IntoResponse trait**

```rust
// act-component-sdk-rust/act-sdk/src/response.rs
use crate::cbor::to_cbor;
use crate::context::RawStreamEvent;

/// Trait for types that can be converted into stream events.
/// Implemented for common return types so tool functions can return
/// String, Vec<u8>, () etc. directly.
pub trait IntoResponse {
    fn into_stream_events(self, default_language: &str) -> Vec<RawStreamEvent>;
}

impl IntoResponse for String {
    fn into_stream_events(self, _default_language: &str) -> Vec<RawStreamEvent> {
        vec![RawStreamEvent::Content {
            data: to_cbor(&self),
            mime_type: Some("text/plain".to_string()),
            metadata: vec![],
        }]
    }
}

impl IntoResponse for &str {
    fn into_stream_events(self, default_language: &str) -> Vec<RawStreamEvent> {
        self.to_string().into_stream_events(default_language)
    }
}

impl IntoResponse for () {
    fn into_stream_events(self, _default_language: &str) -> Vec<RawStreamEvent> {
        vec![]
    }
}

impl IntoResponse for Vec<u8> {
    fn into_stream_events(self, _default_language: &str) -> Vec<RawStreamEvent> {
        vec![RawStreamEvent::Content {
            data: self,
            mime_type: None,
            metadata: vec![],
        }]
    }
}

impl IntoResponse for serde_json::Value {
    fn into_stream_events(self, _default_language: &str) -> Vec<RawStreamEvent> {
        vec![RawStreamEvent::Content {
            data: to_cbor(&self),
            mime_type: Some("application/json".to_string()),
            metadata: vec![],
        }]
    }
}
```

**Step 5: Update lib.rs to re-export everything**

```rust
// act-component-sdk-rust/act-sdk/src/lib.rs
pub mod cbor;
pub mod context;
pub mod response;
pub mod types;

pub use act_sdk_macros::{act_component, act_tool};
pub use context::ActContext;
pub use response::IntoResponse;
pub use types::{ActError, ActResult};

pub mod prelude {
    pub use crate::{act_component, act_tool};
    pub use crate::{ActContext, ActError, ActResult, IntoResponse};
    pub use schemars::JsonSchema;
    pub use serde::Deserialize;
}

// Re-export dependencies that generated code needs
#[doc(hidden)]
pub mod __private {
    pub use ciborium;
    pub use schemars;
    pub use serde;
    pub use serde_json;
    pub use wit_bindgen;
}
```

**Step 6: Verify it compiles**

Run: `cd act-component-sdk-rust && cargo check`
Expected: Compiles. (Note: `StreamWriter` trait uses RPITIT which requires nightly — our toolchain is nightly.)

**Step 7: Commit**

```bash
git add act-component-sdk-rust/act-sdk/src/
git commit -m "feat(act-sdk): add runtime types — ActError, ActContext, IntoResponse, CBOR helpers"
```

---

### Task 3: Implement `#[act_tool]` proc macro — function registration and signature parsing

**Files:**
- Modify: `act-component-sdk-rust/act-sdk-macros/src/lib.rs`
- Create: `act-component-sdk-rust/act-sdk-macros/src/tool.rs`

The `#[act_tool]` macro parses the function signature and stores tool metadata for `#[act_component]` to collect. It does NOT generate the `Guest` impl — that's `#[act_component]`'s job.

Strategy: `#[act_tool]` transforms the annotated function and emits a registration item (a const with a known naming convention) that `#[act_component]` will reference.

**Step 1: Create tool.rs — parse tool attributes**

```rust
// act-component-sdk-rust/act-sdk-macros/src/tool.rs
use proc_macro2::TokenStream;
use quote::{quote, format_ident};
use syn::{parse2, ItemFn, FnArg, PatType, Attribute, Expr, Lit, Meta, Type, ReturnType};

/// Parsed metadata from #[act_tool(...)] attributes
pub struct ToolAttrs {
    pub description: String,
    pub read_only: bool,
    pub idempotent: bool,
    pub destructive: bool,
    pub streaming: bool,
    pub timeout_ms: Option<u64>,
}

/// Parsed function signature info
pub enum ArgsStyle {
    /// No arguments
    None,
    /// Individual params — macro generates a hidden struct
    Individual(Vec<IndividualParam>),
    /// Single struct param — user provides the struct with Deserialize + JsonSchema
    Struct(syn::Type),
}

pub struct IndividualParam {
    pub name: syn::Ident,
    pub ty: syn::Type,
    pub doc: Option<String>,
}

pub struct ToolInfo {
    pub fn_name: syn::Ident,
    pub tool_name: String,
    pub attrs: ToolAttrs,
    pub args_style: ArgsStyle,
    pub config_type: Option<syn::Type>,
    pub has_context: bool,
    pub is_async: bool,
    pub return_type: Option<syn::Type>, // None means ActResult<()>
}

pub fn parse_tool_attrs(attr: TokenStream) -> syn::Result<ToolAttrs> {
    // Parse key = value pairs from the attribute
    let meta_list: syn::punctuated::Punctuated<syn::Meta, syn::Token![,]> =
        syn::parse::Parser::parse2(
            syn::punctuated::Punctuated::parse_terminated,
            attr,
        )?;

    let mut description = String::new();
    let mut read_only = false;
    let mut idempotent = false;
    let mut destructive = false;
    let mut streaming = false;
    let mut timeout_ms = None;

    for meta in meta_list {
        match &meta {
            Meta::NameValue(nv) => {
                let key = nv.path.get_ident().map(|i| i.to_string()).unwrap_or_default();
                match key.as_str() {
                    "description" => {
                        if let Expr::Lit(expr_lit) = &nv.value {
                            if let Lit::Str(s) = &expr_lit.lit {
                                description = s.value();
                            }
                        }
                    }
                    "timeout_ms" => {
                        if let Expr::Lit(expr_lit) = &nv.value {
                            if let Lit::Int(n) = &expr_lit.lit {
                                timeout_ms = Some(n.base10_parse()?);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Meta::Path(path) => {
                let key = path.get_ident().map(|i| i.to_string()).unwrap_or_default();
                match key.as_str() {
                    "read_only" => read_only = true,
                    "idempotent" => idempotent = true,
                    "destructive" => destructive = true,
                    "streaming" => streaming = true,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Ok(ToolAttrs {
        description,
        read_only,
        idempotent,
        destructive,
        streaming,
        timeout_ms,
    })
}

/// Check if a type is `ActContext<T>` and extract T
fn extract_context_type(ty: &Type) -> Option<Type> {
    if let Type::Path(type_path) = ty {
        let last_segment = type_path.path.segments.last()?;
        if last_segment.ident == "ActContext" {
            if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return Some(inner.clone());
                }
            }
            // ActContext without generic = ActContext<()>
            return Some(syn::parse_quote!(()));
        }
    }
    None
}

/// Check if a type looks like a user-defined struct (starts with uppercase, not a primitive)
fn looks_like_struct_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let name = segment.ident.to_string();
            // Primitives and common std types are lowercase
            // User structs are PascalCase
            return name.chars().next().map_or(false, |c| c.is_uppercase())
                && name != "String"
                && name != "Vec"
                && name != "Option"
                && name != "ActContext";
        }
    }
    false
}

/// Extract #[doc = "..."] from attributes
fn extract_doc(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Meta::NameValue(nv) = &attr.meta {
                if let Expr::Lit(expr_lit) = &nv.value {
                    if let Lit::Str(s) = &expr_lit.lit {
                        return Some(s.value().trim().to_string());
                    }
                }
            }
        }
    }
    None
}

pub fn parse_tool_fn(item: &ItemFn, attrs: ToolAttrs) -> syn::Result<ToolInfo> {
    let fn_name = item.sig.ident.clone();
    let tool_name = fn_name.to_string().replace('_', "-");
    let is_async = item.sig.asyncness.is_some();

    // Collect typed params (skip self)
    let typed_params: Vec<&PatType> = item.sig.inputs.iter().filter_map(|arg| {
        if let FnArg::Typed(pat) = arg { Some(pat) } else { None }
    }).collect();

    // Check if last param is ActContext<T>
    let (regular_params, has_context, config_type) = if let Some(last) = typed_params.last() {
        if let Some(cfg_ty) = extract_context_type(&last.ty) {
            let regular = &typed_params[..typed_params.len() - 1];
            (regular.to_vec(), true, Some(cfg_ty))
        } else {
            (typed_params.clone(), false, None)
        }
    } else {
        (typed_params.clone(), false, None)
    };

    // Determine args style
    let args_style = if regular_params.is_empty() {
        ArgsStyle::None
    } else if regular_params.len() == 1 && looks_like_struct_type(&regular_params[0].ty) {
        ArgsStyle::Struct((*regular_params[0].ty).clone())
    } else {
        // Individual params — collect names, types, docs
        let params = regular_params.iter().map(|p| {
            let name = if let syn::Pat::Ident(pat_ident) = &*p.pat {
                pat_ident.ident.clone()
            } else {
                return Err(syn::Error::new_spanned(&p.pat, "Expected identifier pattern"));
            };
            let doc = extract_doc(&p.attrs);
            Ok(IndividualParam {
                name,
                ty: (*p.ty).clone(),
                doc,
            })
        }).collect::<syn::Result<Vec<_>>>()?;
        ArgsStyle::Individual(params)
    };

    // Extract return type T from ActResult<T>
    let return_type = if let ReturnType::Type(_, ty) = &item.sig.output {
        if let Type::Path(type_path) = &**ty {
            if let Some(segment) = type_path.path.segments.last() {
                if segment.ident == "ActResult" {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                            Some(inner.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    Ok(ToolInfo {
        fn_name,
        tool_name,
        attrs,
        args_style,
        config_type,
        has_context,
        is_async,
        return_type,
    })
}

/// Generate the registration const and modified function.
/// The const stores tool metadata as a function pointer that #[act_component] calls.
pub fn generate_tool_registration(info: &ToolInfo, original_fn: &ItemFn) -> TokenStream {
    let fn_name = &info.fn_name;
    let tool_name = &info.tool_name;
    let description = &info.attrs.description;
    let reg_name = format_ident!("__ACT_TOOL_{}", fn_name.to_string().to_uppercase());

    // Generate the args struct for individual params style
    let args_struct = match &info.args_style {
        ArgsStyle::Individual(params) => {
            let struct_name = format_ident!("__{}Args", to_pascal_case(&fn_name.to_string()));
            let fields: Vec<TokenStream> = params.iter().map(|p| {
                let name = &p.name;
                let ty = &p.ty;
                let doc = p.doc.as_deref().unwrap_or("");
                if doc.is_empty() {
                    quote! { pub #name: #ty }
                } else {
                    quote! {
                        #[doc = #doc]
                        pub #name: #ty
                    }
                }
            }).collect();
            Some((struct_name.clone(), quote! {
                #[derive(act_sdk::__private::serde::Deserialize, act_sdk::__private::schemars::JsonSchema)]
                #[allow(non_camel_case_types)]
                struct #struct_name {
                    #(#fields),*
                }
            }))
        }
        _ => None,
    };

    // Generate JSON Schema expression
    let schema_expr = match &info.args_style {
        ArgsStyle::None => quote! {
            act_sdk::__private::serde_json::json!({"type": "object"}).to_string()
        },
        ArgsStyle::Struct(ty) => quote! {
            act_sdk::__private::serde_json::to_string(
                &act_sdk::__private::schemars::schema_for!(#ty)
            ).unwrap()
        },
        ArgsStyle::Individual(_) => {
            let struct_name = &args_struct.as_ref().unwrap().0;
            quote! {
                act_sdk::__private::serde_json::to_string(
                    &act_sdk::__private::schemars::schema_for!(#struct_name)
                ).unwrap()
            }
        }
    };

    // Build metadata entries
    let mut metadata_entries = Vec::new();
    if info.attrs.read_only {
        metadata_entries.push(quote! {
            ("std:read-only".to_string(), act_sdk::cbor::to_cbor(&true))
        });
    }
    if info.attrs.idempotent {
        metadata_entries.push(quote! {
            ("std:idempotent".to_string(), act_sdk::cbor::to_cbor(&true))
        });
    }
    if info.attrs.destructive {
        metadata_entries.push(quote! {
            ("std:destructive".to_string(), act_sdk::cbor::to_cbor(&true))
        });
    }
    if info.attrs.streaming {
        metadata_entries.push(quote! {
            ("std:streaming".to_string(), act_sdk::cbor::to_cbor(&true))
        });
    }
    if let Some(ms) = info.attrs.timeout_ms {
        metadata_entries.push(quote! {
            ("std:timeout-ms".to_string(), act_sdk::cbor::to_cbor(&#ms))
        });
    }

    let args_struct_def = args_struct.as_ref().map(|(_, def)| def.clone()).unwrap_or_default();

    // Keep the original function as-is
    let original = original_fn.clone();

    quote! {
        #args_struct_def

        #original

        #[allow(non_upper_case_globals)]
        const #reg_name: () = ();
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + &chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}
```

**Step 2: Wire up the macro entry point**

```rust
// act-component-sdk-rust/act-sdk-macros/src/lib.rs
mod tool;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn act_component(attr: TokenStream, item: TokenStream) -> TokenStream {
    item // still a stub — Task 4
}

#[proc_macro_attribute]
pub fn act_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2 = proc_macro2::TokenStream::from(attr);
    let item2 = proc_macro2::TokenStream::from(item);

    let attrs = match tool::parse_tool_attrs(attr2) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    let item_fn: syn::ItemFn = match syn::parse2(item2) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error().into(),
    };

    let info = match tool::parse_tool_fn(&item_fn, attrs) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error().into(),
    };

    tool::generate_tool_registration(&info, &item_fn).into()
}
```

**Step 3: Verify it compiles**

Run: `cd act-component-sdk-rust && cargo check`
Expected: Compiles.

**Step 4: Commit**

```bash
git add act-component-sdk-rust/act-sdk-macros/
git commit -m "feat(act-sdk): implement #[act_tool] macro — signature parsing and tool registration"
```

---

### Task 4: Implement `#[act_component]` proc macro — Guest trait generation

**Files:**
- Create: `act-component-sdk-rust/act-sdk-macros/src/component.rs`
- Modify: `act-component-sdk-rust/act-sdk-macros/src/lib.rs`
- Modify: `act-component-sdk-rust/act-sdk-macros/src/tool.rs` (make ToolInfo serializable)

This is the core macro. It needs to:
1. Parse component attributes (name, version, description, default_language)
2. Emit `wit_bindgen::generate!()`
3. Generate the `Guest` impl with `get_info`, `get_config_schema`, `list_tools`, `call_tool`

**Challenge:** `#[act_component]` runs on the struct, but `#[act_tool]` runs on separate functions. The component macro needs to know about all tools.

**Solution:** Use an inventory/linkme pattern — or simpler: require all `#[act_tool]` functions to be defined BEFORE the `#[act_component]` struct in the file. The `#[act_component]` macro receives the entire module item (we change it to be applied to a module instead of a bare struct).

**Revised approach:** Apply `#[act_component]` to a module:

```rust
#[act_component(
    name = "weather-tools",
    version = "0.1.0",
    description = "Weather forecast tools",
)]
mod my_component {
    use act_sdk::prelude::*;

    #[act_tool(description = "Greet someone", read_only = true)]
    fn greet(name: String) -> ActResult<String> {
        Ok(format!("Hello, {name}!"))
    }
}
```

This way `#[act_component]` can see all functions inside the module and find the `#[act_tool]` attributes.

**Step 1: Create component.rs**

```rust
// act-component-sdk-rust/act-sdk-macros/src/component.rs
use proc_macro2::TokenStream;
use quote::{quote, format_ident};
use syn::{parse2, ItemMod, ItemFn, Expr, Lit, Meta, Item};
use crate::tool::{parse_tool_attrs, parse_tool_fn, ToolInfo, ArgsStyle};

pub struct ComponentAttrs {
    pub name: String,
    pub version: String,
    pub description: String,
    pub default_language: String,
}

pub fn parse_component_attrs(attr: TokenStream) -> syn::Result<ComponentAttrs> {
    let meta_list: syn::punctuated::Punctuated<syn::Meta, syn::Token![,]> =
        syn::parse::Parser::parse2(
            syn::punctuated::Punctuated::parse_terminated,
            attr,
        )?;

    let mut name = String::new();
    let mut version = String::new();
    let mut description = String::new();
    let mut default_language = "en".to_string();

    for meta in meta_list {
        if let Meta::NameValue(nv) = &meta {
            let key = nv.path.get_ident().map(|i| i.to_string()).unwrap_or_default();
            if let Expr::Lit(expr_lit) = &nv.value {
                if let Lit::Str(s) = &expr_lit.lit {
                    match key.as_str() {
                        "name" => name = s.value(),
                        "version" => version = s.value(),
                        "description" => description = s.value(),
                        "default_language" => default_language = s.value(),
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(ComponentAttrs {
        name,
        version,
        description,
        default_language,
    })
}

pub fn generate_component(attrs: ComponentAttrs, module: ItemMod) -> syn::Result<TokenStream> {
    let mod_name = &module.ident;
    let mod_vis = &module.vis;

    // Extract items from the module
    let (_, items) = module.content.as_ref()
        .ok_or_else(|| syn::Error::new_spanned(&module, "Module must have a body (not `mod foo;`)"))?;

    // Find all #[act_tool] functions and parse them
    let mut tools: Vec<(ToolInfo, ItemFn)> = Vec::new();
    let mut other_items: Vec<&Item> = Vec::new();

    for item in items {
        if let Item::Fn(item_fn) = item {
            // Check if it has #[act_tool(...)]
            let tool_attr = item_fn.attrs.iter().find(|a| a.path().is_ident("act_tool"));
            if let Some(attr) = tool_attr {
                let attr_tokens = match &attr.meta {
                    Meta::List(list) => list.tokens.clone(),
                    _ => TokenStream::new(),
                };
                let tool_attrs = parse_tool_attrs(attr_tokens)?;
                // Clone fn without the #[act_tool] attribute
                let mut clean_fn = item_fn.clone();
                clean_fn.attrs.retain(|a| !a.path().is_ident("act_tool"));
                let info = parse_tool_fn(&clean_fn, tool_attrs)?;
                tools.push((info, clean_fn));
                continue;
            }
        }
        other_items.push(item);
    }

    let comp_name = &attrs.name;
    let comp_version = &attrs.version;
    let comp_description = &attrs.description;
    let default_lang = &attrs.default_language;

    // Generate args structs for individual-param tools
    let args_structs: Vec<TokenStream> = tools.iter().map(|(info, _)| {
        match &info.args_style {
            ArgsStyle::Individual(params) => {
                let struct_name = format_ident!("__{}Args", to_pascal_case(&info.fn_name.to_string()));
                let fields: Vec<TokenStream> = params.iter().map(|p| {
                    let name = &p.name;
                    let ty = &p.ty;
                    let doc = p.doc.as_deref().unwrap_or("");
                    if doc.is_empty() {
                        quote! { pub #name: #ty }
                    } else {
                        quote! {
                            #[doc = #doc]
                            pub #name: #ty
                        }
                    }
                }).collect();
                quote! {
                    #[derive(act_sdk::__private::serde::Deserialize, act_sdk::__private::schemars::JsonSchema)]
                    #[allow(non_camel_case_types)]
                    struct #struct_name {
                        #(#fields),*
                    }
                }
            }
            _ => quote! {},
        }
    }).collect();

    // Generate tool definition entries for list_tools
    let tool_defs: Vec<TokenStream> = tools.iter().map(|(info, _)| {
        let tool_name = &info.tool_name;
        let description = &info.attrs.description;

        let schema_expr = match &info.args_style {
            ArgsStyle::None => quote! {
                act_sdk::__private::serde_json::json!({"type": "object"}).to_string()
            },
            ArgsStyle::Struct(ty) => quote! {
                act_sdk::__private::serde_json::to_string(
                    &act_sdk::__private::schemars::schema_for!(#ty)
                ).unwrap()
            },
            ArgsStyle::Individual(_) => {
                let struct_name = format_ident!("__{}Args", to_pascal_case(&info.fn_name.to_string()));
                quote! {
                    act_sdk::__private::serde_json::to_string(
                        &act_sdk::__private::schemars::schema_for!(#struct_name)
                    ).unwrap()
                }
            }
        };

        let mut metadata_entries = Vec::new();
        if info.attrs.read_only {
            metadata_entries.push(quote! {
                ("std:read-only".to_string(), act_sdk::cbor::to_cbor(&true))
            });
        }
        if info.attrs.idempotent {
            metadata_entries.push(quote! {
                ("std:idempotent".to_string(), act_sdk::cbor::to_cbor(&true))
            });
        }
        if info.attrs.destructive {
            metadata_entries.push(quote! {
                ("std:destructive".to_string(), act_sdk::cbor::to_cbor(&true))
            });
        }
        if info.attrs.streaming {
            metadata_entries.push(quote! {
                ("std:streaming".to_string(), act_sdk::cbor::to_cbor(&true))
            });
        }
        if let Some(ms) = info.attrs.timeout_ms {
            metadata_entries.push(quote! {
                ("std:timeout-ms".to_string(), act_sdk::cbor::to_cbor(&#ms))
            });
        }

        quote! {
            ToolDefinition {
                name: #tool_name.to_string(),
                description: vec![(#default_lang.to_string(), #description.to_string())],
                parameters_schema: #schema_expr,
                metadata: vec![#(#metadata_entries),*],
            }
        }
    }).collect();

    // Generate config schema (find first tool with config, all must use same config type)
    let config_schema_expr = {
        let config_tool = tools.iter().find(|(info, _)| info.config_type.is_some());
        if let Some((info, _)) = config_tool {
            let config_ty = info.config_type.as_ref().unwrap();
            // Skip if config type is ()
            let is_unit = if let syn::Type::Tuple(t) = config_ty {
                t.elems.is_empty()
            } else {
                false
            };
            if is_unit {
                quote! { None }
            } else {
                quote! {
                    Some(act_sdk::__private::serde_json::to_string(
                        &act_sdk::__private::schemars::schema_for!(#config_ty)
                    ).unwrap())
                }
            }
        } else {
            quote! { None }
        }
    };

    // Generate call_tool dispatch arms
    let dispatch_arms: Vec<TokenStream> = tools.iter().map(|(info, _)| {
        let tool_name = &info.tool_name;
        let fn_name = &info.fn_name;

        // Deserialize args
        let args_deser = match &info.args_style {
            ArgsStyle::None => quote! {},
            ArgsStyle::Struct(ty) => quote! {
                let args: #ty = match act_sdk::cbor::from_cbor(&call.arguments) {
                    Ok(a) => a,
                    Err(e) => {
                        let err_event = StreamEvent::Error(ToolError {
                            kind: "std:invalid-args".to_string(),
                            message: vec![(#default_lang.to_string(), e)],
                            metadata: vec![],
                        });
                        writer.write_all(vec![err_event]).await;
                        return;
                    }
                };
            },
            ArgsStyle::Individual(_) => {
                let struct_name = format_ident!("__{}Args", to_pascal_case(&info.fn_name.to_string()));
                quote! {
                    let args: #struct_name = match act_sdk::cbor::from_cbor(&call.arguments) {
                        Ok(a) => a,
                        Err(e) => {
                            let err_event = StreamEvent::Error(ToolError {
                                kind: "std:invalid-args".to_string(),
                                message: vec![(#default_lang.to_string(), e)],
                                metadata: vec![],
                            });
                            writer.write_all(vec![err_event]).await;
                            return;
                        }
                    };
                }
            }
        };

        // Deserialize config if needed
        let config_deser = if info.has_context {
            let config_ty = info.config_type.as_ref().unwrap();
            let is_unit = if let syn::Type::Tuple(t) = config_ty { t.elems.is_empty() } else { false };
            if is_unit {
                quote! {
                    let mut ctx = act_sdk::ActContext::__new((), #default_lang.to_string());
                }
            } else {
                quote! {
                    let config_val: #config_ty = if let Some(ref config_bytes) = config {
                        match act_sdk::cbor::from_cbor(config_bytes) {
                            Ok(c) => c,
                            Err(e) => {
                                let err_event = StreamEvent::Error(ToolError {
                                    kind: "std:invalid-args".to_string(),
                                    message: vec![(#default_lang.to_string(), format!("Invalid config: {e}"))],
                                    metadata: vec![],
                                });
                                writer.write_all(vec![err_event]).await;
                                return;
                            }
                        }
                    } else {
                        let err_event = StreamEvent::Error(ToolError {
                            kind: "std:invalid-args".to_string(),
                            message: vec![(#default_lang.to_string(), "Config required but not provided".to_string())],
                            metadata: vec![],
                        });
                        writer.write_all(vec![err_event]).await;
                        return;
                    };
                    let mut ctx = act_sdk::ActContext::__new(config_val, #default_lang.to_string());
                }
            }
        } else {
            quote! {}
        };

        // Build the function call
        let call_args = {
            let mut args_list = Vec::new();
            match &info.args_style {
                ArgsStyle::None => {}
                ArgsStyle::Struct(_) => {
                    args_list.push(quote! { args });
                }
                ArgsStyle::Individual(params) => {
                    for p in params {
                        let name = &p.name;
                        args_list.push(quote! { args.#name });
                    }
                }
            }
            if info.has_context {
                args_list.push(quote! { ctx });
            }
            args_list
        };

        let await_token = if info.is_async { quote! { .await } } else { quote! {} };

        // Handle return: if has_context (streaming), tool writes to ctx directly
        // If no context, wrap return value via IntoResponse
        if info.has_context {
            quote! {
                #tool_name => {
                    #args_deser
                    #config_deser
                    ctx.__set_writer(/* TODO: wire up writer */);
                    match #fn_name(#(#call_args),*)#await_token {
                        Ok(_) => {}
                        Err(e) => {
                            let err_event = StreamEvent::Error(ToolError {
                                kind: e.kind.clone(),
                                message: vec![(#default_lang.to_string(), e.message.clone())],
                                metadata: vec![],
                            });
                            writer.write_all(vec![err_event]).await;
                        }
                    }
                }
            }
        } else {
            quote! {
                #tool_name => {
                    #args_deser
                    match #fn_name(#(#call_args),*)#await_token {
                        Ok(val) => {
                            use act_sdk::IntoResponse;
                            let events = val.into_stream_events(#default_lang);
                            let wit_events: Vec<StreamEvent> = events.into_iter().map(|e| match e {
                                act_sdk::context::RawStreamEvent::Content { data, mime_type, metadata } => {
                                    StreamEvent::Content(ContentPart {
                                        data,
                                        mime_type,
                                        metadata,
                                    })
                                }
                                act_sdk::context::RawStreamEvent::Error { kind, message, default_language } => {
                                    StreamEvent::Error(ToolError {
                                        kind,
                                        message: vec![(default_language, message)],
                                        metadata: vec![],
                                    })
                                }
                            }).collect();
                            writer.write_all(wit_events).await;
                        }
                        Err(e) => {
                            let err_event = StreamEvent::Error(ToolError {
                                kind: e.kind,
                                message: vec![(#default_lang.to_string(), e.message)],
                                metadata: vec![],
                            });
                            writer.write_all(vec![err_event]).await;
                        }
                    }
                }
            }
        }
    }).collect();

    // The tool functions (cleaned of #[act_tool] attrs)
    let tool_fns: Vec<&ItemFn> = tools.iter().map(|(_, f)| f).collect();

    // WIT path — relative to the crate that uses act-sdk
    // Users will need to vendor the WIT or we provide it via act-sdk
    let wit_path = "wit";

    let struct_name = format_ident!("__ActComponent");

    Ok(quote! {
        #mod_vis mod #mod_name {
            act_sdk::__private::wit_bindgen::generate!({
                path: #wit_path,
                world: "act-world",
            });

            use exports::act::core::tool_provider::Guest;
            use act::core::types::*;

            #(#args_structs)*

            #(#other_items)*

            #(#tool_fns)*

            struct #struct_name;

            export!(#struct_name);

            impl Guest for #struct_name {
                fn get_info() -> ComponentInfo {
                    ComponentInfo {
                        name: #comp_name.to_string(),
                        version: #comp_version.to_string(),
                        default_language: #default_lang.to_string(),
                        description: vec![(#default_lang.to_string(), #comp_description.to_string())],
                        capabilities: vec![],
                        metadata: vec![],
                    }
                }

                fn get_config_schema() -> Option<String> {
                    #config_schema_expr
                }

                async fn list_tools(
                    _config: Option<Vec<u8>>,
                ) -> Result<ListToolsResponse, ToolError> {
                    Ok(ListToolsResponse {
                        metadata: vec![],
                        tools: vec![#(#tool_defs),*],
                    })
                }

                async fn call_tool(
                    config: Option<Vec<u8>>,
                    call: ToolCall,
                ) -> CallResponse {
                    let (mut writer, reader) = wit_stream::new::<StreamEvent>();
                    let call_name = call.name.clone();

                    act_sdk::__private::wit_bindgen::spawn(async move {
                        match call_name.as_str() {
                            #(#dispatch_arms)*
                            other => {
                                let err_event = StreamEvent::Error(ToolError {
                                    kind: "std:not-found".to_string(),
                                    message: vec![(#default_lang.to_string(), format!("Tool '{}' not found", other))],
                                    metadata: vec![],
                                });
                                writer.write_all(vec![err_event]).await;
                            }
                        }
                    });

                    CallResponse {
                        metadata: vec![],
                        body: reader,
                    }
                }
            }
        }
    })
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}
```

**Step 2: Wire up in lib.rs**

```rust
// act-component-sdk-rust/act-sdk-macros/src/lib.rs
mod component;
mod tool;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn act_component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2 = proc_macro2::TokenStream::from(attr);
    let item2 = proc_macro2::TokenStream::from(item);

    let attrs = match component::parse_component_attrs(attr2) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    let module: syn::ItemMod = match syn::parse2(item2) {
        Ok(m) => m,
        Err(e) => return e.to_compile_error().into(),
    };

    match component::generate_component(attrs, module) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn act_tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // When used inside #[act_component] module, this attribute is stripped
    // and processed by the component macro. If used standalone, just pass through.
    item
}
```

**Step 3: Verify it compiles**

Run: `cd act-component-sdk-rust && cargo check`
Expected: Compiles.

**Step 4: Commit**

```bash
git add act-component-sdk-rust/act-sdk-macros/src/
git commit -m "feat(act-sdk): implement #[act_component] macro — Guest trait codegen with dispatch"
```

---

### Task 5: Wire up ActContext stream writer to WIT stream

**Files:**
- Modify: `act-component-sdk-rust/act-sdk/src/context.rs`
- Modify: `act-component-sdk-rust/act-sdk-macros/src/component.rs`

The `ActContext` needs to write to the WIT `StreamWriter<StreamEvent>`. The challenge is that `StreamWriter<StreamEvent>` is a type generated by `wit_bindgen` inside the component module, so `act-sdk` can't reference it directly.

**Solution:** Make `ActContext` generic over a writer closure. The generated code passes a closure that captures the WIT writer.

**Step 1: Refactor context.rs — use a callback-based writer**

Replace the `StreamWriter` trait approach with a simpler design: `ActContext` holds a `Vec<RawStreamEvent>` buffer, and the generated code drains it after the tool function returns (for non-streaming tools). For streaming tools, the generated code wraps `ActContext` methods to write directly via the WIT writer inside the spawned task.

Actually, simplest approach: for non-streaming tools, the function returns a value and the generated code converts it. For streaming tools, the generated code gives the tool function direct access to the WIT writer through a wrapper. Since the streaming tool runs inside `wit_bindgen::spawn()`, it has direct access to the writer.

Revise `ActContext` to hold the writer as an opaque function pointer:

```rust
// act-component-sdk-rust/act-sdk/src/context.rs
use crate::types::{ActResult, ActError};
use crate::cbor::to_cbor;

/// Opaque stream writer handle. The actual writing is done by a closure
/// provided by the generated code, which captures the WIT StreamWriter.
pub struct ActStreamWriter {
    write_fn: Box<dyn FnMut(RawStreamEvent) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>>,
}

impl ActStreamWriter {
    #[doc(hidden)]
    pub fn __new(
        write_fn: impl FnMut(RawStreamEvent) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> + 'static,
    ) -> Self {
        Self { write_fn: Box::new(write_fn) }
    }

    async fn write(&mut self, event: RawStreamEvent) {
        (self.write_fn)(event).await;
    }
}

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

pub struct ActContext<C = ()> {
    config: C,
    writer: Option<ActStreamWriter>,
    default_language: String,
}

impl<C> ActContext<C> {
    #[doc(hidden)]
    pub fn __new(config: C, default_language: String) -> Self {
        Self {
            config,
            writer: None,
            default_language,
        }
    }

    #[doc(hidden)]
    pub fn __set_writer(&mut self, writer: ActStreamWriter) {
        self.writer = Some(writer);
    }

    pub fn config(&self) -> &C {
        &self.config
    }

    pub async fn send_text(&mut self, text: impl Into<String>) -> ActResult<()> {
        let data = to_cbor(&text.into());
        self.write(RawStreamEvent::Content {
            data,
            mime_type: Some("text/plain".to_string()),
            metadata: vec![],
        }).await
    }

    pub async fn send_content(
        &mut self,
        data: Vec<u8>,
        mime_type: Option<String>,
        metadata: Vec<(String, Vec<u8>)>,
    ) -> ActResult<()> {
        self.write(RawStreamEvent::Content { data, mime_type, metadata }).await
    }

    pub async fn send_progress(
        &mut self,
        current: u64,
        total: u64,
        text: impl Into<String>,
    ) -> ActResult<()> {
        let data = to_cbor(&text.into());
        self.write(RawStreamEvent::Content {
            data,
            mime_type: Some("text/plain".to_string()),
            metadata: vec![
                ("std:progress".to_string(), to_cbor(&current)),
                ("std:progress-total".to_string(), to_cbor(&total)),
            ],
        }).await
    }

    async fn write(&mut self, event: RawStreamEvent) -> ActResult<()> {
        let writer = self.writer.as_mut().ok_or_else(|| {
            ActError::internal("Stream writer not available")
        })?;
        writer.write(event).await;
        Ok(())
    }
}
```

**Step 2: Update the generated dispatch for streaming tools in component.rs**

In the dispatch arm for tools with `has_context`, the generated code should create the `ActStreamWriter` that wraps the WIT writer, and pass it to `ActContext`:

The dispatch arm for streaming tools becomes (update the relevant section in `generate_component`):

```rust
// Inside the spawned async block, for streaming tool dispatch:
#tool_name => {
    #args_deser
    #config_deser
    // Create a writer wrapper that converts RawStreamEvent to WIT StreamEvent
    let writer_ref = std::rc::Rc::new(std::cell::RefCell::new(Some(writer)));
    let writer_rc = writer_ref.clone();
    let lang = #default_lang.to_string();
    let stream_writer = act_sdk::context::ActStreamWriter::__new(move |event| {
        let writer_rc = writer_rc.clone();
        let lang = lang.clone();
        Box::pin(async move {
            let wit_event = match event {
                act_sdk::context::RawStreamEvent::Content { data, mime_type, metadata } => {
                    StreamEvent::Content(ContentPart { data, mime_type, metadata })
                }
                act_sdk::context::RawStreamEvent::Error { kind, message, default_language } => {
                    StreamEvent::Error(ToolError {
                        kind,
                        message: vec![(default_language, message)],
                        metadata: vec![],
                    })
                }
            };
            if let Some(w) = writer_rc.borrow_mut().as_mut() {
                w.write_all(vec![wit_event]).await;
            }
        })
    });
    ctx.__set_writer(stream_writer);
    match #fn_name(#(#call_args),*)#await_token {
        Ok(_) => {}
        Err(e) => {
            if let Some(w) = writer_ref.borrow_mut().as_mut() {
                let err_event = StreamEvent::Error(ToolError {
                    kind: e.kind,
                    message: vec![(#default_lang.to_string(), e.message)],
                    metadata: vec![],
                });
                w.write_all(vec![err_event]).await;
            }
        }
    }
}
```

**Step 3: Verify it compiles**

Run: `cd act-component-sdk-rust && cargo check`
Expected: Compiles.

**Step 4: Commit**

```bash
git add act-component-sdk-rust/act-sdk/src/ act-component-sdk-rust/act-sdk-macros/src/
git commit -m "feat(act-sdk): wire ActContext stream writer to WIT StreamWriter"
```

---

### Task 6: Create example component using the SDK

**Files:**
- Create: `act-component-sdk-rust/examples/hello-sdk/Cargo.toml`
- Create: `act-component-sdk-rust/examples/hello-sdk/src/lib.rs`
- Create: `act-component-sdk-rust/examples/hello-sdk/wit/` (symlink or copy)

This example validates that the SDK works end-to-end. It reimplements the hello-world and counter examples using the SDK.

**Step 1: Create Cargo.toml**

```toml
# act-component-sdk-rust/examples/hello-sdk/Cargo.toml
[package]
name = "hello-sdk"
version = "0.1.0"
edition = "2024"

[dependencies]
act-sdk = { path = "../../act-sdk" }
serde = { version = "1", features = ["derive"] }
schemars = "0.8"

[lib]
crate-type = ["cdylib"]
```

**Step 2: Create the example component**

```rust
// act-component-sdk-rust/examples/hello-sdk/src/lib.rs
use act_sdk::prelude::*;

#[derive(Deserialize, JsonSchema)]
struct GreetArgs {
    /// Name of the person to greet
    name: String,
}

#[act_component(
    name = "hello-sdk",
    version = "0.1.0",
    description = "Hello world using act-sdk",
)]
mod component {
    use super::*;

    #[act_tool(description = "Say hello to someone", read_only = true)]
    fn greet(args: GreetArgs) -> ActResult<String> {
        Ok(format!("Hello, {}!", args.name))
    }

    #[act_tool(description = "List supported greetings", read_only = true)]
    fn list_greetings() -> ActResult<String> {
        Ok("hello, hi, hey, greetings".to_string())
    }

    #[act_tool(description = "Count from 1 to N", streaming = true)]
    async fn count(
        #[doc = "Number to count to"] n: u32,
        ctx: ActContext<()>,
    ) -> ActResult<()> {
        for i in 1..=n {
            ctx.send_progress(i as u64, n as u64, format!("Count: {i}")).await?;
        }
        Ok(())
    }
}
```

**Step 3: Add WIT files**

```bash
cp -r act-component-sdk-rust/wit act-component-sdk-rust/examples/hello-sdk/wit
```

**Step 4: Add example to workspace**

Update `act-component-sdk-rust/Cargo.toml`:
```toml
[workspace]
members = ["act-sdk", "act-sdk-macros", "examples/hello-sdk"]
resolver = "3"
```

**Step 5: Try to compile for wasm32-wasip2**

Run: `cd act-component-sdk-rust && cargo build --target wasm32-wasip2 -p hello-sdk`
Expected: May fail — iterate on macro output until it compiles.

**Step 6: Fix issues and iterate**

This is the integration step. Likely issues:
- Import paths inside generated module
- `wit_stream`, `wit_bindgen::spawn` availability
- Type mismatches between SDK types and WIT types
- Async/Send bounds in wasm context

Debug by expanding the macro output:
Run: `cd act-component-sdk-rust && cargo expand -p hello-sdk --target wasm32-wasip2 2>&1 | head -200`

Fix each issue in the macro crate until the example compiles.

**Step 7: Commit**

```bash
git add act-component-sdk-rust/examples/ act-component-sdk-rust/Cargo.toml
git commit -m "feat(act-sdk): add hello-sdk example component"
```

---

### Task 7: Test with act-host

**Files:** None new — uses existing act-host.

**Step 1: Build the example component to .wasm**

Run: `cd act-component-sdk-rust && cargo build --target wasm32-wasip2 -p hello-sdk --release`

**Step 2: Run it with act-host**

Run: `cd act-host && cargo run -- call ../act-component-sdk-rust/target/wasm32-wasip2/release/hello_sdk.wasm --tool greet --args '{"name":"SDK"}'`
Expected: `Hello, SDK!`

**Step 3: Test the streaming tool**

Run: `cd act-host && cargo run -- call ../act-component-sdk-rust/target/wasm32-wasip2/release/hello_sdk.wasm --tool count --args '{"n":3}'`
Expected: Three events with progress.

**Step 4: Test the no-args tool**

Run: `cd act-host && cargo run -- call ../act-component-sdk-rust/target/wasm32-wasip2/release/hello_sdk.wasm --tool list-greetings --args '{}'`
Expected: `hello, hi, hey, greetings`

**Step 5: Commit (if any fixes needed)**

```bash
git add -A
git commit -m "fix(act-sdk): fixes from integration testing with act-host"
```

---

### Task 8: Add config example

**Files:**
- Create: `act-component-sdk-rust/examples/config-sdk/Cargo.toml`
- Create: `act-component-sdk-rust/examples/config-sdk/src/lib.rs`

**Step 1: Create a component that uses config**

```rust
// act-component-sdk-rust/examples/config-sdk/src/lib.rs
use act_sdk::prelude::*;

#[derive(Deserialize, JsonSchema)]
struct AppConfig {
    /// API key for authentication
    api_key: String,
    /// Optional greeting prefix
    #[serde(default = "default_prefix")]
    prefix: String,
}

fn default_prefix() -> String {
    "Hello".to_string()
}

#[derive(Deserialize, JsonSchema)]
struct GreetArgs {
    name: String,
}

#[act_component(
    name = "config-example",
    version = "0.1.0",
    description = "Example with config",
)]
mod component {
    use super::*;

    #[act_tool(description = "Greet with configured prefix")]
    fn greet(args: GreetArgs, ctx: ActContext<AppConfig>) -> ActResult<String> {
        let config = ctx.config();
        Ok(format!("{}, {}! (key: {}...)", config.prefix, args.name, &config.api_key[..3]))
    }
}
```

**Step 2: Build and test with act-host**

Run: `cargo build --target wasm32-wasip2 -p config-sdk --release`
Then test via act-host HTTP server with `X-ACT-Config` header.

**Step 3: Commit**

```bash
git add act-component-sdk-rust/examples/config-sdk/
git commit -m "feat(act-sdk): add config example component"
```
