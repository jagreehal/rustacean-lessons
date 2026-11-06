// Every other task needing this lock waits out the whole `.await`.
use std::sync::Mutex;

async fn slow_call() {}

pub async fn update(counter: &Mutex<u32>) {
    let Ok(guard) = counter.lock() else { return };
    slow_call().await;
    drop(guard);
}
