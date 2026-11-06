// ═════════════════════════════════════════════════════════════
// Chapter 4: illegal states. Mistakes that can't compile.
//
// New to this? Read guide/04-illegal-states.md first. It explains the ideas and why they exist.
//
// Run:  cargo test ch04
//
// Most of what this chapter proves is that a program does NOT compile, so the
// evidence is in tests/compile_fail/ch04_*.rs and their .stderr files.
//
// The idea behind all of it is ownership: a value has one owner, and handing it
// on means you no longer have it. So a type can say more than what a value is. It
// can say you've used it up.
// ═════════════════════════════════════════════════════════════

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Barrier, Mutex, PoisonError};
use std::thread;

// ─────────────────────────────────────────────────────────────
// Lesson 1: a value that can only exist if it's valid.
//
// The field is private, so outside this module `try_from` is the only way to get
// an Amount. Every function taking an Amount can skip validation.
//
// There's no cast that gets around it, as ch04_forged_amount.rs shows, and
// `unsafe_code = "forbid"` closes the remaining door. A compile-time label alone
// can't promise that: in TypeScript, `0 as Amount` forges a branded type.
// ─────────────────────────────────────────────────────────────

// `mod` makes a module. Without `pub`, a field stays private to its module.
mod money {
    #[derive(Debug, PartialEq, Clone, Copy)]
    pub struct Amount(u32);

    #[derive(Debug, PartialEq)]
    pub struct NotPositive;

    impl TryFrom<u32> for Amount {
        type Error = NotPositive;

        fn try_from(minor_units: u32) -> Result<Self, NotPositive> {
            if minor_units == 0 {
                return Err(NotPositive);
            }
            Ok(Amount(minor_units))
        }
    }

    impl Amount {
        pub fn minor_units(self) -> u32 {
            self.0
        }
    }
}

use money::{Amount, NotPositive};

#[test]
fn lesson_1_parsing_is_the_only_way_in() {
    assert_eq!(Amount::try_from(0), Err(NotPositive));
    assert_eq!(Amount::try_from(499).map(Amount::minor_units), Ok(499));
}

#[test]
fn lesson_1_an_amount_cannot_be_forged() {
    crate::assert_rejected("ch04_forged_amount", "E0603");
}

// ─────────────────────────────────────────────────────────────
// Lesson 2: typestate. Each stage of a payment is its own type.
//
// Draft has no `charge` method, so charging an unvalidated payment is a compile
// error (ch04_charge_unvalidated.rs). Any language with a type per stage can do
// this much.
//
// Lesson 3 is where ownership goes further.
// ─────────────────────────────────────────────────────────────

struct Draft {
    minor_units: u32,
}

struct Validated {
    amount: Amount,
}

#[derive(Debug, PartialEq)]
struct Charged {
    receipt: String,
}

impl Draft {
    fn validate(self) -> Result<Validated, NotPositive> {
        let amount = Amount::try_from(self.minor_units)?;
        Ok(Validated { amount })
    }
}

impl Validated {
    // `self` with no `&` takes ownership: the caller gives the value up.
    fn charge(self) -> Charged {
        let receipt = format!("charged {}", self.amount.minor_units());
        Charged { receipt }
    }
}

#[test]
fn lesson_2_the_stages_run_in_order() -> Result<(), NotPositive> {
    let charged = Draft { minor_units: 499 }.validate()?.charge();
    assert_eq!(charged.receipt, "charged 499");
    Ok(())
}

#[test]
fn lesson_2_charging_an_unvalidated_payment_does_not_compile() {
    crate::assert_rejected("ch04_charge_unvalidated", "E0599");
}

// ─────────────────────────────────────────────────────────────
// Lesson 3: charging twice. OWNERSHIP GIVES YOU USE-ONCE.
//
// ┌─ PREDICT ─────────────────────────────────────────────────┐
// │ `validated.charge()` is called twice. What happens?       │
// └───────────────────────────────────────────────────────────┘
//
// Answer: a type error. `charge(self)` consumes the Validated, so the second call
// is a use-after-move. See ch04_double_charge.rs.
//
// Without ownership, a type can say "this is validated" but not "this can be used
// once". So this version type-checks, and the customer pays twice:
//
//     charge(validated);
//     charge(validated);
//
// The limit: this stops double use of one value in one process. Two requests
// for the same payment are two values, so you still need idempotency keys.
// ─────────────────────────────────────────────────────────────

#[test]
fn lesson_3_charging_twice_does_not_compile() {
    crate::assert_rejected("ch04_double_charge", "E0382");
}

// ─────────────────────────────────────────────────────────────
// Lesson 4: data races.
//
// Two threads changing the same value without coordination don't compile. See
// ch04_data_race.rs. `Send` and `Sync` carry the rule, and they work on any type.
//
// So you get threads running at once on separate cores, with no data races, as
// below. Single-threaded runtimes avoid data races by giving up that parallelism.
// ─────────────────────────────────────────────────────────────

#[test]
fn lesson_4_shared_state_across_threads_must_say_how_it_is_shared() {
    let count = AtomicU32::new(0);

    thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                for _ in 0..1_000 {
                    count.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });

    assert_eq!(count.load(Ordering::Relaxed), 8_000);
}

#[test]
fn lesson_4_a_data_race_does_not_compile() {
    crate::assert_rejected("ch04_data_race", "E0499");
}

// ─────────────────────────────────────────────────────────────
// Lesson 5: what the compiler does NOT catch.
//
// "No data races" doesn't mean "no race conditions". Each lock below is correct
// on its own, but reading and writing under two separate locks loses an update.
// This compiles with no warning. The barrier forces the bad ordering, so the
// test is deterministic.
//
// Any language has this trap, including single-threaded ones with `await` between
// the read and the write. The fix: do the read and the write under one lock.
// ─────────────────────────────────────────────────────────────

fn read(balance: &Mutex<u32>) -> u32 {
    *balance.lock().unwrap_or_else(PoisonError::into_inner)
}

fn write(balance: &Mutex<u32>, value: u32) {
    *balance.lock().unwrap_or_else(PoisonError::into_inner) = value;
}

#[test]
fn lesson_5_a_race_condition_still_compiles() {
    let balance = Mutex::new(100);
    let both_have_read = Barrier::new(2);

    thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                let seen = read(&balance);
                both_have_read.wait();
                write(&balance, seen + 10);
            });
        }
    });

    // Two deposits of 10 should make 120. One was lost.
    assert_eq!(read(&balance), 110);
}

// ─────────────────────────────────────────────────────────────
// Exercise: `cargo test ch04 -- --ignored`
//
//   1. Add a `Refunded` stage that can only come from Charged.
//   2. Add a compile_fail test proving you can't refund twice, and generate its
//      .stderr with `TRYBUILD=overwrite cargo test --test compile_fail`.
//   3. Fix lesson 5 so the balance reaches 120, then update its assertion.
// ─────────────────────────────────────────────────────────────

#[test]
#[ignore = "exercise: done when a second refund fails to compile and lesson 5 reaches 120"]
fn exercise_add_a_refunded_stage() {
    todo!("replace this with your test")
}
