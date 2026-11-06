//! Lessons in writing Rust services where wrong programs fail the build. See README.md.
#![cfg(test)]

mod ch01_errors;
mod ch02_services;
mod ch03_resources;
mod ch04_illegal_states;
mod ch05_async;
pub(crate) mod ch06_guard_rails;
mod ch07_payment;

/// Names a compile-time claim in `cargo test` output, so it reads like any lesson.
/// `tests/compile_fail.rs` proves the program fails. This points at the proof.
fn assert_rejected(program: &str, expected: &str) {
    let path = format!("tests/compile_fail/{program}.stderr");
    let errors = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        errors.contains(expected),
        "{path} should contain {expected}:\n{errors}"
    );
}
