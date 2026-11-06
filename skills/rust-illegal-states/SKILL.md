---
name: rust-illegal-states
description: Makes bad Rust values impossible to build with newtypes, TryFrom, private fields, typestate and consuming self methods. Use when validating input in Rust, wrapping primitives such as amounts, emails or ids, modelling a status field or multi-stage process, or stopping an operation from running twice.
---

# Rust illegal states

Design types so a bad value can't exist, then let the compiler check every caller.

## Rules

- Parse once at the edge. Wrap the checked value in a newtype with a private field and implement `TryFrom`. Code outside the module gets E0603 if it tries to build one.
- Take the newtype in function signatures and skip re-checking it. The type proves validation ran.
- Give each stage its own type in place of a `status` field. A method that only exists on `Validated` can't run on a `Draft` (E0599).
- Take `self` by value for use-once steps. A second call fails with E0382.
- Say how state is shared across threads: `AtomicU32` or `Mutex`. Two mutable borrows fail with E0499.

## The shape

```rust
mod money {
    #[derive(Debug, PartialEq)]
    pub struct NotPositive;

    #[derive(Debug, Clone, Copy)]
    pub struct Amount(u32); // no `pub` on the field

    impl TryFrom<u32> for Amount {
        type Error = NotPositive;

        fn try_from(minor_units: u32) -> Result<Self, NotPositive> {
            if minor_units == 0 {
                return Err(NotPositive);
            }
            Ok(Amount(minor_units))
        }
    }
}

use money::{Amount, NotPositive};

pub struct Draft {
    pub amount_minor: u32,
}
pub struct Validated {
    amount: Amount,
}
pub struct Charged {
    pub amount: Amount,
}

impl Draft {
    pub fn validate(self) -> Result<Validated, NotPositive> {
        Ok(Validated { amount: Amount::try_from(self.amount_minor)? })
    }
}

impl Validated {
    // `self` has no `&`, so this consumes the Validated.
    pub fn charge(self) -> Charged {
        Charged { amount: self.amount }
    }
}
```

## Proving a claim

When a design depends on something failing to compile, keep the proof. Add a test with `trybuild` that holds the program and its expected compiler error, so it can't pass by failing for another reason.

## GAP

- Use-once stops double use of one value in one process. Two HTTP requests create two values, so you still need idempotency keys.
- Race conditions still compile. Rust stops data races, and locking around a read and again around a write leaves a gap between them. Do the read and the write under one lock.

Optional background, with the tests behind these rules: https://github.com/jagreehal/rustacean-lessons/blob/main/guide/04-illegal-states.md
