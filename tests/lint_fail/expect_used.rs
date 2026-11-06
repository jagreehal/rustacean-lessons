// A crash with a nicer message. Expected failures belong in the return type.
pub fn port(raw: &str) -> u16 {
    raw.parse().expect("port should be a number")
}
