use std::collections::HashMap;
use std::iter::once;

use convert_case::{Case, Casing};

use itertools::Itertools;
use proc_macro2::{self, Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    punctuated::Punctuated, spanned::Spanned, token::Comma, Error, ImplItem, ImplItemFn, ItemImpl,
    PatType, Receiver,
};
use syn::{File, LitStr, Meta, MetaList, ReturnType};

enum RefType {
    Shared,
    Mut,
}

struct ParseFn {
    fn_name: Ident,
    ref_type: Option<RefType>,
    args: Vec<PatType>,
    output: ReturnType,
}
pub type PackageLookup = HashMap<String, String>;

pub fn fill_lookup(rust_code: &str, lookup: &mut PackageLookup) -> Result<(), syn::Error> {
    let syntax_tree: File = syn::parse_str(rust_code)?;
    let mut visitor = ImplVisitor { impls: Vec::new() };
    visitor.visit_file(&syntax_tree);
    let java_classes = visitor.impls.iter().flat_map(|impl_item| {
        impl_item.attrs.iter().filter_map(|attr| match &attr.meta {
            Meta::List(MetaList { path, tokens, .. })
                if path
                    .get_ident()
                    .map(|ident| ident.to_string().as_str() == "java_class")
                    .unwrap_or(false) =>
            {
                Some(
                    syn::parse2::<LitStr>(tokens.clone())
                        .map(|res| (impl_item.self_ty.to_token_stream(), res.value().to_string())),
                )
            }
            _ => None,
        })
    });
    for res in java_classes {
        let (self_ty, path) = res?;
        let self_ty = self_ty.to_string();

        if lookup
            .insert(self_ty.clone(), format!("{path}.{self_ty}"))
            .is_some()
        {
            return Err(syn::Error::new(
                Span::call_site(),
                format!("Found more than one #[java_class] for struct_name {self_ty}",),
            ));
        }
    }
    Ok(())
}

fn parse_fn(path: &str, ty: &TokenStream, input: &ImplItemFn) -> Result<ParseFn, syn::Error> {
    let input = input.clone();
    let fn_name = input.sig.ident;
    let args = input.sig.inputs;
    let output = input.sig.output;
    let mut self_ty: Option<RefType> = None;
    let mut rest_args: Vec<PatType> = Vec::with_capacity(args.len());
    for arg in args.iter() {
        match arg {
            syn::FnArg::Receiver(Receiver {
                self_token: _self_token,
                reference,
                mutability,
                ..
            }) => {
                // comment
                reference.as_ref().ok_or_else(|| {
                    syn::Error::new(Span::call_site(), "Must only call self as reference Boop")
                })?;
                let self_call = match mutability {
                    Some(_) => RefType::Mut,
                    None => RefType::Shared,
                };
                self_ty = Some(self_call)
            }
            syn::FnArg::Typed(pat @ PatType { .. }) => {
                rest_args.push(pat.clone());
            }
        }
    }
    Ok(ParseFn {
        fn_name,
        ref_type: self_ty,
        args: rest_args,
        output,
    })
}

fn get_kotlin_output<'a>(
    output: &'a str,
    self_ty_str: &'a str,
    lookup: &'a PackageLookup,
) -> Result<(&'a str, bool), syn::Error> {
    let res = if output == "Self" {
        (
            lookup
                .get(self_ty_str)
                .ok_or_else(|| {
                    syn::Error::new(
                        Span::call_site(),
                        format!(
                            "Failed to find {self_ty_str} in java_class lookup. This is a bug."
                        ),
                    )
                })?
                .as_str(),
            true,
        )
    } else if let Some(package) = lookup.get(output) {
        (package.as_str(), true)
    } else {
        (map_kotlin_type_from_rust(output)?, false)
    };
    Ok(res)
}

