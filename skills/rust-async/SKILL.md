---
name: rust-async
description: Writes cancellation-safe async Rust with tokio, covering timeouts, retries with backoff, join and try_join, JoinSet task ownership, locks across await, and stable retry identity. Use when writing or reviewing async Rust, adding a timeout or retry, spawning tasks, or debugging work that stops halfway.
---

# Rust async

## The rule: every `.await` is an exit

A timeout, `try_join!` or `select!` cancels a future by dropping it. The future stops at its current `.await` and the code below never runs, with no error at that line. Read each `.await` the way you read a `?`: the function might end here.

When you review async code, ask at each `.await`: if the function ended here, is the state still correct?

```rust
async fn transfer(accounts: &Accounts, amount: u32) {
    accounts.from.fetch_sub(amount, Ordering::SeqCst);
    sleep(Duration::from_secs(1)).await; // cancelled here, the money vanishes
    accounts.to.fetch_add(amount, Ordering::SeqCst);
}
```

Two fixes:

1. Make the steps atomic with a database transaction. A cancelled transaction rolls back.
2. Run the critical part in `tokio::spawn`. A dropped handle doesn't stop the task, and a runtime shutdown does, so work that must survive a restart needs fix 1 or a durable record.

## Rules

- `.await` every future. A future does nothing until then.
- Treat `tokio::time::timeout` as returning a `Result`, and convert it into your error enum.
- Put "should I retry?" on the error type as an exhaustive `match`, so a new variant must decide.
- Double the delay after each attempt, and stop at a maximum attempt count.
- Pick a retry's identity once, before the first attempt. A timeout means you don't know the outcome, and a new reference per attempt can charge twice.
- Use `JoinSet` so tasks stop when their owner goes out of scope. Use `tokio::spawn` only for work that must outlive the caller.
- Release a `std::sync::Mutex` guard before `.await`.
- Use `join!` to wait for all, and `try_join!` to stop at the first failure and drop the rest.
- Test with `#[tokio::test(start_paused = true)]`. The fake clock skips sleeps, so tests stay fast and exact.

## Retry

```rust
#[derive(Debug, PartialEq, Clone, Copy)]
enum CallError {
    Unavailable,
    Rejected,
}

impl CallError {
    fn is_retryable(self) -> bool {
        match self {
            CallError::Unavailable => true,
            CallError::Rejected => false,
        }
    }
}

struct RetryPolicy {
    max_attempts: u32,
    first_delay: Duration,
}

async fn retry<T>(
    policy: &RetryPolicy,
    mut call: impl AsyncFnMut() -> Result<T, CallError>,
) -> Result<T, CallError> {
    let mut delay = policy.first_delay;
    let mut attempt = 1;
    loop {
        match call().await {
            Err(error) if error.is_retryable() && attempt < policy.max_attempts => {
                sleep(delay).await;
                delay *= 2;
                attempt += 1;
            }
            outcome => return outcome,
        }
    }
}
```

This has no jitter, so clients retry in step. Add it for real traffic.

## GAP

- The compiler has no opinion on cancellation. This is the biggest gap in async Rust, and only reading each `.await` finds it.
- A lock held across `.await` is a compile error inside a spawned task only. Elsewhere, deny Clippy's `await_holding_lock` lint.

Optional background, with the tests behind these rules: https://github.com/jagreehal/rustacean-lessons/blob/main/guide/05-async.md
