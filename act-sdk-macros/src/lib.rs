use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn act_component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item // stub — pass through for now
}

#[proc_macro_attribute]
pub fn act_tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item // stub — pass through for now
}
