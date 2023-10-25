use std::{iter::repeat, sync::RwLock};

use jni::{
    descriptors::Desc,
    objects::{JByteArray, JClass, JObject, JObjectArray, JString, JValue, ReleaseMode},
    sys::{jboolean, jdouble, jfloat, jint, jlong},
    JNIEnv,
};
pub use jni_fn::jni_fn;

pub(crate) type Result<T> = std::result::Result<T, Error>;
pub mod token_processing;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Jni Error {0}")]
    Jni(#[from] jni::errors::Error),
    #[error("String error {0}")]
    Str(#[from] std::str::Utf8Error),
    #[error("{context_message}, caused by {err}")]
    Contextual {
        context_message: String,
        err: Box<Error>,
    },
}

impl Error {
    fn context(self, message: String) -> Error {
        match self {
            Error::Contextual {
                context_message,
                err,
            } => {
                let context_message = format!("{message}\n{context_message}");
                Error::Contextual {
                    context_message,
                    err,
                }
            }
            err => Error::Contextual {
                context_message: message,
                err: Box::new(err),
            },
        }
    }
}

pub(crate) trait Context<T> {
    fn context(self, message: String) -> Result<T>;
}

impl<T, E> Context<T> for std::result::Result<T, E>
where
    E: Into<Error>,
{
    fn context(self, message: String) -> Result<T> {
        self.map_err(Into::into).map_err(|err| err.context(message))
    }
}

/// # Safety
/// a type that implements JavaClass, must be dropped by the java garbage collector
/// with a cleaner
pub unsafe trait JavaClass
where
    Self: Sized,
{
    const LOC: &'static str;
    const PATH: &'static str;

    unsafe fn new_from_rust_type<'local>(
        self,
        env: &mut JNIEnv<'local>,
    ) -> Result<JObject<'local>> {
        let class = format!("L{};", Self::PATH);
        let mut return_obj = env
            //.new_object("Ldev/gigapixel/tok4j/Model;", "()V", &[])
            .new_object(&class, "()V", &[])
            .context(format!("failed to instantiate class {class}"))?;

        let boxed = Box::new(RwLock::new(self));

        let handle = Box::into_raw(boxed) as jlong;

        env.set_field(&mut return_obj, "handle", "J", JValue::Long(handle))
            .context(format!(
                "Failed to set handle pointer for java object: {}",
                Self::PATH
            ))?;
        Ok(return_obj)
    }

    unsafe fn rust_type_from_handle(handle: jlong) -> Box<RwLock<Self>> {
        unsafe { Box::from_raw(handle as *mut RwLock<Self>) }
    }

    fn use_shared<T, F: FnOnce(&Self) -> T>(handle: jlong, f: F) -> T {
        let rust_type = unsafe { Self::rust_type_from_handle(handle) };
        let t = f(&rust_type.read().expect("Failed to readRwLock"));
        // garbage collector has to clean up
        std::mem::forget(rust_type);
        t
    }

    fn use_mut<T, F: FnOnce(&mut Self) -> T>(handel: jlong, f: F) -> T {
        let rust_type = unsafe { Self::rust_type_from_handle(handel) };
        let t = f(&mut *rust_type.write().expect("Failed to lock RwLock"));
        // garbage collector has to clean up
        std::mem::forget(rust_type);
        t
    }
    unsafe fn drop_by_handle(handle: jlong) {
        unsafe {
            let _ = Self::rust_type_from_handle(handle);
        }
    }
}

pub trait JType {
    type JType<'a>;
    fn to_j_type<'env>(self, env: &mut JNIEnv<'env>) -> Result<Self::JType<'env>>;
}

pub trait PrimitiveJType: JType + Sized {
    fn from_j_type<'env>(env: &mut JNIEnv<'env>, j_type: Self::JType<'env>) -> Result<Self>;
}

impl<T> JType for T
where
    T: JavaClass,
{
    type JType<'a> = JObject<'a>;

    fn to_j_type<'env>(self, env: &mut JNIEnv<'env>) -> Result<Self::JType<'env>> {
        unsafe { self.new_from_rust_type(env) }
    }
}

impl JType for String {
    type JType<'a> = JString<'a>;

    fn to_j_type<'env>(self, env: &mut JNIEnv<'env>) -> Result<Self::JType<'env>> {
        Ok(env.new_string(self)?)
    }
}

impl PrimitiveJType for String {
    fn from_j_type<'env>(env: &mut JNIEnv<'env>, j_type: Self::JType<'env>) -> Result<Self> {
        Ok(env.get_string(&j_type)?.into())
    }
}

