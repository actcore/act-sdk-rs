mod component;
mod tool;

use darling::FromMeta;
use proc_macro::TokenStream;

/// Attribute macro for ACT component modules.
///
/// Transforms a module containing `#[act_tool]` functions into a complete
/// WIT component implementation with `wit_bindgen::generate!()`, `export!()`,
/// and a `Guest` trait implementation.
///
/// # Attributes
///
/// All attributes are optional — defaults are taken from `Cargo.toml`:
///
/// - `name = "..."` — Component name (default: `CARGO_PKG_NAME`)
/// - `version = "..."` — Component version (default: `CARGO_PKG_VERSION`)
/// - `description = "..."` — Component description (default: `CARGO_PKG_DESCRIPTION`)
/// - `default_language = "..."` — BCP 47 language tag (default: `"en"`)
///
/// # Examples
///
/// ```ignore
/// // All fields from Cargo.toml:
/// #[act_component]
/// mod component {
///     use super::*;
///
///     #[act_tool(description = "Say hello")]
///     fn greet(name: String) -> ActResult<String> {
///         Ok(format!("Hello, {name}!"))
///     }
/// }
///
/// // Override just the name:
/// #[act_component(name = "custom-name")]
/// mod component {
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn act_component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_stream: proc_macro2::TokenStream = attr.into();
    let attr_args = if attr_stream.is_empty() {
        Vec::new()
    } else {
        match darling::ast::NestedMeta::parse_meta_list(attr_stream) {
            Ok(a) => a,
            Err(e) => return TokenStream::from(darling::Error::from(e).write_errors()),
        }
    };
    let attrs = match component::ComponentAttrs::from_list(&attr_args) {
        Ok(a) => a,
        Err(e) => return TokenStream::from(e.write_errors()),
    };
    let module = match syn::parse::<syn::ItemMod>(item) {
        Ok(m) => m,
        Err(e) => return e.to_compile_error().into(),
    };
    match component::generate(attrs, &module) {
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
