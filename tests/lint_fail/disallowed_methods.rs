// A hidden input. The signature says this needs nothing, and tests can't control it.
pub fn is_expired(created: std::time::Instant) -> bool {
    std::time::Instant::now().duration_since(created).as_secs() > 60
}
