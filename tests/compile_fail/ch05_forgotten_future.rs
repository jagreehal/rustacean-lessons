// Chapter 5, lesson 1. No `.await`, so this never runs. This repo denies the lint.
#![deny(unused_must_use)]

async fn save() {}

fn main() {
    save();
}
