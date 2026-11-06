# Agent notes

Lessons in writing Rust services where wrong programs fail the build. All lesson
code is test code. Readers are new to Rust, so `guide/` explains the ideas and why
they exist, and `src/` proves them.

- `make check` is the only gate. It must pass before you call anything done.
- Explain by concept and outcome. Don't compare against other libraries by name.
- Lead with what Rust gives you. Any comparison comes second.
- No frameworks, and nothing that builds the dependency graph for you. Deps stay a
  struct you can read, and `?` stays the way steps compose.
- Say tasks and `JoinSet`. Teach validation as newtypes and `TryFrom`.
- No `#[allow]`. Use `#[expect(lint, reason = "...")]`, and only when the lesson is
  about that behaviour.
- Services: `fn(args, deps)` with a small deps struct per function, holding
  `Arc<dyn Trait>`. Traits only at I/O edges. Never one shared `AppDeps`.
- Stubs and spies are hand-written. No mock libraries.
- Errors are enums, and each `match` lists every variant. No `_ =>` on an enum, no `unwrap`.
- A claim that something can't compile needs a file in `tests/compile_fail/`.
  Create its `.stderr` with `TRYBUILD=overwrite cargo test --test compile_fail`,
  then read it to check it fails for the reason the lesson gives.
- A new denied lint in `Cargo.toml` needs `tests/lint_fail/<lint>.rs`.
- Writing: explain each new concept and why it exists before using it. Use Mermaid
  diagrams in `guide/`, because they don't render in `.rs` comments. Prefer active
  voice and "you", no em dashes, no filler adverbs, and state the point directly with no contrast setups.
- Be honest about costs. Mark them `GAP` in the lesson and list them in the README.
