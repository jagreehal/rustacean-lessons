// ═════════════════════════════════════════════════════════════
// Chapter 6: guard rails. Closing the holes the language leaves open.
//
// New to this? Read guide/06-guard-rails.md first. It explains the ideas and why they exist.
//
// Run:  cargo test ch06
//
// The compiler stops a lot, but Rust still lets you write `.unwrap()`, `items[0]`
// and `_ =>`. Those are the fast ways out, for tired people and for coding agents
// alike. So this repo denies them in Cargo.toml and clippy.toml, which turns "please
// remember" into a failed build.
//
// The outcome: a rule that fails the build doesn't depend on anyone reading it. An
// agent that breaks one gets an error telling it what to do instead.
//
// Rules that aren't tested rot, so this chapter tests the rails themselves. Every
// lint denied in Cargo.toml must have a file in tests/lint_fail/ with that name,
// and the file must trip it. Each one starts with a comment on what it prevents.
// ═════════════════════════════════════════════════════════════

use std::fs;
use std::process::{Command, Output};
use std::thread;

// ─────────────────────────────────────────────────────────────
// The rails are read from Cargo.toml, so these tests check the real config.
// The format is part of the contract: a rail must be a line reading `name = "deny"`
// under [lints.clippy]. Anything else isn't seen here, so it goes untested.
// ─────────────────────────────────────────────────────────────

fn denied_lints() -> Result<Vec<String>, std::io::Error> {
    let manifest = fs::read_to_string("Cargo.toml")?;
    let lints = manifest
        .lines()
        .skip_while(|line| *line != "[lints.clippy]")
        .filter_map(|line| line.strip_suffix(" = \"deny\""))
        .map(str::to_string)
        .collect();
    Ok(lints)
}

fn run_clippy(file: &str, lints: &[String]) -> Result<Output, std::io::Error> {
    // One directory per test thread, because two tests can check the same file.
    let out_dir = format!("target/guard_rails/{:?}", thread::current().id());
    let mut clippy = Command::new("clippy-driver");
    clippy
        .env("CLIPPY_CONF_DIR", ".")
        .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
        .args(["--out-dir", &out_dir, file]);
    for lint in lints {
        clippy.args(["-D", &format!("clippy::{lint}")]);
    }
    clippy.output()
}

// ─────────────────────────────────────────────────────────────
// Lesson 1: every rail catches the mistake it's there for.
// ─────────────────────────────────────────────────────────────

pub(crate) fn assert_rail_rejects(lint: &str) -> Result<(), std::io::Error> {
    let output = run_clippy(&format!("tests/lint_fail/{lint}.rs"), &denied_lints()?)?;
    let errors = String::from_utf8_lossy(&output.stderr);
    let name_in_output = lint.replace('_', "-");

    assert!(!output.status.success(), "{lint} let its example through");
    assert!(
        errors.contains(&format!("clippy::{name_in_output}")),
        "{lint} failed for another reason:\n{errors}"
    );
    Ok(())
}

#[test]
fn lesson_1_every_denied_lint_rejects_its_example() -> Result<(), std::io::Error> {
    let lints = denied_lints()?;
    assert!(lints.len() >= 10, "found only {lints:?}");

    for lint in &lints {
        assert_rail_rejects(lint)?;
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────
// Lesson 2: what gets through. GAP.
//
// These three all throw away a Result, and every rail here passes them:
//
//     _ = save();
//     save().ok();
//     drop(save());
//
// `let _ = save()` is caught, but its close relatives aren't. This test pins that
// down: if a future clippy starts catching them, it fails and tells us to update
// this lesson. Until then, a code review or a custom tool has to find them.
// ─────────────────────────────────────────────────────────────

#[test]
fn lesson_2_three_silent_drops_still_get_through() -> Result<(), std::io::Error> {
    let output = run_clippy("tests/lint_pass/silent_drops.rs", &denied_lints()?)?;
    let errors = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "a rail caught one:\n{errors}");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
// Exercise: `cargo test ch06 -- --ignored`
//
//   1. Pick a mistake you've made in Rust and find the clippy lint for it.
//   2. Deny it in Cargo.toml and run `make check`. Lesson 1 now fails, because
//      the rail has no example.
//   3. Write tests/lint_fail/<lint>.rs.
// ─────────────────────────────────────────────────────────────

#[test]
#[ignore = "exercise: done when make check passes with your new lint denied"]
fn exercise_add_your_own_rail() {
    todo!("delete this test when your rail passes")
}
