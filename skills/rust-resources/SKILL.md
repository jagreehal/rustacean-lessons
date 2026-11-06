---
name: rust-resources
description: Manages Rust connections, files, locks and sessions with a composition root, Arc sharing, Drop cleanup and close(self) methods. Use when wiring up a Rust app in main, sharing a pool or client, writing a Drop impl or guard, or designing open and close lifecycles, including async cleanup.
---

# Rust resources

Resources need opening, sharing and closing. Put each stage where the code shows it.

## Rules

- Build the app in one function called from `main`, the composition root. It reads top to bottom in the order it runs, and failures use `?`.
- Call each constructor once and share the result with `Arc`. One `connect` call is one connection.
- Put cleanup in `Drop`, so it runs on return, on `?` and on panic.
- Mind drop order. Local variables drop in reverse order. Struct fields drop in declaration order, so list a session above the pool it came from.
- Take `self` by value in `close`, so using a closed value fails with E0382.

## The shape

```rust
use std::sync::Arc;

pub struct Connection;
pub struct ConnectError;
pub struct Repo {
    pub connection: Arc<Connection>,
}
pub struct App {
    pub users: Repo,
    pub orders: Repo,
}

async fn connect(_url: &str) -> Result<Connection, ConnectError> {
    Ok(Connection)
}

pub async fn build_app(url: &str) -> Result<App, ConnectError> {
    let connection = Arc::new(connect(url).await?);
    Ok(App {
        users: Repo { connection: connection.clone() },
        orders: Repo { connection },
    })
}
```

## Async cleanup

`drop` is an ordinary function, so it can't `.await`. Pick by what the cleanup needs:

| Cleanup | Use |
|---|---|
| Synchronous | `Drop` |
| Async, and skipping it on cancellation is harmless | An async `close(self)`, called by a helper that wraps the work |
| Async, and it should survive cancellation | A guard whose `Drop` spawns the cleanup as a task |
| Async, and it must happen | That guard, plus an expiry or durable recovery |

```rust
use std::sync::Arc;

use async_trait::async_trait;
use tokio::runtime::Handle;

#[derive(Clone, Copy)]
pub struct LockToken(pub u64); // proof of who holds the lock

#[async_trait]
pub trait Locks: Send + Sync {
    // Frees the lock only if `token` still holds it, as one atomic step.
    async fn release(&self, key: &str, token: LockToken);
}

pub struct LockGuard {
    pub locks: Arc<dyn Locks>,
    pub key: String,
    pub token: LockToken,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let locks = self.locks.clone();
        let key = std::mem::take(&mut self.key);
        let token = self.token;
        let Ok(runtime) = Handle::try_current() else {
            return; // no runtime left, so the expiry frees it
        };
        runtime.spawn(async move { locks.release(&key, token).await });
    }
}
```

An expiry lets a lock pass to someone else while the old guard still exists, so release by token, never by key alone.

## GAP

- A spawned cleanup survives cancellation of its caller and nothing more. A runtime shutdown drops the task, and a process crash skips `Drop`, so it's best effort.
- Cancelling a future drops it at its current `.await`, so a `close().await` below that line never runs.
- `close(self)` can't apply inside an `Arc`, because you share the value. `Arc::try_unwrap` returns it once a single owner remains.

Optional background, with the tests behind these rules: https://github.com/jagreehal/rustacean-lessons/blob/main/guide/03-resources.md
