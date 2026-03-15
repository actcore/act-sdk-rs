use darling::FromMeta;
use syn::{FnArg, ItemFn, Pat, PatType, Type};

/// Parsed information about a single `#[act_tool]` function.
#[derive(Debug)]
pub struct ToolInfo {
    /// The original function item (with #[act_tool] stripped).
    #[allow(dead_code)]
    pub func: ItemFn,
    /// Tool name (fn name with underscores replaced by hyphens).
    pub tool_name: String,
    /// Rust function identifier.
    pub fn_ident: syn::Ident,
    /// Description from #[act_tool(description = "...")].
    pub description: String,
    /// Whether the tool is async.
    pub is_async: bool,
    /// Whether it has a streaming ActContext parameter.
    pub has_context: bool,
    /// The config type inside ActContext<C> (None if no context or C is ()).
    pub metadata_type: Option<Type>,
    /// Parsed argument info (excluding ActContext param).
    pub args: Vec<ToolArg>,
    /// If a parameter is marked with #[args], its type is used directly
    /// for schema generation and deserialization (no hidden wrapper struct).
    pub struct_args: Option<Type>,
    /// Tool metadata flags.
    pub read_only: bool,
    pub idempotent: bool,
    pub destructive: bool,
    pub streaming: bool,
    pub timeout_ms: Option<u64>,
}

/// A single argument to a tool function (for individual-params style).
#[derive(Debug)]
pub struct ToolArg {
    pub name: String,
    pub ty: Type,
    pub doc: Option<String>,
}

/// Attributes parsed from #[act_tool(...)].
#[derive(Debug, Default, FromMeta)]
pub struct ToolAttrs {
    #[darling(default)]
    pub description: Option<String>,
    #[darling(default)]
    pub read_only: bool,
    #[darling(default)]
    pub idempotent: bool,
    #[darling(default)]
    pub destructive: bool,
    #[darling(default)]
    pub streaming: bool,
    #[darling(default)]
    pub timeout_ms: Option<u64>,
}

/// Check if a type path looks like ActContext<T>.
fn is_act_context(ty: &Type) -> bool {
    if let Type::Reference(r) = ty {
        return is_act_context(&r.elem);
    }
    if let Type::Path(tp) = ty {
        let last = tp.path.segments.last();
        if let Some(seg) = last {
            return seg.ident == "ActContext";
        }
    }
    false
}

/// Extract the type parameter from ActContext<T>.
/// Returns None if it's ActContext<()> or just ActContext.
fn extract_metadata_type(ty: &Type) -> Option<Type> {
    let inner = match ty {
        Type::Reference(r) => &*r.elem,
        other => other,
    };
    if let Type::Path(tp) = inner
        && let Some(seg) = tp.path.segments.last()
        && let syn::PathArguments::AngleBracketed(args) = &seg.arguments
        && let Some(syn::GenericArgument::Type(t)) = args.args.first()
    {
        // Check if it's ()
        if let Type::Tuple(tuple) = t
            && tuple.elems.is_empty()
        {
            return None;
        }
        return Some(t.clone());
    }
    None
}

/// Parse a function with #[act_tool] attributes into ToolInfo.
pub fn parse_tool_fn(func: &ItemFn, attrs: ToolAttrs) -> syn::Result<ToolInfo> {
    let fn_ident = func.sig.ident.clone();
    let tool_name = fn_ident.to_string().replace('_', "-");
    let is_async = func.sig.asyncness.is_some();

    // Collect parameters, identifying ActContext and #[args]
    let mut args = Vec::new();
    let mut has_context = false;
    let mut metadata_type = None;
    let mut struct_args = None;

    for input in &func.sig.inputs {
        if let FnArg::Typed(PatType { pat, ty, attrs, .. }) = input {
            if is_act_context(ty) {
                has_context = true;
                metadata_type = extract_metadata_type(ty);
                continue;
            }

            // Check for #[args] attribute — marks this param as the args struct
            let is_args_param = attrs.iter().any(|a| a.path().is_ident("args"));
            if is_args_param {
                struct_args = Some(ty.as_ref().clone());
                continue;
            }

            // Extract param name
            let param_name = if let Pat::Ident(pi) = pat.as_ref() {
                pi.ident.to_string()
            } else {
                return Err(syn::Error::new_spanned(pat, "expected identifier pattern"));
            };

            // Check for doc attributes
            let doc = attrs
                .iter()
                .find(|a| a.path().is_ident("doc"))
                .and_then(|a| {
                    if let syn::Meta::NameValue(nv) = &a.meta
                        && let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(s),
                            ..
                        }) = &nv.value
                    {
                        return Some(s.value().trim().to_string());
                    }
                    None
                });

            args.push(ToolArg {
                name: param_name,
                ty: ty.as_ref().clone(),
                doc,
            });
        }
    }

    let streaming = attrs.streaming;

    Ok(ToolInfo {
        func: func.clone(),
        tool_name,
        fn_ident,
        description: attrs.description.unwrap_or_default(),
        is_async,
        has_context,
        metadata_type,
        args,
        struct_args,
        read_only: attrs.read_only,
        idempotent: attrs.idempotent,
        destructive: attrs.destructive,
        streaming,
        timeout_ms: attrs.timeout_ms,
    })
}

/// Determine the return type's inner type from ActResult<T>.
/// Returns None if the return type isn't ActResult or Result.
#[allow(dead_code)]
pub fn extract_result_inner_type(ret: &syn::ReturnType) -> Option<Type> {
    if let syn::ReturnType::Type(_, ty) = ret
        && let Type::Path(tp) = ty.as_ref()
        && let Some(seg) = tp.path.segments.last()
        && (seg.ident == "ActResult" || seg.ident == "Result")
        && let syn::PathArguments::AngleBracketed(args) = &seg.arguments
        && let Some(syn::GenericArgument::Type(t)) = args.args.first()
    {
        return Some(t.clone());
    }
    None
}
