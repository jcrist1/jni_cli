# jni-cli
a cli to build a java library around a rust dylib

The goal of this tool somewhat like [maturin](https://github.com/PyO3/maturin/) to package a rust JNI library
into a convenient library for use in a JVM application. The basic idea is to be able to annotate 
a rust struct's impl block with a procedural macro that will fill in some JNI boilerplate for the 
impl methods, then to take those methods and generate kotlin code that will call them as well 
as calling the descructor for the struct on cleanup

For now the cli only works for the example provided in the `example/` folder. A small todo item is to
parse to location of the generated dylib

Call
```sh
cargo build --release
cd example
../target/release/cli -g dev.gigapixel -p tokenizers
```

### Todos:
* get dylib artifacts programatically
* rewrite
* testing
* ...




## The initial goal
Initially I wanted something like the following
```rust
#[java_class('dev.gigapixel.tokenizers', thread_safe = true)]
pub struct Structure {
    inner: Type
}

#[java_class('dev.gigapixel.tokenizers')]
pub struct Structure2 {
    field: Type
}


#[java_methods]
impl Structure {
    fn new() -> Self {
        todo!()
    }
    
    fn modify(&mut self, struct_2: &Structure2) {
        todo!()
    }

    fn string_magic(&self, string: &str) -> Vec<String>{
        todo!()
    }
} 
```

I wound up just annotating the impl block of the struct. 
```rust
pub struct Structure {
    inner: Type
}


#[java_class("dev.gigapixel.tokenizers")]
impl Structure {
    fn create() -> Self {
        todo!()
    }
    
    fn modify(&mut self, struct_2: &Structure2) {
        todo!()
    }

    fn string_magic(&self, string: &str) -> Vec<String>{
        todo!()
    }
} 
```
by default the class _should_ be thread safe.
But it's very early stages and I have very little experience with unsafe rust. **ABSOLUTELY NO GUARANTEES!!**.
Additionally there is eventually a risk of deadlocking, because we use an RwLock for this. Regardless each 
java class can only be used in a method via the `&self` or `&mut self` param, and so it's not possible to use 
two of these objects in a method... for now.

Additionally I haven't given any thought to async.



What this roughly expands to 

```rust
impl JavaClass for Structure {
    const LOC: &'static str = "dev.gigapixe.tok4j.Structure";
    const PATH &'static str = "dev/gigapixe/tok4j/Structure";
}

#[jni_fn("dev.gigapixel.tok4j.Structure")]
pub fn createExtern<'loca>(JNIEnv, Class) -> JObject {
  // jni_stuff to create object
}
...

```

and then a kotlin file like
```kotlin
package dev.gigapixel.tok4j

import 
class Structure {
    private var handle: Long = -1
    companion object {
        private class StructureCleaner(val handle: long): Runnable {
            override fun run() {
                Model.dropByHandle(handle)
            }
        }
        
        @JvmStatic
        fun create(): Structure {
            val obj = newStructure()
            CLEANER.register(model, StructureCleaner(model.handle));
            return obj  
        }

        @JvmStatic
        private external fun createExtern(): Structure

        @JvmStatic
        private external fun modifyExtern(handle: Long, struct2: Structure2)

        @JvmStatic
        private external fun stringMagicExtern(handle: Long, string: String): Array<String>

        @JvmStatic
        private external fun dropByHandleExtern(handle: Long)
    }

    fun modify(struct2: dev.gigapixel.tok4j.Structure2) {modifyExtern(handle, struct2)}

    fun stringMagic(string: String): Array<String> {stringMagicExtern(handle, string)}
}

```

