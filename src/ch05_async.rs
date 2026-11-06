// ═════════════════════════════════════════════════════════════
// Chapter 5: async. Time, concurrency and cancellation.
//
// New to this? Read guide/05-async.md first. It explains the ideas and why they exist.
//
// Run:  cargo test ch05
//
//   work that hasn't started  ->  a future you haven't awaited
//   a deadline                ->  tokio::time::timeout
//   trying again              ->  a loop, a policy, and a typed reason to retry
//   several at once           ->  tokio::join! and try_join!
//   tasks that die with you   ->  JoinSet
//   a fake clock in tests     ->  #[tokio::test(start_paused = true)]
//
// The outcome: you can see where a function may stop, because every `.await` is an
// exit (lesson 6), and retries are safe, because a request keeps its identity across
// attempts (lesson 8). A fake clock proves both with no real waiting.
// ═════════════════════════════════════════════════════════════

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio::time::error::Elapsed;
use tokio::time::{Instant, sleep, timeout};

// ─────────────────────────────────────────────────────────────
// Lesson 1: a future does nothing until you await it.
//
// Calling an async fn builds a description of the work. So a forgotten `.await`
// means the work never happens, and that's why the lints reject it. See
// tests/compile_fail/ch05_forgotten_future.rs.
// ─────────────────────────────────────────────────────────────

async fn record_call(calls: &AtomicU32) {
    calls.fetch_add(1, Ordering::SeqCst);
}

#[tokio::test]
async fn lesson_1_nothing_runs_until_awaited() {
    let calls = AtomicU32::new(0);

    let future = record_call(&calls);
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    future.await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn lesson_1_a_forgotten_await_does_not_compile() {
    crate::assert_rejected("ch05_forgotten_future", "unused implementer of `Future`");
}

// ─────────────────────────────────────────────────────────────
// Lesson 2: a timeout is an ordinary typed failure.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
struct TimedOut;

impl From<Elapsed> for TimedOut {
    fn from(_: Elapsed) -> Self {
        TimedOut
    }
}

async fn fetch(takes: Duration, deadline: Duration) -> Result<&'static str, TimedOut> {
    let work = async {
        sleep(takes).await;
        "data"
    };
    Ok(timeout(deadline, work).await?)
}

#[tokio::test(start_paused = true)]
async fn lesson_2_a_timeout_is_a_value_and_the_clock_is_fake() {
    let start = Instant::now();

    let outcome = fetch(Duration::from_secs(3600), Duration::from_secs(5)).await;

    assert_eq!(outcome, Err(TimedOut));
    assert!(start.elapsed() >= Duration::from_secs(5));
    assert!(start.elapsed() < Duration::from_secs(6));
}

// ─────────────────────────────────────────────────────────────
// Lesson 3: retrying.
//
// Retry is a loop. What matters is that "should I retry?" is an exhaustive match
// on the error type, so a new error can't pick a retry behaviour by accident.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone, Copy)]
enum CallError {
    Unavailable,
    Rejected,
}

impl CallError {
    fn is_retryable(self) -> bool {
        match self {
            CallError::Unavailable => true,
            CallError::Rejected => false,
        }
    }
}

struct RetryPolicy {
    max_attempts: u32,
    first_delay: Duration,
}

// `impl AsyncFnMut() -> ...` accepts any async closure you can call more than once.
async fn retry<T>(
    policy: &RetryPolicy,
    mut call: impl AsyncFnMut() -> Result<T, CallError>,
) -> Result<T, CallError> {
    let mut delay = policy.first_delay;
    let mut attempt = 1;
    loop {
        match call().await {
            Err(error) if error.is_retryable() && attempt < policy.max_attempts => {
                sleep(delay).await;
                delay *= 2;
                attempt += 1;
            }
            outcome => return outcome,
        }
    }
}

const POLICY: RetryPolicy = RetryPolicy {
    max_attempts: 3,
    first_delay: Duration::from_millis(100),
};

#[tokio::test(start_paused = true)]
async fn lesson_3_retries_with_backoff_until_it_works() {
    let start = Instant::now();
    let mut attempts = 0;

    let outcome = retry(&POLICY, async || {
        attempts += 1;
        if attempts < 3 {
            return Err(CallError::Unavailable);
        }
        Ok("done")
    })
    .await;

    assert_eq!(outcome, Ok("done"));
    assert_eq!(attempts, 3);
    assert!(start.elapsed() >= Duration::from_millis(300)); // 100ms, then 200ms
}

#[tokio::test(start_paused = true)]
async fn lesson_3_gives_up_after_the_last_attempt() {
    let mut attempts = 0;

    let outcome: Result<(), CallError> = retry(&POLICY, async || {
        attempts += 1;
        Err(CallError::Unavailable)
    })
    .await;

    assert_eq!(outcome, Err(CallError::Unavailable));
    assert_eq!(attempts, 3);
}