fn kotlin_class_method(
    path: &str,
    self_ty: &TokenStream,
    input: &ImplItemFn,
    lookup: &PackageLookup,
) -> Result<Option<String>, syn::Error> {
    let ParseFn {
        fn_name,
        ref_type,
        args,
        output,
    } = parse_fn(path, self_ty, input)?;
    let Some(ref_type) = ref_type else {
        return Ok(None);
    };
    let self_ty_str = self_ty.to_string();
    let kotlin_args_iter = args.iter().map(|PatType { pat, .. }| {
        let var = pat.to_token_stream().to_string().to_case(Case::Camel);
        format!("{var}")
    });

    let mut kotlin_args_with_types_iter = args.iter().map(|PatType { pat, ty, .. }| {
        let var = pat.to_token_stream().to_string().to_case(Case::Camel);
        let ty = ty.to_token_stream().to_string();
        let kotlin_type = if ty == self_ty_str {
            &self_ty_str
        } else {
            map_kotlin_type_from_rust(&ty).expect("Failed to create type")
        };
        format!("{var}: {kotlin_type}")
    });
    let j_args = once("handle".to_string())
        .chain(kotlin_args_iter)
        .join(", ");

    let j_args_with_types = kotlin_args_with_types_iter.join(", ");

    let output = match output {
        ReturnType::Default => String::new(),
        ReturnType::Type(_, ty) => ty.to_token_stream().to_string(),
    };

    let (output_class, cleanup) = get_kotlin_output(&output, &self_ty_str, lookup)?;
    let j_fn_name = fn_name.to_string().to_case(Case::Camel);
    Ok(Some(kotlin_class_fn(
        &self_ty_str,
        &j_fn_name,
        &j_args_with_types,
        &j_args,
        &output,
        output_class,
        cleanup,
    )))
}

fn kotlin_companion_method(
    path: &str,
    self_ty: &TokenStream,
    input: &ImplItemFn,
    lookup: &PackageLookup,
) -> Result<Option<String>, syn::Error> {
    let ParseFn {
        fn_name,
        ref_type,
        args,
        output,
    } = parse_fn(path, self_ty, input)?;
    let self_ty_str = self_ty.to_string();
    let kotlin_args_iter = args.iter().map(|PatType { pat, ty, .. }| {
        let var = pat.to_token_stream().to_string().to_case(Case::Camel);
        let ty = ty.to_token_stream().to_string();
        let kotlin_type = if ty == self_ty_str {
            &self_ty_str
        } else {
            map_kotlin_type_from_rust(&ty).expect("Failed to create type")
        };
        format!("{var}: {kotlin_type}")
    });
    let j_args_with_types = ref_type
        .as_ref()
        .map(|_| format!("handle: Long"))
        .into_iter()
        .chain(kotlin_args_iter)
        .join(", ");

    let output = match output {
        ReturnType::Default => String::new(),
        ReturnType::Type(_, ty) => ty.to_token_stream().to_string(),
    };

    let (output_package, cleanup) = get_kotlin_output(&output, &self_ty_str, lookup)?;
    let j_fn_name = fn_name.to_string().to_case(Case::Camel);
    let public_fn = if ref_type.as_ref().is_none() {
        let j_args = args
            .iter()
            .map(|PatType { pat, .. }| {
                let var = pat.to_token_stream().to_string().to_case(Case::Camel);
                format!("{var}")
            })
            .join(", ");

        kotlin_static_fn(
            &self_ty_str,
            &j_fn_name,
            &j_args_with_types,
            &j_args,
            &output,
            output_package,
            cleanup,
        )
    } else {
        String::new()
    };

    let private_static_native_fn =
        kotlin_private_native_fn(&j_fn_name, &j_args_with_types, output_package);
    Ok(Some(format!(
        r#"
        {public_fn}
        {private_static_native_fn}

                "#
    )))
}

