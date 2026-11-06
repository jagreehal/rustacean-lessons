// All three throw away a Result, and every rail lets them through.
pub fn save() -> Result<(), String> {
    Err("disk full".to_string())
}

pub fn assignment() {
    _ = save();
}

pub fn converted_to_option() {
    save().ok();
}

pub fn dropped() {
    drop(save());
}
