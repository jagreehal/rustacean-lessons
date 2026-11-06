// Looks like a call. The future is dropped, so the save never happens.
async fn save() {}

pub fn forget() {
    let _ = save();
}
