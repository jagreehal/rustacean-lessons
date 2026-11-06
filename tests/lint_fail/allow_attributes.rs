// A silent opt-out. `#[expect(lint, reason = "...")]` must say why, and it fails
// once the lint stops firing, so the exception can't go stale.
#[allow(dead_code)]
pub fn unused() {}
