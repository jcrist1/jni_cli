use jni_cli_macro::java_class;

pub(crate) struct SomeStruct;

#[java_class("beep.boop")]
impl SomeStruct {
    fn do_stuff(s: String, idx: i32) -> SomeStruct {
        SomeStruct
    }

    fn do_more_stuff(&self, string: String) -> i64 {
        string.len() as i64
    }
}
