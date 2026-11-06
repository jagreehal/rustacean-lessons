// Chapter 4, lesson 4. Two threads writing one integer.
use std::thread;

fn main() {
    let mut count = 0;

    thread::scope(|scope| {
        scope.spawn(|| count += 1);
        scope.spawn(|| count += 1);
    });

    println!("{count}");
}
