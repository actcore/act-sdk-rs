mod component;
mod tool;

use proc_macro::TokenStream;

/// Attribute macro for ACT component modules. Takes no arguments.
///
/// Transforms a module of `#[act_tool]` functions into a complete WIT component
/// implementation (wit-bindgen world, `Guest` impl, `list_tools`/`call_tool`
/// dispatch, plus session-provider when `#[session_open]`/`#[session_close]` are
/// present). Component metadata and skills are embedded by `act-build pack`, not
/// this macro.
#[proc_macro_attribute]
pub fn act_component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_stream: proc_macro2::TokenStream = attr.into();
    if !attr_stream.is_empty() {
        return syn::Error::new_spanned(
            &attr_stream,
            "#[act_component] takes no arguments; component metadata comes from act.toml via `act-build pack`",
        )
        .to_compile_error()
        .into();
    }
    let module = match syn::parse::<syn::ItemMod>(item) {
        Ok(m) => m,
        Err(e) => return e.to_compile_error().into(),
    };
    match component::generate(&module) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Attribute macro for ACT tool functions.
///
/// When used inside an `#[act_component]` module, marks a function as a tool.
/// The `#[act_component]` macro processes these attributes during code generation.
///
/// When used standalone (outside `#[act_component]`), this is a no-op pass-through.
///
/// # Attributes
///
/// - `description = "..."` (required) — Tool description
/// - `read_only` — Mark tool as read-only
/// - `idempotent` — Mark tool as idempotent
/// - `destructive` — Mark tool as destructive
/// - `streaming` — Mark tool as streaming (auto-detected if ActContext param present)
/// - `timeout_ms = N` — Set timeout in milliseconds
#[proc_macro_attribute]
pub fn act_tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // When used standalone, pass through unchanged.
    // When inside #[act_component], the component macro processes this.
    item
}

/// Mark a function as `act:sessions/session-provider.open-session`.
///
/// Inside `#[act_component]`, the component macro picks up this annotation,
/// generates the `session-provider` Guest impl, and derives the
/// `get-open-session-args-schema` JSON Schema from the function's argument
/// type via `schemars::JsonSchema`.
///
/// Signature: `fn open(args: T) -> ActResult<String>` (sync or async). `T`
/// must implement `serde::Deserialize` and `schemars::JsonSchema`. The
/// returned `String` is the session-id the host will use in subsequent
/// capability calls.
///
/// Outside `#[act_component]`, this attribute is a no-op pass-through.
#[proc_macro_attribute]
pub fn session_open(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Mark a function as `act:sessions/session-provider.close-session`.
///
/// Signature: `fn close(session_id: String)`. Synchronous, no return value
/// (matches the WIT close-session signature).
///
/// Outside `#[act_component]`, this attribute is a no-op pass-through.
#[proc_macro_attribute]
pub fn session_close(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