fn java_method_fn(
    path: &str,
    ty: &TokenStream,
    input: &ImplItemFn,
) -> Result<TokenStream, syn::Error> {
    let ParseFn {
        fn_name,
        ref_type,
        args,
        output,
    } = parse_fn(path, ty, input)?;
    let java_fn_name = fn_name.to_string().to_case(Case::Camel);
    let java_fn_name = format!("{java_fn_name}Extern");
    let java_ident = Ident::new(&java_fn_name, Span::call_site()).to_token_stream();
    let j_output = match output {
        syn::ReturnType::Type(_, ty) => {
            quote! {<#ty as JType>::JType<'local>}
        }
        x => x.to_token_stream(),
    };

    let j_args: Vec<TokenStream> = args
        .iter()
        .map(|PatType { pat, ty, .. }| {
            let j_arg = format!("j_{}", pat.into_token_stream());
            let j_arg = Ident::new(&j_arg, Span::call_site()).to_token_stream();
            quote! {#j_arg: <#ty as JType>::JType<'local>}
        })
        .collect();
    let to_rust_types: Vec<TokenStream> = args.iter().map(|PatType { pat, ty , ..}| {
                let rest_arg = pat.to_token_stream();
            let j_arg = format!("j_{}", pat.into_token_stream());
            let j_arg = Ident::new(&j_arg, Span::call_site()).to_token_stream();
                 quote! {
                    let #rest_arg: #ty  = PrimitiveJType::from_j_type(&mut env, #j_arg).expect("Failed to cast");
                }

    }).collect();

    let j_args: Punctuated<TokenStream, Comma> = once(quote! {mut env: jni::JNIEnv<'local>})
        .chain(once(quote! {class: jni::objects::JClass<'local>}))
        .chain(ref_type.as_ref().map(|_| quote! {handle: jni::sys::jlong}))
        .chain(j_args)
        .collect();
    let call_args: Punctuated<TokenStream, Comma> = args
        .into_iter()
        .map(|PatType { pat, .. }| pat.to_token_stream())
        .collect();
    let transforms: TokenStream = to_rust_types.into_iter().collect();
    let fn_call = ref_type
        .map(|ref_type| match ref_type {
            RefType::Shared => quote! {
                <#ty as JavaClass>::use_shared(handle, |self_type| self_type.#fn_name(#call_args))
            },
            RefType::Mut => quote! {
                <#ty as JavaClass>::use_mut(handle, |self_type| self_type.#fn_name(#call_args))
            },
        })
        .unwrap_or_else(|| quote! {#ty::#fn_name(#call_args)});

    Ok(quote! {
        #[jni_fn(#path)]
        pub fn #java_ident<'local>(#j_args) -> #j_output {
            #transforms
            #fn_call.to_j_type(&mut env).expect("Failed to cast")
        }
    })
}

pub fn java_class_fn(attr: TokenStream, item: TokenStream) -> Result<TokenStream, syn::Error> {
    let attr_span = attr.span();
    let item_span = item.span();
    let impl_name: syn::ItemImpl = match syn::parse2(item.clone()) {
        Ok(s) => s,
        Err(_err) => {
            return Err(syn::Error::new(
                item_span,
                "The `java_class` attribute can only be applied to `impl` items",
            ))
        }
    };
    let struct_name = impl_name.self_ty.to_token_stream();

    let namespace = match syn::parse2::<syn::LitStr>(attr) {
        Ok(n) => n,
        Err(_e) => return Err(syn::Error::new(attr_span, "The `java_class` attribute must have a single string literal supplied to specify the class path")),
    }.value();

    let namespace = format!("{namespace}.{struct_name}");
    let namepath = namespace.replace('.', "/");
    let struct_n = struct_name;

    let fns: TokenStream = impl_name
        .items
        .iter()
        .filter_map(|impl_item| match impl_item {
            ImplItem::Fn(fn_item) => Some(fn_item),
            _ => None,
        })
        .map(|fn_item| java_method_fn(&namespace, &struct_n, fn_item))
        .collect::<Result<_, syn::Error>>()?;

    Ok(quote! {

        #item

        use jni_cli_core::*;
        unsafe impl JavaClass for #struct_n {
            const LOC: &'static str = #namespace;
            const PATH: &'static str = #namepath;
        }
        #fns

        #[jni_fn(#namespace)]
        pub fn dropByHandle<'local>(env: jni::JNIEnv<'local>, _class: jni::objects::JClass<'local>, handle: jni::sys::jlong) {}
    })
}

fn map_jni_type(ident: Ident) -> Result<&'static str, syn::Error> {
    let ret = match ident.to_string().as_ref() {
        "jboolean" => "Boolean",
        "jbyte" => "Byte",
        "jshort" => "Short",
        "jint" => "Int",
        "jlong" => "Long",
        "jfloat" => "Float",
        "jdouble" => "Double",
        "JString" => "String",
        "JBooleanArray" => "BooleanArray",
        "JByteArray" => "ByteArray",
        "JShortArray" => "ShortArray",
        "JIntArray" => "IntArray",
        "JLongArray" => "LongArray",
        "JFloatArray" => "FloutArray",
        "JDoubleArray" => "DoubleArray",
        "JObjectArray" => "ObjectArray",
        _ => return Err(syn::Error::new(Span::call_site(), "Shouldn't be here")),
    };
    Ok(ret)
}

fn map_kotlin_type_from_rust(ident: &str) -> Result<&'static str, syn::Error> {
    let ret = match ident.to_string().as_ref() {
        "bool" => "Boolean",
        "i8" => "Byte",
        "i16" => "Short",
        "i32" => "Int",
        "i64" => "Long",
        "f32" => "Float",
        "f63" => "Double",
        "String" => "String",
        "Vec < bool >" => "BooleanArray",
        "Vec < i8 >" => "ByteArray",
        "Vec < u8 >" => "ByteArray",
        "Vec < i16 >" => "ShortArray",
        "Vec < i32 >" => "IntArray",
        "Vec < i64 >" => "LongArray",
        "Vec < f32 >" => "FloutArray",
        "Vec < f64 >" => "DoubleArray",
        "Vec < String >" => "Array<String>",
        x => {
            return Err(syn::Error::new(
                Span::call_site(),
                format!("Shouldn't be here: {x}: result {}", x == "Vec < u8 >"),
            ))
        }
    };
    Ok(ret)
}
fn map_jni_type_from_rust(ident: Ident) -> Result<&'static str, syn::Error> {
    let ret = match ident.to_string().as_ref() {
        "bool" => "Boolean",
        "i8" => "Byte",
        "i16" => "Short",
        "i32" => "Int",
        "i64" => "Long",
        "f32" => "Float",
        "f63" => "Double",
        "String" => "String",
        "Vec < bool >" => "BooleanArray",
        "Vec < i8 >" => "ByteArray",
        "Vec < u8 >" => "ByteArray",
        "Vec < i16 >" => "ShortArray",
        "Vec < i32 >" => "IntArray",
        "Vec < i64 >" => "LongArray",
        "Vec < f32 >" => "FloutArray",
        "Vec < f64 >" => "DoubleArray",
        "Vec < String >" => "Array<String>",
        _ => return Err(syn::Error::new(Span::call_site(), "Shouldn't be here")),
    };
    Ok(ret)
}

fn kotlin_class_fn(
    class_name: &str,
    j_fn_name: &str,
    j_args_with_types: &str,
    j_args: &str,
    output: &str,
    output_class: &str,
    cleanup: bool,
) -> String {
    format!(
        r#"
    fun {j_fn_name}({j_args_with_types}): {output_class} {{
        val obj = Companion.{j_fn_name}Extern({j_args})
        {cleanup}
        return obj
    }}
        "#,
        cleanup = if cleanup {
            kotlin_cleanup(&format!("{output_class}.Companion"), output)
        } else {
            String::new()
        }
    )
}

fn kotlin_private_native_fn(j_fn_name: &str, j_args_with_types: &str, output: &str) -> String {
    format!(
        r#"
        @JvmStatic
        private external fun {j_fn_name}Extern({j_args_with_types}): {output}
        "#
    )
}

fn kotlin_cleanup(class_path: &str, class_name: &str) -> String {
    format!(
        r#"
        CLEANER.register(obj, {class_path}.{class_name}Cleaner(obj));
    "#
    )
}

fn kotlin_static_fn(
    class_name: &str,
    j_fn_name: &str,
    j_args_with_types: &str,
    j_args: &str,
    output_class: &str,
    output: &str,
    cleanup: bool,
) -> String {
    format!(
        r#"
        @JvmStatic
        fun {j_fn_name}({j_args_with_types}): {output_class} {{
            val obj = {j_fn_name}Extern({j_args})
            {cleanup}
            return obj
        }}
        "#,
        cleanup = if cleanup {
            kotlin_cleanup("Companion", class_name)
        } else {
            String::new()
        }
    )
}

pub struct KotlinClass {
    pub path: String,
    pub name: String,
    pub code: String,
}

fn kotlin_class(
    project_root: &str,
    path: &str,
    kotlin_static_fns: &str,
    kotlin_fns: &str,
    class_name: &str,
    rust_lib: &str,
) -> KotlinClass {
    KotlinClass {
        path: path.into(),
        name: class_name.into(),
        code: format!(
            r#"
package {path}

import {project_root}.Library.CLEANER
import cz.adamh.utils.NativeUtils

class {class_name} {{
    private var handle: Long = -1
    companion object {{
        val _libImport = NativeUtils.loadLibraryFromJar("/lib{rust_lib}.dylib")

        class {class_name}Cleaner(val obj: {class_name}): Runnable {{

            override fun run() {{
                dropByHandleExtern(obj.handle)
            }}
        }}
        {kotlin_static_fns}

        @JvmStatic
        private external fun dropByHandleExtern(handle: Long)
    }}
    {kotlin_fns}
}}

    "#
        ),
    }
}

use syn::visit::{self, Visit};
struct ImplVisitor {
    impls: Vec<ItemImpl>,
}
impl<'ast> Visit<'ast> for ImplVisitor {
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.impls.push(node.clone());
        visit::visit_item_impl(self, node);
    }
}

pub fn rust_file_to_tokens(
    project_root: &str,
    rust_file_str: &str,
    lookup: &PackageLookup,
    rust_lib: &str,
) -> Result<Vec<KotlinClass>, Error> {
    let syntax_tree: File = syn::parse_str(rust_file_str)?;
    let mut visitor = ImplVisitor { impls: Vec::new() };
    visitor.visit_file(&syntax_tree);
    visitor
        .impls
        .iter()
        .flat_map(|impl_item| {
            impl_item
                .attrs
                .iter()
                .filter_map(|attr| match &attr.meta {
                    Meta::List(MetaList { path, tokens, .. })
                        if path
                            .get_ident()
                            .map(|ident| ident.to_string().as_str() == "java_class")
                            .unwrap_or(false) =>
                    {
                        Some(
                            syn::parse2::<LitStr>(tokens.clone())
                                .map(|res| res.value().to_string()),
                        )
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|namespace| -> Result<_, _> {
                    let namespace = namespace?;
                    let struct_n = impl_item.self_ty.to_token_stream();
                    let path = format!("{namespace}.{struct_n}");

                    let companion_fns = impl_item
                        .items
                        .iter()
                        .filter_map(|impl_item| match impl_item {
                            ImplItem::Fn(fn_item) => Some(fn_item),
                            _ => None,
                        })
                        .filter_map(|fn_item| {
                            match kotlin_companion_method(&namespace, &struct_n, fn_item, lookup) {
                                Ok(Some(x)) => Some(Ok(x)),
                                Err(err) => Some(Err(err)),
                                Ok(None) => None,
                            }
                        })
                        .try_fold::<_, _, Result<String, syn::Error>>(
                            String::new(),
                            |mut accum, line| {
                                accum.push('\n');
                                accum.push_str(&(line?));
                                Ok(accum)
                            },
                        )?;
                    let class_fns = impl_item
                        .items
                        .iter()
                        .filter_map(|impl_item| match impl_item {
                            ImplItem::Fn(fn_item) => Some(fn_item),
                            _ => None,
                        })
                        .filter_map(|fn_item| {
                            match kotlin_class_method(&namespace, &struct_n, fn_item, lookup) {
                                Ok(Some(x)) => Some(Ok(x)),
                                Err(err) => Some(Err(err)),
                                Ok(None) => None,
                            }
                        })
                        .try_fold::<_, _, Result<String, syn::Error>>(
                            String::new(),
                            |mut accum, line| {
                                accum.push('\n');
                                accum.push_str(&(line?));
                                Ok(accum)
                            },
                        )?;
                    Ok(kotlin_class(
                        project_root,
                        &namespace,
                        &companion_fns,
                        &class_fns,
                        struct_n.to_string().as_str(),
                        rust_lib,
                    ))
                })
        })
        .collect::<Result<Vec<_>, _>>()
}

#[cfg(test)]
mod test {
    use crate::token_processing::rust_file_to_tokens;

    use super::{java_class_fn, KotlinClass, PackageLookup};

    use quote::quote;

    #[test]
    fn test_java_method() {
        let attr = quote! {"boop.bop"};
        let stream: proc_macro2::TokenStream = quote! {

        impl SomeStruct {
            fn some_stuff(string: String, idx: i32) -> SomeStruct {
                SomeStruct
            }
            fn some_more_stuff(&self, string: String) -> i32 {
                todo!()
            }

            fn some_more_stuff_mut(&mut self, string: String) -> i32 {
                todo!()
            }
        }
                };

        let tokens = java_class_fn(attr, stream).expect("Okay");
        println!("{tokens}");
        println!(
            "{}",
            prettyplease::unparse(
                &syn::parse_file(&tokens.to_string()).expect("Couldn't parse file")
            )
        );
    }

    #[test]
    fn test_kotlin_class() {
        let stream: proc_macro2::TokenStream = quote! {


        #[java_class("beep.bop")]
        impl SomeStruct {
            fn some_stuff(string: String, idx: i32) -> SomeStruct {
                SomeStruct
            }
            fn some_more_stuff(&self, string: String) -> i32 {
                todo!()
            }

            fn some_more_stuff_mut(&mut self, string: String) -> i32 {
                todo!()
            }
        }
                };

        let token_str = stream.to_string();
        let lookup: PackageLookup = [("SomeStruct".into(), "beep.bop.SomeStruct".into())]
            .into_iter()
            .collect();

        let tokens = rust_file_to_tokens("beep.boop", &token_str, &lookup).expect("Not OK");
        for KotlinClass { code: token, .. } in tokens {
            println!("Tokens: {token}");
        }
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;
    #[test]
    fn test_kotlin_fn() {
        let fun = quote! {
            fn
        };
    }
}
