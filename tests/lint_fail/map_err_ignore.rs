// Throws away why the parse failed. Keep the source error, or convert with `From`.
pub fn port(raw: &str) -> Result<u16, String> {
    raw.parse().map_err(|_| "bad port".to_string())
}
