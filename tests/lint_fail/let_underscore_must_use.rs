// A failure thrown away on purpose, with nothing to show why.
pub fn save() -> Result<(), String> {
    Err("disk full".to_string())
}

pub fn forget() {
    let _ = save();
}
