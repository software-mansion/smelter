use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Expr, ExprLit, ItemFn, Lit, LitStr, Meta, Stmt, Token, parse_macro_input, parse_quote,
    punctuated::Punctuated,
};

#[proc_macro_attribute]
pub fn pipeline_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr with Punctuated::<Meta, Token![,]>::parse_terminated);
    let mut input = parse_macro_input!(item as ItemFn);

    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let const_name = format_ident!("{}", fn_name_str.to_uppercase());

    let mut field_assignments = Vec::with_capacity(args.len());
    let mut snapshot_name_value: Option<Expr> = None;
    for meta in &args {
        match meta {
            Meta::NameValue(nv) => {
                let path = &nv.path;
                let value = &nv.value;
                if path.is_ident("snapshot_name") {
                    snapshot_name_value = Some(value.clone());
                }
                if path.is_ident("description")
                    && let Expr::Lit(ExprLit {
                        lit: Lit::Str(s), ..
                    }) = value
                {
                    let dedented = dedent(&s.value());
                    let new_lit = LitStr::new(&dedented, s.span());
                    field_assignments.push(quote! { #path: #new_lit });
                    continue;
                }
                field_assignments.push(quote! { #path: #value });
            }
            other => {
                let err =
                    syn::Error::new_spanned(other, "expected `name = value`").to_compile_error();
                return err.into();
            }
        }
    }

    if let Some(value) = snapshot_name_value {
        let stmt: Stmt = parse_quote! {
            const OUTPUT_DUMP_FILE: &str = #value;
        };
        input.block.stmts.insert(0, stmt);
    }

    let expanded = quote! {
        #[cfg_attr(test, test)]
        #input

        const #const_name: crate::pipeline_tests::PipelineTest =
            crate::pipeline_tests::PipelineTest {
                test_name: #fn_name_str,
                full_test_name: concat!(module_path!(), "::", #fn_name_str),
                test_fn: #fn_name,
                #(#field_assignments),*
            };
    };

    expanded.into()
}

#[proc_macro_attribute]
pub fn render_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr with Punctuated::<Meta, Token![,]>::parse_terminated);
    let mut input = parse_macro_input!(item as ItemFn);

    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let const_name = format_ident!("{}", fn_name_str.to_uppercase());

    // Inject `MODULE` and `TEST_NAME` consts so `TestRunner::new(MODULE, TEST_NAME)`
    // call sites don't have to repeat the `<module>/<test_name>` literal.
    let test_name_stmt: Stmt = parse_quote! {
        const TEST_NAME: &::std::primitive::str = #fn_name_str;
    };
    let module_stmt: Stmt = parse_quote! {
        const MODULE: &::std::primitive::str =
            crate::render_tests::module_from_module_path(module_path!());
    };
    input.block.stmts.insert(0, test_name_stmt);
    input.block.stmts.insert(0, module_stmt);

    let mut field_assignments = Vec::with_capacity(args.len());
    for meta in &args {
        match meta {
            Meta::NameValue(nv) => {
                let path = &nv.path;
                let value = &nv.value;
                if path.is_ident("description")
                    && let Expr::Lit(ExprLit {
                        lit: Lit::Str(s), ..
                    }) = value
                {
                    let dedented = dedent(&s.value());
                    let new_lit = LitStr::new(&dedented, s.span());
                    field_assignments.push(quote! { #path: #new_lit });
                    continue;
                }
                field_assignments.push(quote! { #path: #value });
            }
            other => {
                let err =
                    syn::Error::new_spanned(other, "expected `name = value`").to_compile_error();
                return err.into();
            }
        }
    }

    let expanded = quote! {
        #[cfg_attr(test, test)]
        #input

        #[allow(dead_code)]
        const #const_name: crate::render_tests::RenderTest =
            crate::render_tests::RenderTest {
                test_name: #fn_name_str,
                full_test_name: concat!(module_path!(), "::", #fn_name_str),
                test_fn: #fn_name,
                module: crate::render_tests::module_from_module_path(module_path!()),
                #(#field_assignments),*
            };
    };

    expanded.into()
}

fn dedent(s: &str) -> String {
    let min_indent = s
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    let dedented: Vec<String> = s
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                line[min_indent..].to_string()
            }
        })
        .collect();

    let mut result = dedented.join("\n");
    let leading_trimmed = result.trim_start_matches('\n').to_string();
    result = leading_trimmed.trim_end().to_string();
    result
}
