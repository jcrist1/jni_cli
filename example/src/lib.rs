use jni_cli_macro::java_class;
use tokenizers as tkz;

struct Tokenizer(tkz::Tokenizer);

#[java_class("dev.gigapixel.tokenizers")]
impl Tokenizer {
    fn new_from_bytes(bytes: Vec<u8>) -> Tokenizer {
        let inner = tkz::Tokenizer::from_bytes(bytes).expect("boop");
        Tokenizer(inner)
    }

    fn tokenize(&self, text: String) -> Vec<String> {
        self.0
            .encode(text, false)
            .expect("failed to tokenize")
            .get_tokens()
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
