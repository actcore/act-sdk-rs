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
    pub config_type: Option<Type>,
    /// Parsed argument info (excluding ActContext param).
    pub args: Vec<ToolArg>,
    /// Whether arguments come from a single struct type.
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
#[derive(Debug, Default)]
pub struct ToolAttrs {
    pub description: Option<String>,
    pub read_only: bool,
    pub idempotent: bool,
    pub destructive: bool,
    pub streaming: bool,
    pub timeout_ms: Option<u64>,
}

impl ToolAttrs {
    pub fn parse(attr: &syn::Attribute) -> syn::Result<Self> {
        let mut result = ToolAttrs::default();

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("description") {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                result.description = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("read_only") {
                result.read_only = true;
                Ok(())
            } else if meta.path.is_ident("idempotent") {
                result.idempotent = true;
                Ok(())
            } else if meta.path.is_ident("destructive") {
                result.destructive = true;
                Ok(())
            } else if meta.path.is_ident("streaming") {
                // streaming can be a flag or streaming = true
                if meta.input.peek(syn::Token![=]) {
                    let value = meta.value()?;
                    let lit: syn::LitBool = value.parse()?;
                    result.streaming = lit.value();
                } else {
                    result.streaming = true;
                }
                Ok(())
            } else if meta.path.is_ident("timeout_ms") {
                let value = meta.value()?;
                let lit: syn::LitInt = value.parse()?;
                result.timeout_ms = Some(lit.base10_parse()?);
                Ok(())
            } else {
                Err(meta.error("unknown act_tool attribute"))
            }
        })?;

        Ok(result)
    }
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
fn extract_config_type(ty: &Type) -> Option<Type> {
    let inner = match ty {
        Type::Reference(r) => &*r.elem,
        other => other,
    };
    if let Type::Path(tp) = inner {
        if let Some(seg) = tp.path.segments.last() {
            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                if let Some(syn::GenericArgument::Type(t)) = args.args.first() {
                    // Check if it's ()
                    if let Type::Tuple(tuple) = t {
                        if tuple.elems.is_empty() {
                            return None;
                        }
                    }
                    return Some(t.clone());
                }
            }
        }
    }
    None
}

/// Check if a type name looks like a user-defined struct (PascalCase, not a standard type).
fn looks_like_struct_type(ty: &Type) -> bool {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            let name = seg.ident.to_string();
            // Standard types that are NOT user structs
            let standard = [
                "String", "Vec", "Option", "bool", "u8", "u16", "u32", "u64", "i8", "i16",
                "i32", "i64", "f32", "f64", "usize", "isize", "ActContext",
            ];
            if standard.contains(&name.as_str()) {
                return false;
            }
            // Must start with uppercase
            return name.starts_with(|c: char| c.is_uppercase());
        }
    }
    false
}

/// Parse a function with #[act_tool] attributes into ToolInfo.
pub fn parse_tool_fn(func: &ItemFn, attrs: ToolAttrs) -> syn::Result<ToolInfo> {
    let fn_ident = func.sig.ident.clone();
    let tool_name = fn_ident.to_string().replace('_', "-");
    let is_async = func.sig.asyncness.is_some();

    // Collect parameters, identifying ActContext
    let mut args = Vec::new();
    let mut has_context = false;
    let mut config_type = None;
    let mut struct_args = None;

    for input in &func.sig.inputs {
        if let FnArg::Typed(PatType { pat, ty, attrs, .. }) = input {
            if is_act_context(ty) {
                has_context = true;
                config_type = extract_config_type(ty);
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
                    if let syn::Meta::NameValue(nv) = &a.meta {
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(s),
                            ..
                        }) = &nv.value
                        {
                            return Some(s.value().trim().to_string());
                        }
                    }
                    None
                });

            // Check if this single param looks like a struct type
            if looks_like_struct_type(ty) && func.sig.inputs.len() <= 2 {
                // Could be struct-args style (1 struct + maybe ActContext)
                struct_args = Some(ty.as_ref().clone());
            }

            args.push(ToolArg {
                name: param_name,
                ty: ty.as_ref().clone(),
                doc,
            });
        }
    }

    // If we have more than 1 non-context arg, it's individual params style
    if args.len() > 1 {
        struct_args = None;
    }

    let streaming = attrs.streaming || has_context;

    Ok(ToolInfo {
        func: func.clone(),
        tool_name,
        fn_ident,
        description: attrs.description.unwrap_or_default(),
        is_async,
        has_context,
        config_type,
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
    if let syn::ReturnType::Type(_, ty) = ret {
        if let Type::Path(tp) = ty.as_ref() {
            if let Some(seg) = tp.path.segments.last() {
                if seg.ident == "ActResult" || seg.ident == "Result" {
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        if let Some(syn::GenericArgument::Type(t)) = args.args.first() {
                            return Some(t.clone());
                        }
                    }
                }
            }
        }
    }
    None
}
