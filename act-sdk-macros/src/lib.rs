mod component;
mod tool;

use proc_macro::TokenStream;

/// Attribute macro for ACT component modules.
///
/// Transforms a module containing `#[act_tool]` functions into a complete
/// WIT component implementation with `wit_bindgen::generate!()`, `export!()`,
/// and a `Guest` trait implementation.
///
/// # Attributes
///
/// - `name = "..."` (required) — Component name
/// - `version = "..."` (required) — Component version
/// - `description = "..."` (required) — Component description
/// - `default_language = "..."` (optional, defaults to "en") — BCP 47 language tag
///
/// # Example
///
/// ```ignore
/// #[act_component(
///     name = "my-component",
///     version = "0.1.0",
///     description = "My ACT component",
/// )]
/// mod component {
///     use super::*;
///
///     #[act_tool(description = "Say hello")]
///     fn greet(args: GreetArgs) -> ActResult<String> {
///         Ok(format!("Hello, {}!", args.name))
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn act_component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = match component::ComponentAttrs::parse(attr.into()) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
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
