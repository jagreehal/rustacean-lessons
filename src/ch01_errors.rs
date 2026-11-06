// ═════════════════════════════════════════════════════════════
// Chapter 1: errors. How a function tells you it can fail.
//
// New to this? Read guide/01-errors.md first. It explains the ideas and why they exist.
//
// Read top to bottom. Each lesson adds one idea.
// Run:  cargo test ch01
//
// A good signature answers three questions: what do I get back, how can this
// fail, and what does it need? This chapter covers the first two.
//
//   a success             ->   Ok(x)
//   an expected failure   ->   Err(e), named in the return type
//   stop at first failure ->   step?
//   recover from a failure->   match
//   a bug                 ->   panic!
//   the caller's response ->   one exhaustive match at the boundary
//
// The outcome: you can't forget a failure, because it's in the type you're holding,
// and each kind of failure gets its own response at the boundary (lesson 7).
// Chapter 2 covers the third question.
// ═════════════════════════════════════════════════════════════

use thiserror::Error;

// ─────────────────────────────────────────────────────────────
// Lesson 1: a failure is a value.
//
// The return type names how this can fail, so no caller has to guess.
//
// `enum` lists the possibilities, and a value is one of them and no other.
// `#[derive(...)]` asks the compiler to write code for you: Debug lets you print it,
// PartialEq gives you ==, and Error (from the thiserror crate) marks it as an error
// and uses each `#[error("...")]` line as its message.
// `&str` is borrowed text. `String` is text you own.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Error)]
enum AmountError {
    #[error("not a number: {0}")]
    NotANumber(String),
    #[error("amount must be positive")]
    NotPositive,
}

fn parse_amount(raw: &str) -> Result<u32, AmountError> {
    // `let ... else` unpacks a success, and runs the block on anything else.
    let Ok(amount) = raw.parse::<u32>() else {
        return Err(AmountError::NotANumber(raw.to_string()));
    };

    if amount == 0 {
        return Err(AmountError::NotPositive);
    }
    Ok(amount)
}

#[test]
fn lesson_1_failures_are_values_you_can_compare() {
    assert_eq!(parse_amount("499"), Ok(499));
    assert_eq!(parse_amount("0"), Err(AmountError::NotPositive));
    assert_eq!(
        parse_amount("banana"),
        Err(AmountError::NotANumber("banana".to_string()))
    );
}

// ─────────────────────────────────────────────────────────────
// Lesson 2: composing two steps with different failures.
//
// GAP. Nothing works out `AmountError or ProviderError` for you. You write the
// union as an enum. `#[from]` generates the conversion that `?`
// calls, so the steps themselves stay clean.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Error)]
#[error("provider declined")]
struct ProviderDeclined;

