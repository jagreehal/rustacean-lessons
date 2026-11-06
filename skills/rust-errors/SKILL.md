---
name: rust-errors
description: Models Rust failures as enums returned in a Result, composed with the ? operator, and mapped to responses in one exhaustive match. Use when writing or reviewing Rust error types, Result signatures, From conversions, thiserror enums, HTTP status mapping, or code that calls unwrap, expect or panic.
---

# Rust errors

A signature answers three questions: what do I get back, how can this fail, what does it need? This skill covers the first two.

## Rules

- Return expected failures as `Err`, with an enum naming each kind.
- Keep `panic!` for bugs: broken promises no caller can recover from.
- Write no `.unwrap()` or `.expect()`. They turn an expected failure into a crash.
- Compose steps with `?`, so the happy path reads top to bottom.
- List every variant in each `match`. `_ =>` on an enum swallows variants added later.
- Keep the cause. `.map_err(|_| ...)` throws away why it failed.

## The shape

`#[from]` generates the conversion `?` needs, so each step's error joins the union:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid amount")]
pub struct AmountError(#[from] std::num::ParseIntError);

#[derive(Debug, Error)]
#[error("card declined")]
pub struct Declined;

#[derive(Debug, Error)]
pub enum PayError {
    #[error(transparent)]
    Amount(#[from] AmountError),
    #[error(transparent)]
    Declined(#[from] Declined),
}

fn parse_amount(raw: &str) -> Result<u32, AmountError> {
    Ok(raw.parse()?)
}

fn charge(amount: u32) -> Result<String, Declined> {
    if amount > 1_000 {
        return Err(Declined);
    }
    Ok(format!("receipt for {amount}"))
}

pub fn pay(raw: &str) -> Result<String, PayError> {
    let amount = parse_amount(raw)?;
    let receipt = charge(amount)?;
    Ok(receipt)
}
```

## Translate once, at the boundary

Inside the program a failure is a typed value. One exhaustive `match` turns it into what the caller sees:

```rust
pub enum SignupError {
    Invalid(String),
    Conflict(String),
    Unavailable,
    Storage(String),
}

pub fn to_response(error: &SignupError) -> (u16, String) {
    match error {
        SignupError::Invalid(reason) => (400, reason.clone()),
        SignupError::Conflict(reason) => (409, reason.clone()),
        SignupError::Unavailable => (503, "try again soon".to_string()),
        SignupError::Storage(_detail) => (500, "something went wrong".to_string()),
    }
}
```

A new variant can't compile until you choose its response, and `Storage` keeps database detail away from customers.

## Adding a variant

Add it, run `cargo check`, and work down the list of `match` expressions the compiler reports. Choose a response at each one.

## GAP

- You write the error union yourself. The compiler checks it and won't infer it.
- Handling one variant can't shrink the type. To say "only two remain", write a second enum.

Optional background, with the tests behind these rules: https://github.com/jagreehal/rustacean-lessons/blob/main/guide/01-errors.md
