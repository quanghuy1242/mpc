use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(attr, item, MacroKind::Test)
}

#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(attr, item, MacroKind::Main)
}

enum MacroKind {
    Test,
    Main,
}

fn expand(attr: TokenStream, item: TokenStream, kind: MacroKind) -> TokenStream {
    if !attr.is_empty() {
        let tokens = TokenStream2::from(attr);
        return syn::Error::new_spanned(
            tokens,
            "core_async attribute macros do not accept arguments yet",
        )
        .to_compile_error()
        .into();
    }

    let input = parse_macro_input!(item as ItemFn);

    if input.sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            input.sig.fn_token,
            "core_async attribute macros require `async fn`",
        )
        .to_compile_error()
        .into();
    }

    let mut sync_sig = input.sig.clone();
    sync_sig.asyncness = None;

    let async_sig = input.sig.clone();

    let attrs_native = input.attrs.clone();
    let attrs_wasm = input.attrs;
    let vis = input.vis.clone();

    let native_block = input.block.clone();
    let wasm_block = input.block;

    match kind {
        MacroKind::Test => expand_test(
            attrs_native,
            attrs_wasm,
            vis,
            sync_sig,
            async_sig,
            native_block,
            wasm_block,
        ),
        MacroKind::Main => expand_main(
            attrs_native,
            attrs_wasm,
            vis,
            sync_sig,
            async_sig,
            native_block,
            wasm_block,
        ),
    }
}

fn expand_test(
    attrs_native: Vec<syn::Attribute>,
    attrs_wasm: Vec<syn::Attribute>,
    vis: syn::Visibility,
    sync_sig: syn::Signature,
    async_sig: syn::Signature,
    native_block: Box<syn::Block>,
    wasm_block: Box<syn::Block>,
) -> TokenStream {
    let vis_native = vis.clone();
    let vis_wasm = vis;

    let native = quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #(#attrs_native)*
        #[test]
        #vis_native #sync_sig {
            core_async::runtime::block_on(async move #native_block)
        }
    };

    let wasm = quote! {
        #[cfg(target_arch = "wasm32")]
        #(#attrs_wasm)*
        #[cfg_attr(
            target_arch = "wasm32",
            core_async::test_support::wasm_bindgen_test
        )]
        #vis_wasm #async_sig #wasm_block
    };

    quote!(#native #wasm).into()
}

fn expand_main(
    attrs_native: Vec<syn::Attribute>,
    attrs_wasm: Vec<syn::Attribute>,
    vis: syn::Visibility,
    sync_sig: syn::Signature,
    async_sig: syn::Signature,
    native_block: Box<syn::Block>,
    wasm_block: Box<syn::Block>,
) -> TokenStream {
    let vis_native = vis.clone();
    let vis_wasm = vis;

    let native = quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #(#attrs_native)*
        #vis_native #sync_sig {
            core_async::runtime::block_on(async move #native_block)
        }
    };

    let wasm = quote! {
        #[cfg(target_arch = "wasm32")]
        #(#attrs_wasm)*
        #vis_wasm #async_sig #wasm_block
    };

    quote!(#native #wasm).into()
}