#[tokio::test(start_paused = true)]
async fn lesson_3_never_retries_a_rejection() {
    let mut attempts = 0;

    let outcome: Result<(), CallError> = retry(&POLICY, async || {
        attempts += 1;
        Err(CallError::Rejected)
    })
    .await;

    assert_eq!(outcome, Err(CallError::Rejected));
    assert_eq!(attempts, 1);
}

// ─────────────────────────────────────────────────────────────
// Lesson 4: doing several things at once.
//
// `join!` waits for all of them. `try_join!` stops at the first failure and drops
// the rest, which cancels them (lesson 6 explains what that means).
// ─────────────────────────────────────────────────────────────

async fn take(seconds: u64) -> u64 {
    sleep(Duration::from_secs(seconds)).await;
    seconds
}

#[tokio::test(start_paused = true)]
async fn lesson_4_join_takes_as_long_as_the_slowest() {
    let start = Instant::now();

    let (two, three) = tokio::join!(take(2), take(3));

    assert_eq!((two, three), (2, 3));
    assert!(start.elapsed() < Duration::from_secs(4)); // not 5
}

#[tokio::test(start_paused = true)]
async fn lesson_4_try_join_stops_at_the_first_failure() {
    let finished = AtomicU32::new(0);
    let fails_fast = async {
        sleep(Duration::from_secs(1)).await;
        Err::<(), _>("boom")
    };
    let slow = async {
        sleep(Duration::from_secs(60)).await;
        finished.fetch_add(1, Ordering::SeqCst);
        Ok(())
    };

    let outcome = tokio::try_join!(fails_fast, slow);

    assert_eq!(outcome, Err("boom"));
    assert_eq!(finished.load(Ordering::SeqCst), 0);
}

// ─────────────────────────────────────────────────────────────
// Lesson 5: who owns a background task?
//
// `tokio::spawn` detaches: dropping the handle doesn't stop the task. A JoinSet
// owns its tasks, so they stop when it goes out of scope. Prefer it, and no task
// outlives the code that started it.
// ─────────────────────────────────────────────────────────────

async fn finish_after_a_second(finished: Arc<AtomicU32>) {
    sleep(Duration::from_secs(1)).await;
    finished.fetch_add(1, Ordering::SeqCst);
}

