mod component;
mod manifest;
mod skill;
mod tool;

use darling::FromMeta;
use proc_macro::TokenStream;

/// Attribute macro for ACT component modules.
///
/// Transforms a module containing `#[act_tool]` functions into a complete
/// WIT component implementation with `wit_bindgen::generate!()`, `export!()`,
/// and a `Guest` trait implementation.
///
/// # Manifest
///
/// Reads `act.toml` from the crate root for component metadata and capabilities.
/// If `act.toml` is absent, all metadata comes from `Cargo.toml`.
///
/// Resolution order: **attribute > act.toml > Cargo.toml**.
///
/// # Attributes
///
/// - `manifest = "..."` — Path to manifest file (default: `"act.toml"`)
/// - `name = "..."` — Override component name
/// - `version = "..."` — Override component version
/// - `description = "..."` — Override component description
/// - `default_language = "..."` — Override BCP 47 language tag
///
/// # Examples
///
/// ```ignore
/// // Reads act.toml, falls back to Cargo.toml:
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
/// // Feature-flag variant with attribute overrides:
/// #[cfg_attr(not(feature = "vec"), act_component)]
/// #[cfg_attr(feature = "vec", act_component(
///     name = "sqlite-vec",
///     description = "SQLite with vector search"
/// ))]
/// mod component { /* ... */ }
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

/// Embed an Agent Skills directory as an `act:skill` WASM custom section.
///
/// Reads the directory at compile time, packs it as an uncompressed tar archive,
/// and emits a `#[link_section = "act:skill"]` static. The directory must contain
/// at least a `SKILL.md` file.
///
/// The path is relative to the crate's `CARGO_MANIFEST_DIR`.
///
/// # Example
///
/// ```ignore
/// act_sdk::embed_skill!("skill/");
/// ```
///
/// See `ACT-AGENTSKILLS.md` for the full specification.
#[proc_macro]
pub fn embed_skill(input: TokenStream) -> TokenStream {
    match skill::generate(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
