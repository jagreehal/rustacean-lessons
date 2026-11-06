---
name: rust-services
description: Structures Rust service functions as fn(args, deps) with a small deps struct of Arc<dyn Trait> per function, and tests them with hand-written stubs and spies. Use when writing Rust business logic, adding a database, clock or provider dependency, designing traits, or testing code that touches I/O without a framework or mock library.
---

# Rust services

```rust
async fn signup(args: SignupArgs, deps: &SignupDeps) -> Result<UserId, SignupError>
```

`args` holds the data for this call. `deps` holds the outside world. The signature tells you, a test or an agent what the function returns, how it fails and what it needs.

## Rules

- Give each function its own small deps struct. One shared `AppDeps` makes every signature say "everything".
- Hold services as `Arc<dyn Trait>`. Use traits at I/O edges only: database, provider, clock.
- Keep business logic in plain functions. A trait with one implementation buys nothing.
- Pass hidden inputs through deps too: time, configuration, randomness.
- Write stand-ins by hand. No mock library and no framework that builds the graph for you.

## The shape

```rust
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

#[async_trait]
pub trait Db: Send + Sync {
    async fn get_name(&self, id: u32) -> String;
}

pub struct GreetDeps {
    pub db: Arc<dyn Db>,
}

pub async fn greet(id: u32, deps: &GreetDeps) -> String {
    format!("Hello {}", deps.db.get_name(id).await)
}

// A spy: the test and deps both hold it through an Arc.
#[derive(Default)]
pub struct DbSpy {
    pub calls: Mutex<Vec<u32>>,
}

#[async_trait]
impl Db for DbSpy {
    async fn get_name(&self, id: u32) -> String {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(id);
        }
        "Ada".to_string()
    }
}
```

| Part | Why it's there |
|---|---|
| `dyn Db` | Production and tests supply different types |
| `Arc<...>` | Several owners can hold one service. `.clone()` copies the pointer |
| `Send + Sync` | Async tasks can move between threads |
| `#[async_trait]` | Native `async fn` in traits rejects `dyn` with E0038 |

## Stand-ins

| Stand-in | Behaviour | Use it to test |
|---|---|---|
| Stub | Returns a fixed answer | Logic, given a known input |
| Failing stub | Returns an error | The sad path |
| Spy | Records its calls | How your code used the service |
| Fake | A working in-memory version | Flows that write, then read |

## Adding a dependency

Add a field to the deps struct, then fix each construction site the compiler reports (E0063). You can't start the program with a dependency missing.

## GAP

- Nothing infers dependencies. A new need means a new field and an update at each caller.
- Nothing stops a function reaching for the outside world itself. Ban the common cases, like reading the clock, with Clippy's `disallowed_methods`.

Optional background, with the tests behind these rules: https://github.com/jagreehal/rustacean-lessons/blob/main/guide/02-services.md