impl JType for i64 {
    type JType<'a> = jlong;

    fn to_j_type<'env>(self, _env: &mut JNIEnv<'env>) -> Result<Self::JType<'env>> {
        Ok(self)
    }
}

impl PrimitiveJType for i64 {
    fn from_j_type<'env>(_env: &mut JNIEnv<'env>, j_type: Self::JType<'env>) -> Result<Self> {
        Ok(j_type)
    }
}

impl JType for bool {
    type JType<'a> = jboolean;

    fn to_j_type<'env>(self, _env: &mut JNIEnv<'env>) -> Result<Self::JType<'env>> {
        Ok(self as u8)
    }
}

impl JType for i32 {
    type JType<'a> = jint;

    fn to_j_type<'env>(self, _env: &mut JNIEnv<'env>) -> Result<Self::JType<'env>> {
        Ok(self)
    }
}

impl PrimitiveJType for i32 {
    fn from_j_type<'env>(_env: &mut JNIEnv<'env>, j_type: Self::JType<'env>) -> Result<Self> {
        Ok(j_type)
    }
}

impl JType for f32 {
    type JType<'a> = jfloat;

    fn to_j_type<'env>(self, _env: &mut JNIEnv<'env>) -> Result<Self::JType<'env>> {
        Ok(self)
    }
}

impl PrimitiveJType for f32 {
    fn from_j_type<'env>(_env: &mut JNIEnv<'env>, j_type: Self::JType<'env>) -> Result<Self> {
        Ok(j_type)
    }
}

impl JType for f64 {
    type JType<'a> = jdouble;

    fn to_j_type<'env>(self, _env: &mut JNIEnv<'env>) -> Result<Self::JType<'env>> {
        Ok(self)
    }
}

impl PrimitiveJType for f64 {
    fn from_j_type<'env>(_env: &mut JNIEnv<'env>, j_type: Self::JType<'env>) -> Result<Self> {
        Ok(j_type)
    }
}

impl JType for Vec<String> {
    type JType<'a> = JObjectArray<'a>;

    fn to_j_type<'env>(self, env: &mut JNIEnv<'env>) -> Result<Self::JType<'env>> {
        let empty_string = env.new_string("")?;
        let string_class = env.get_object_class(&empty_string)?;
        let mut object_array =
            env.new_object_array(self.len() as i32, string_class, empty_string)?;
        for (idx, elem) in self.into_iter().enumerate() {
            let j_elem = elem.to_j_type(env)?;
            env.set_object_array_element(&mut object_array, idx as i32, j_elem)?
        }
        Ok(object_array)
    }
}

impl PrimitiveJType for Vec<String> {
    fn from_j_type<'env>(env: &mut JNIEnv<'env>, j_type: Self::JType<'env>) -> Result<Self> {
        (0..(env.get_array_length(&j_type)?))
            .map(|i| -> Result<String> {
                let j_obj = env.get_object_array_element(&j_type, i)?;
                let j_str: JString<'env> = j_obj.into();
                let j_string = env.get_string(&j_str)?;
                Ok(j_string.to_str()?.to_string())
            })
            .collect()
    }
}

impl JType for Vec<u8> {
    type JType<'a> = JByteArray<'a>;

    fn to_j_type<'env>(self, env: &mut JNIEnv<'env>) -> Result<Self::JType<'env>> {
        let mut object_array = env.new_byte_array(self.len() as i32)?;
        for (i, val) in self.into_iter().enumerate() {
            let arr = [val as i8];
            env.set_byte_array_region(&mut object_array, i as i32, &arr)?;
        }
        Ok(object_array)
    }
}

impl PrimitiveJType for Vec<u8> {
    fn from_j_type<'env>(env: &mut JNIEnv<'env>, j_type: Self::JType<'env>) -> Result<Self> {
        Ok(env.convert_byte_array(j_type)?.to_vec())
    }
}

impl<T> JType for Vec<T>
where
    T: JavaClass + Default,
{
    type JType<'a> = JObjectArray<'a>;

    fn to_j_type<'env>(self, env: &mut JNIEnv<'env>) -> Result<Self::JType<'env>> {
        let default_object = T::default();
        let default_j_object = default_object.to_j_type(env)?;

        let class = Desc::<JClass>::lookup(T::LOC, env)?;
        let mut object_array = env.new_object_array(self.len() as i32, class, default_j_object)?;
        for (idx, elem) in self.into_iter().enumerate() {
            let j_elem = elem.to_j_type(env)?;
            env.set_object_array_element(&mut object_array, idx as i32, j_elem)?
        }
        Ok(object_array)
    }
}
