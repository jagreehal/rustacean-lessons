---
name: rust-guard-rails
description: Adds denied Clippy lints and banned methods to a Rust project so unwrap, indexing, wildcard matches, hidden inputs and silent opt-outs fail the build. Use when setting up a Rust project or CI, tightening lints, or when a build fails on one of these lints and you need the right fix.
---

# Rust guard rails

A rule that fails the build doesn't depend on anyone reading it. An agent that breaks one gets an error naming the lint and fixes it on the next attempt.

## Install

1. Add this to `Cargo.toml`. In a workspace, put it under `[workspace.lints]` and add `lints.workspace = true` to each member.

```toml
[lints.rust]
unsafe_code = "forbid"
unused_must_use = "deny"

[lints.clippy]
all = { level = "deny", priority = -1 }
allow_attributes = "deny"
allow_attributes_without_reason = "deny"
as_conversions = "deny"
await_holding_lock = "deny"
disallowed_methods = "deny"
expect_used = "deny"
indexing_slicing = "deny"
let_underscore_future = "deny"
let_underscore_must_use = "deny"
map_err_ignore = "deny"
panic = "deny"
unwrap_used = "deny"
wildcard_enum_match_arm = "deny"
```

2. Add this `clippy.toml` beside it:

```toml
# Ambient inputs a function could grab without saying so in its signature.
# Convention isn't enough, so these fail the build.
disallowed-methods = [
  { path = "std::time::Instant::now", reason = "take a clock through deps" },
  { path = "std::time::SystemTime::now", reason = "take a clock through deps" },
  { path = "std::env::var", reason = "read config in main and pass it through deps" },
  { path = "std::thread::sleep", reason = "blocks the async runtime, use tokio::time::sleep" },
]
```

3. Run `cargo clippy --all-targets -- -D warnings` in CI, so tests follow the rules too.

In an existing project, the first run may report hundreds of errors. Add one lint at a time, and fix each error with the table below. Ask before you skip a lint.

## Fixing a failure

| Lint | The mistake | Do this instead |
|---|---|---|
| `unwrap_used`, `expect_used` | A failure becomes a crash | Return a `Result` with `?` |
| `panic` | A crash a caller could cause | Return an `Err` |
| `indexing_slicing` | `items[0]` panics on an empty list | `items.first()` gives an `Option` |
| `as_conversions` | `300 as u8` becomes 44 | `u8::try_from(n)` gives a `Result` |
| `wildcard_enum_match_arm` | `_ =>` swallows new variants | List each variant |
| `let_underscore_must_use` | `let _ = save()` hides a failure | Handle the `Result` |
| `map_err_ignore` | `.map_err(\|_\| ...)` loses the cause | Keep the source, or convert with `From` |
| `let_underscore_future` | A future that never runs | `.await` it |
| `await_holding_lock` | A lock held during a slow wait | Release it before `.await` |
| `disallowed_methods` | Hidden inputs like the clock | Pass them through deps |
| `allow_attributes`, `allow_attributes_without_reason` | Silent opt-outs | `#[expect(lint, reason = "...")]` |

Fix the code. Reach for an exception only when the code must do the banned thing, such as the one real clock implementation or the config loader in `main`:

```rust
#[expect(clippy::disallowed_methods, reason = "the one place that reads the real clock")]
pub fn now() -> std::time::Instant {
    std::time::Instant::now()
}
```

`#[expect]` states a reason your reviewer can read, and it fails the build once the lint stops firing, so stale exceptions can't pile up.

## GAP

These three throw away a `Result` and pass every rail, so review has to find them:

```rust
_ = save();
save().ok();
drop(save());
```

Config rots too. Keep one small example per lint that must fail, and test it, so a tool upgrade can't turn a rule off unnoticed.

Optional background, with the tests behind these rules: https://github.com/jagreehal/rustacean-lessons/blob/main/guide/06-guard-rails.md
