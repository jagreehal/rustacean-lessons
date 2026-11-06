// Fine for a bug, wrong for anything a caller could cause.
pub fn parse(raw: &str) -> u32 {
    match raw.parse() {
        Ok(n) => n,
        Err(_) => panic!("bad input"),
    }
}
