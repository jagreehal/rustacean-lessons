// The quickest way to turn an expected failure into a crash.
pub fn port(raw: &str) -> u16 {
    raw.parse().unwrap()
}
