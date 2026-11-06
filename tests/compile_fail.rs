//! Rust's version of `@ts-expect-error`. Each file in `compile_fail/` must fail
//! to build, and its error must match the `.stderr` next to it, so a file can't
//! pass by failing for the wrong reason.

#[test]
fn programs_that_must_not_compile() {
    trybuild::TestCases::new().compile_fail("tests/compile_fail/*.rs");
}
