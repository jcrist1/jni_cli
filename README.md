# jni-cli
a cli to build a java library around a rust dylib

Goal
```rust
#[java_class('dev.gigapixel.tok4j', thread_safe = true)]
pub struct Structure {
    inner: Type
}

#[java_class('dev.gigapixel.tok4j')]
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

Want this to expand to 

```rust
impl JavaClass for Structure {
    const LOC: &'static str = "dev.gigapixe.tok4j.Structure";
    const PATH &'static str = "dev/gigapixe/tok4j/Structure";
    fn 
}

#[jni_fn("dev.gigapixel.tok4j.Structure")]
pub fn newStructure<'loca>(JNIEnv, Class) -> JObject {
  ... jni_stuff to create object of 
}

```

```kotlin
package dev.gigapixel.tok4j

import dev.gigapixel.tok4j.Structure2

class Structure {
    private var handle: Long = -1
    private class StructureCleaner(val handle: Long): Runnable {
        override fun run() {
            Model.dropByHandle(handle)
        }
    }
    companion object {
        
        // no self parameter, returns new instance so needs to provide cleanup
        fun newStructure(): Structure {
            val structure = newStructure()
            CLEANER.register(model, StructureCleaner(model.handle));
            return structure 
        }

        @JvmStatic
        private external fun new(): Structure

        @JvmStatic
        private external fun modify(handle: Long, struct2: Structure2)

        @JvmStatic
        private external fun stringMagic(handle: Long, string: String): Array<String>

        @JvmStatic
        private external fun dropByHandle(handle: Long)
    }

    fun modify(struct2: Structure2) {_modify(handle, struct2)}

    fun stringMagic(string: String): Array<String> {_stringMagic(handle, string)}
}

inner

```
