// Chapter 1, lesson 4. A forgotten Result. This repo denies the lint, so this
// file does too.
#![deny(unused_must_use)]

fn save() -> Result<(), String> {
    Err("disk full".to_string())
}

fn main() {
    save();
}
