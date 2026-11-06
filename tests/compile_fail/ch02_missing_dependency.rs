// Chapter 2, lesson 5. Without a Db there is no Greeter to run.
use std::sync::Arc;

trait Db {}

struct Greeter {
    db: Arc<dyn Db>,
}

fn main() {
    let _greeter = Greeter {};
}