#[derive(Debug, PartialEq, Error)]
enum PayError {
    #[error(transparent)]
    Amount(#[from] AmountError),
    #[error(transparent)]
    Provider(#[from] ProviderDeclined),
}

fn charge(amount: u32) -> Result<String, ProviderDeclined> {
    if amount > 1000 {
        return Err(ProviderDeclined);
    }
    Ok(format!("charged {amount}"))
}

// `?` means: on Err, return it now. On Ok, hand me the value.
fn pay(raw: &str) -> Result<String, PayError> {
    let amount = parse_amount(raw)?; // AmountError -> PayError
    let receipt = charge(amount)?; // ProviderDeclined -> PayError
    Ok(receipt)
}

#[test]
fn lesson_2_question_mark_converts_each_error_into_the_union() {
    assert_eq!(pay("499"), Ok("charged 499".to_string()));
    assert_eq!(pay("0"), Err(PayError::Amount(AmountError::NotPositive)));
    assert_eq!(pay("5000"), Err(PayError::Provider(ProviderDeclined)));
}

// ─────────────────────────────────────────────────────────────
// Lesson 3: recovering.
//
// Once every failure is handled, the return type drops `Result`. The signature
// now promises that this function can't fail, and the compiler holds you to it.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
struct UserNotFound;

fn find_name(id: u32) -> Result<String, UserNotFound> {
    if id == 1 {
        return Ok("Alice".to_string());
    }
    Err(UserNotFound)
}

fn greet_or_guest(id: u32) -> String {
    let name = match find_name(id) {
        Ok(name) => name,
        Err(UserNotFound) => "guest".to_string(),
    };
    format!("Hello {name}")
}

#[test]
fn lesson_3_handling_the_only_error_removes_result_from_the_type() {
    assert_eq!(greet_or_guest(1), "Hello Alice");
    assert_eq!(greet_or_guest(2), "Hello guest");
}

// ─────────────────────────────────────────────────────────────
// Lesson 4: exhaustive matching.
//
// ┌─ PREDICT ─────────────────────────────────────────────────┐
// │ Someone adds a new variant to an error enum. Does a match │
// │ that was complete yesterday still compile?                │
// └───────────────────────────────────────────────────────────┘
//
// Answer: tests/compile_fail/ch01_new_variant_not_handled.rs and its .stderr.
// The compiler lists every match that needs updating, which turns "add an error
// case" into a to-do list a person or an agent can work through.
//
// One way to lose this: `_ => ...` swallows future variants with no warning. Our
// `wildcard_enum_match_arm` lint rejects that.
//
// Ignoring a Result has its own test: ch01_ignored_result.rs.
// ─────────────────────────────────────────────────────────────

#[test]
fn lesson_4_a_new_variant_breaks_every_match_that_misses_it() {
    crate::assert_rejected("ch01_new_variant_not_handled", "E0004");
}

#[test]
fn lesson_4_an_ignored_result_does_not_compile() {
    crate::assert_rejected("ch01_ignored_result", "unused `Result` that must be used");
}

// ─────────────────────────────────────────────────────────────
// Lesson 5: handling SOME errors.
//
// GAP. You'd like to handle NotFound and have the type say the other errors remain.
// Enums are nominal: a type can't lose a variant, so Rust makes you write the second
// enum. Languages with error-channel operators get this free. Here you write it, and
// the compiler checks it.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
enum LoadError {
    NotFound,
    Timeout,
    Corrupt,
}

#[derive(Debug, PartialEq)]
enum LoadErrorAfterDefault {
    Timeout,
    Corrupt,
}

fn load(key: &str) -> Result<String, LoadError> {
    match key {
        "ok" => Ok("value".to_string()),
        "slow" => Err(LoadError::Timeout),
        "bad" => Err(LoadError::Corrupt),
        _ => Err(LoadError::NotFound),
    }
}

fn load_or_default(key: &str) -> Result<String, LoadErrorAfterDefault> {
    match load(key) {
        Ok(value) => Ok(value),
        Err(LoadError::NotFound) => Ok("default".to_string()),
        Err(LoadError::Timeout) => Err(LoadErrorAfterDefault::Timeout),
        Err(LoadError::Corrupt) => Err(LoadErrorAfterDefault::Corrupt),
    }
}

#[test]
fn lesson_5_narrowing_an_error_takes_a_second_enum() {
    assert_eq!(load_or_default("missing"), Ok("default".to_string()));
    assert_eq!(load_or_default("slow"), Err(LoadErrorAfterDefault::Timeout));
    assert_eq!(load_or_default("bad"), Err(LoadErrorAfterDefault::Corrupt));
    assert_eq!(load_or_default("ok"), Ok("value".to_string()));
}

// ─────────────────────────────────────────────────────────────
// Lesson 6: bugs are not errors.
//
// Err means "this can happen, decide what to do".
// A panic means "our code is wrong", and it doesn't appear in the signature.
//
// Our lints deny `unwrap`, `expect` and `panic!`, because those are the quick
// ways to turn an expected failure into a crash. A deliberate one needs an
// `#[expect]` with a reason, which makes it easy to spot in review.
//
// This split is the spine of the whole repo, so it's enforced: `.unwrap()` in
// domain code fails the build, tested below with the repo's real lint config.
// ─────────────────────────────────────────────────────────────

#[expect(clippy::panic, reason = "a lesson about what a defect looks like")]
fn split_evenly(total: u32, people: u32) -> u32 {
    if people == 0 {
        panic!("bug: caller promised at least one person");
    }
    total / people
}

#[test]
#[should_panic(expected = "bug: caller promised")]
fn lesson_6_a_panic_is_a_defect_not_a_typed_failure() {
    split_evenly(10, 0);
}

#[test]
fn lesson_6_the_build_rejects_unwrap() -> Result<(), std::io::Error> {
    crate::ch06_guard_rails::assert_rail_rejects("unwrap_used")
}

// ─────────────────────────────────────────────────────────────
// Lesson 7: the failure taxonomy, at the boundary.
//
// Inside, failures are one domain enum. At the edge, one exhaustive function decides
// what each kind means to the caller. Add a variant and this stops compiling until
// someone decides. A `String` error could never do that, because there's nothing to
// match on.
//
// The boundary also decides what leaks. `Storage` carries detail for your logs, and
// the caller gets none of it. A bug never reaches this function: it's a panic, and
// the server's top level turns that into a 500.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Error)]
enum CheckoutError {
    #[error("{field} is invalid")]
    Invalid { field: &'static str },
    #[error("this order is already being processed")]
    Conflict,
    #[error("the payment provider is down, try again soon")]
    Unavailable,
    #[error("storage failed: {0}")]
    Storage(String),
}

#[derive(Debug, PartialEq)]
struct Response {
    status: u16,
    body: String,
}

fn to_response(error: &CheckoutError) -> Response {
    let (status, body) = match error {
        CheckoutError::Invalid { .. } => (400, error.to_string()),
        CheckoutError::Conflict => (409, error.to_string()),
        CheckoutError::Unavailable => (503, error.to_string()),
        CheckoutError::Storage(_) => (500, "something went wrong".to_string()),
    };
    Response { status, body }
}

#[test]
fn lesson_7_each_kind_of_failure_gets_its_own_response() {
    let invalid = CheckoutError::Invalid { field: "email" };
    assert_eq!(
        to_response(&invalid),
        Response {
            status: 400,
            body: "email is invalid".to_string()
        }
    );
    assert_eq!(to_response(&CheckoutError::Conflict).status, 409);
    assert_eq!(to_response(&CheckoutError::Unavailable).status, 503);
}

#[test]
fn lesson_7_internal_detail_stays_inside() {
    let error = CheckoutError::Storage("postgres://admin:secret@db".to_string());

    let response = to_response(&error);

    assert_eq!(response.status, 500);
    assert!(!response.body.contains("secret"));
}

// ─────────────────────────────────────────────────────────────
// Exercise: `cargo test ch01 -- --ignored`
//
//   1. Add a `Frozen` variant to PayError, with a `charge` that returns it.
//   2. Run `make check` and read what the compiler asks for.
//   3. Write a `pay_message(raw) -> String` that matches every PayError
//      variant, then assert on it here.
// ─────────────────────────────────────────────────────────────

#[test]
#[ignore = "exercise: done when pay_message covers every PayError variant and make check passes"]
fn exercise_add_a_variant_and_follow_the_compiler() {
    todo!("replace this with your test")
}
