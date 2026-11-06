// Chapter 3, lesson 6. `close(self)` consumes the session.
struct Session;

impl Session {
    fn query(&self) -> &'static str {
        "rows"
    }

    fn close(self) {}
}

fn main() {
    let session = Session;
    session.close();
    println!("{}", session.query());
}
