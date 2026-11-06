// `_` also matches every variant added later, so the compiler stops helping.
pub enum PayError {
    Declined,
    Timeout,
    Frozen,
}

pub fn retryable(error: PayError) -> bool {
    match error {
        PayError::Timeout => true,
        _ => false,
    }
}