#[tokio::test(start_paused = true)]
async fn lesson_5_a_spawned_task_outlives_its_handle() {
    let finished = Arc::new(AtomicU32::new(0));

    let handle = tokio::spawn(finish_after_a_second(finished.clone()));
    drop(handle);
    sleep(Duration::from_secs(2)).await;

    assert_eq!(finished.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn lesson_5_a_join_set_stops_its_tasks_when_dropped() {
    let finished = Arc::new(AtomicU32::new(0));

    let mut tasks = JoinSet::new();
    tasks.spawn(finish_after_a_second(finished.clone()));
    drop(tasks);
    sleep(Duration::from_secs(2)).await;

    assert_eq!(finished.load(Ordering::SeqCst), 0);
}

// ─────────────────────────────────────────────────────────────
// Lesson 6: every `.await` is an exit. GAP.
//
// A timeout, try_join! or select! cancels by dropping the future. It stops at
// its current `.await`. There's no error at that point and nothing on the line to
// see, so read every `.await` the way you'd read a `?`: this function may end here.
//
// ┌─ PREDICT ─────────────────────────────────────────────────┐
// │ A transfer takes money out, makes a network call, then    │
// │ pays it in. It's cancelled during the network call.       │
// │ Where's the money?                                        │
// └───────────────────────────────────────────────────────────┘
//
// Answer: gone, and the compiler has nothing to say about it. Two fixes: make the steps atomic (a database
// transaction), or run the critical part as its own task, which a dropped handle
// doesn't stop (lesson 5).
// ─────────────────────────────────────────────────────────────

struct Accounts {
    from: AtomicU32,
    to: AtomicU32,
}

impl Accounts {
    fn new() -> Self {
        Accounts {
            from: AtomicU32::new(100),
            to: AtomicU32::new(0),
        }
    }

    fn total(&self) -> u32 {
        self.from.load(Ordering::SeqCst) + self.to.load(Ordering::SeqCst)
    }
}

async fn transfer(accounts: &Accounts, amount: u32) {
    accounts.from.fetch_sub(amount, Ordering::SeqCst);
    sleep(Duration::from_secs(1)).await; // the network
    accounts.to.fetch_add(amount, Ordering::SeqCst);
}

#[tokio::test(start_paused = true)]
async fn lesson_6_cancellation_can_lose_money() {
    let accounts = Accounts::new();

    let outcome = timeout(Duration::from_millis(100), transfer(&accounts, 10)).await;

    assert!(outcome.is_err());
    assert_eq!(accounts.total(), 90); // 10 vanished
}

#[tokio::test(start_paused = true)]
async fn lesson_6_a_spawned_critical_section_survives_cancellation() {
    let accounts = Arc::new(Accounts::new());
    let for_task = accounts.clone();

    let task = tokio::spawn(async move { transfer(&for_task, 10).await });
    let outcome = timeout(Duration::from_millis(100), task).await;
    assert!(outcome.is_err()); // the caller stopped waiting

    sleep(Duration::from_secs(2)).await;
    assert_eq!(accounts.total(), 100); // the transfer finished anyway
}

// GAP. A spawned task outlives its caller, and it doesn't outlive the runtime.
// Shutting down drops it at its next `.await`, so the line below that never runs.
#[test]
fn lesson_6_runtime_shutdown_drops_a_spawned_task() -> Result<(), std::io::Error> {
    let finished = Arc::new(AtomicU32::new(0));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;

    runtime.spawn(finish_after_a_second(finished.clone()));
    runtime.block_on(tokio::task::yield_now()); // the task starts, then waits
    drop(runtime);

    assert_eq!(finished.load(Ordering::SeqCst), 0);
    Ok(())
}

async fn three_steps(done: &AtomicU32) {
    for _ in 0..3 {
        sleep(Duration::from_secs(1)).await;
        done.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test(start_paused = true)]
async fn lesson_6_each_await_is_a_place_the_function_can_end() {
    for (cancel_after_ms, steps_done) in [(500, 0), (1500, 1), (2500, 2), (3500, 3)] {
        let done = AtomicU32::new(0);

        let outcome = timeout(Duration::from_millis(cancel_after_ms), three_steps(&done)).await;

        assert_eq!(outcome.is_ok(), steps_done == 3);
        assert_eq!(done.load(Ordering::SeqCst), steps_done);
    }
}

// ─────────────────────────────────────────────────────────────
// Lesson 7: holding a lock across `.await`.
//
// A std Mutex guard held across `.await` in a spawned task is a compile error,
// because the task might resume on another thread. See
// tests/compile_fail/ch05_lock_across_await.rs. Where that rule doesn't apply, the
// `await_holding_lock` lint covers it (chapter 6).
// ─────────────────────────────────────────────────────────────

#[test]
fn lesson_7_holding_a_lock_across_await_does_not_compile() {
    crate::assert_rejected(
        "ch05_lock_across_await",
        "future cannot be sent between threads safely",
    );
}

// ─────────────────────────────────────────────────────────────
// Lesson 8: a retry is only safe if the side effect is idempotent.
//
// A timeout doesn't mean the work failed. It means you don't know. Here the bank
// takes the money and the reply is lost, so the client retries.
//
// ┌─ PREDICT ─────────────────────────────────────────────────┐
// │ The client retries with a new reference, then with the    │
// │ same one. How many times is the customer charged?         │
// └───────────────────────────────────────────────────────────┘
//
// The pattern is stable retry identity: pick the reference once, before the first
// attempt, so every attempt is the same request. The other side can then spot the
// repeat. Chapter 7 uses the idempotency key for this.
// ─────────────────────────────────────────────────────────────

#[derive(Default)]
struct Bank {
    charged: Mutex<HashSet<String>>,
    calls: AtomicU32,
}

impl Bank {
    async fn charge(&self, reference: String) -> Result<(), CallError> {
        self.charged.lock().await.insert(reference);
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            sleep(Duration::from_secs(60)).await; // money taken, reply lost
        }
        Ok(())
    }
}

async fn pay_with_retries(bank: &Bank, mut reference: impl FnMut() -> String) {
    let outcome = retry(&POLICY, async || {
        timeout(Duration::from_secs(1), bank.charge(reference()))
            .await
            .unwrap_or(Err(CallError::Unavailable))
    })
    .await;
    assert_eq!(outcome, Ok(()));
}

#[tokio::test(start_paused = true)]
async fn lesson_8_a_new_reference_per_attempt_charges_twice() {
    let bank = Bank::default();
    let mut attempt = 0;

    pay_with_retries(&bank, || {
        attempt += 1;
        format!("order-1-attempt-{attempt}")
    })
    .await;

    assert_eq!(bank.charged.lock().await.len(), 2);
}

#[tokio::test(start_paused = true)]
async fn lesson_8_a_stable_reference_charges_once() {
    let bank = Bank::default();

    pay_with_retries(&bank, || "order-1".to_string()).await;

    assert_eq!(bank.charged.lock().await.len(), 1);
}

// ─────────────────────────────────────────────────────────────
// Exercise: `cargo test ch05 -- --ignored`
//
//   1. Add a `Throttled { retry_after: Duration }` variant to CallError.
//   2. Follow the compiler. It stops at `is_retryable`, so you must decide.
//   3. Make `retry` wait `retry_after` for that variant, and prove the wait with
//      the fake clock.
// ─────────────────────────────────────────────────────────────

#[test]
#[ignore = "exercise: done when the fake clock proves the wait equals retry_after"]
fn exercise_respect_a_retry_after() {
    todo!("replace this with your test")
}
