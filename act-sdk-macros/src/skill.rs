use proc_macro2::{Literal, TokenStream};
use quote::quote;
use std::path::PathBuf;

/// Generate a `#[link_section = "act:skill"]` static from a directory.
pub fn generate(input: TokenStream) -> syn::Result<TokenStream> {
    let lit: syn::LitStr = syn::parse2(input)?;
    let rel_path = lit.value();

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        syn::Error::new(
            lit.span(),
            "CARGO_MANIFEST_DIR not set — must be called from a Cargo build",
        )
    })?;

    let dir = PathBuf::from(&manifest_dir).join(&rel_path);
    if !dir.is_dir() {
        return Err(syn::Error::new(
            lit.span(),
            format!("skill directory not found: {}", dir.display()),
        ));
    }

    let skill_md = dir.join("SKILL.md");
    if !skill_md.exists() {
        return Err(syn::Error::new(
            lit.span(),
            format!("SKILL.md not found in {}", dir.display()),
        ));
    }

    let tar_bytes = pack_tar(&dir)
        .map_err(|e| syn::Error::new(lit.span(), format!("failed to create tar archive: {e}")))?;

    let tar_len = tar_bytes.len();
    let tar_literal = Literal::byte_string(&tar_bytes);

    Ok(quote! {
        #[unsafe(link_section = "act:skill")]
        #[used]
        static __ACT_SKILL_SECTION: [u8; #tar_len] = *#tar_literal;
    })
}

/// Create an uncompressed tar archive from a directory.
fn pack_tar(dir: &std::path::Path) -> Result<Vec<u8>, std::io::Error> {
    let buf = Vec::new();
    let mut ar = tar::Builder::new(buf);

    // Walk directory recursively, add files with relative paths
    add_dir_recursive(&mut ar, dir, &PathBuf::new())?;

    ar.into_inner()
}

fn add_dir_recursive(
    ar: &mut tar::Builder<Vec<u8>>,
    base: &std::path::Path,
    prefix: &std::path::Path,
) -> Result<(), std::io::Error> {
    let mut entries: Vec<_> = std::fs::read_dir(base)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let name = prefix.join(entry.file_name());

        if path.is_dir() {
            add_dir_recursive(ar, &path, &name)?;
        } else {
            ar.append_path_with_name(&path, &name)?;
        }
    }

    Ok(())
}
