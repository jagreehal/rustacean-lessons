// Chapter 1, lesson 4. `Frozen` was added and this match wasn't updated.
enum PayError {
    Declined,
    Timeout,
    Frozen,
}

fn message(error: PayError) -> &'static str {
    match error {
        PayError::Declined => "try another card",
        PayError::Timeout => "try again",
    }
}

fn main() {
    println!("{}", message(PayError::Frozen));
}
