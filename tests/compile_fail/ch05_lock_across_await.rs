// Chapter 5, lesson 7. A std MutexGuard can't move between threads, and a spawned
// task might resume on a different one.
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    let counter = Arc::new(Mutex::new(0));

    tokio::spawn(async move {
        let guard = counter.lock();
        tokio::time::sleep(Duration::from_secs(1)).await;
        drop(guard);
    });
}
