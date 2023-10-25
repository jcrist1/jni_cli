







use jni_cli_core::token_processing::java_class_fn;

#[proc_macro_attribute]
pub fn java_class(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    java_class_fn(attr.into(), item.into())
        .expect("Oops")
        .into()
}
