use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, Pat, PatType, ReturnType};

/// #[serve] -> ServeFn<S, C>
#[proc_macro_attribute]
pub fn serve(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let f = parse_macro_input!(item as ItemFn);
    expand(f, Kind::Serve)
}

/// #[accept] -> AcceptFn<S, C>
#[proc_macro_attribute]
pub fn accept(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let f = parse_macro_input!(item as ItemFn);
    expand(f, Kind::Accept)
}

enum Kind {
    Serve,
    Accept,
}
fn pat_ident(pt: &PatType) -> &syn::Ident {
    match &*pt.pat {
        Pat::Ident(pi) => &pi.ident,
        _ => panic!("expected simple identifier argument"),
    }
}
fn expand(f: ItemFn, kind: Kind) -> TokenStream {
    let ItemFn { attrs, vis, sig, block } = f;
    let name = &sig.ident;

    let inputs: Vec<&PatType> = sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Typed(pt) => pt,
            FnArg::Receiver(_) => panic!("#[serve]/#[accept] fns can't take self"),
        })
        .collect();

    let ret = match &sig.output {
        ReturnType::Type(_, ty) => quote!(#ty),
        ReturnType::Default => quote!(()),
    };

    match kind {
        Kind::Serve => {
            assert_eq!(inputs.len(), 5, "#[serve] expects (state, addr, route_tx, structure, bytes)");
            let state_pat = &inputs[0].pat;
            let state_ty  = &inputs[0].ty;
            let addr_pat  = &inputs[1].pat; // keep `mut` if present
            let addr_ty   = &inputs[1].ty;
            let route_tx_pat = &inputs[2].pat;
            let route_tx_ty  = &inputs[2].ty;
            let structure_pat = &inputs[3].pat;
            let structure_ty  = &inputs[3].ty;
            let bytes_pat = &inputs[4].pat;
            let bytes_ty  = &inputs[4].ty;

            quote! {
                #(#attrs)*
                #vis fn #name(
                    #state_pat: #state_ty,
                    (#addr_pat, #route_tx_pat): (#addr_ty, #route_tx_ty),
                    #structure_pat: #structure_ty,
                    #bytes_pat: #bytes_ty,
                ) -> ::tfserver::server::handler::ServeFuture
                where
                    ::std::result::Result<::bytes::Bytes, ::bytes::Bytes>: ::std::convert::From<#ret>,
                {
                    ::std::boxed::Box::pin(async move #block)
                }
            }
                .into()
        }
        Kind::Accept => {
            assert_eq!(inputs.len(), 4, "#[accept] expects (state, addr, framed, holder)");
            let state_pat = &inputs[0].pat;
            let state_ty  = &inputs[0].ty;
            let addr_pat  = &inputs[1].pat;
            let addr_ty   = &inputs[1].ty;
            let framed_pat = &inputs[2].pat;
            let framed_ty  = &inputs[2].ty;
            let holder_pat = &inputs[3].pat;
            let holder_ty  = &inputs[3].ty;

            quote! {
                #(#attrs)*
                #vis fn #name(
                    #state_pat: #state_ty,
                    #addr_pat: #addr_ty,
                    (#framed_pat, #holder_pat): (#framed_ty, #holder_ty),
                ) -> ::tfserver::server::handler::AcceptFuture {
                    ::std::boxed::Box::pin(async move #block)
                }
            }
                .into()
        }
    }
}