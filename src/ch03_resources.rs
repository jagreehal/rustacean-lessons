// ═════════════════════════════════════════════════════════════
// Chapter 3: resources. Building, sharing and cleaning up.
//
// New to this? Read guide/03-resources.md first. It explains the ideas and why they exist.
//
// Run:  cargo test ch03
//
//   building the app      ->  ordinary constructor functions, run from main
//   building can fail     ->  a constructor returning Result
//   sharing one instance  ->  Arc::clone
//   a separate instance   ->  calling the constructor again
//   guaranteed cleanup    ->  Drop, with a real gap for async (lesson 5)
//
// The outcome: what's shared is visible in the code, and a resource can't be used
// after it's closed. The cost is async cleanup, and lesson 5 shows it.
// ═════════════════════════════════════════════════════════════

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

// ─────────────────────────────────────────────────────────────
// Lesson 1: the composition root.
//
// No framework builds the app. `build_app` is ordinary code that runs top to
// bottom, so the order of construction is the order you read. Building can fail,
// and `?` covers that as it does anywhere else. A real app calls this once, in main.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
struct Connection {
    id: u32,
}

#[derive(Debug, PartialEq)]
struct ConnectError;

async fn connect(url: &str, opened: &AtomicU32) -> Result<Connection, ConnectError> {
    if url.is_empty() {
        return Err(ConnectError);
    }
    let id = opened.fetch_add(1, Ordering::SeqCst) + 1;
    Ok(Connection { id })
}

struct Repo {
    connection: Arc<Connection>,
}

struct App {
    users: Repo,
    orders: Repo,
}

// ─────────────────────────────────────────────────────────────
// Lesson 2: sharing is something you can see.
//
// Does the app open one connection or two? A DI container answers with scopes and
// lifetimes in its config. Here the answer is on the page: one `connect` call and
// two clones.
// ─────────────────────────────────────────────────────────────

async fn build_app(url: &str, opened: &AtomicU32) -> Result<App, ConnectError> {
    let connection = Arc::new(connect(url, opened).await?);
    Ok(App {
        users: Repo {
            connection: connection.clone(),
        },
        orders: Repo { connection },
    })
}

#[tokio::test]
async fn lesson_2_one_constructor_call_is_one_connection() -> Result<(), ConnectError> {
    let opened = AtomicU32::new(0);

    let app = build_app("db://live", &opened).await?;

    assert_eq!(app.users.connection.id, 1);
    assert_eq!(app.orders.connection.id, 1);
    assert_eq!(opened.load(Ordering::SeqCst), 1);
    Ok(())
}

// ─────────────────────────────────────────────────────────────
// Lesson 3: a separate instance is a second constructor call.
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn lesson_3_two_constructor_calls_are_two_connections() -> Result<(), ConnectError> {
    let opened = AtomicU32::new(0);

    let users = Repo {
        connection: Arc::new(connect("db://live", &opened).await?),
    };
    let orders = Repo {
        connection: Arc::new(connect("db://live", &opened).await?),
    };

    assert_eq!(users.connection.id, 1);
    assert_eq!(orders.connection.id, 2);
    Ok(())
}

#[tokio::test]
async fn lesson_3_a_failed_build_returns_an_error_not_an_app() {
    let opened = AtomicU32::new(0);
    assert!(matches!(build_app("", &opened).await, Err(ConnectError)));
}

// ─────────────────────────────────────────────────────────────
// Lesson 4: cleanup with nothing to remember.
//
// You implement the `Drop` trait, and Rust calls `drop` when the value's owner goes
// away. You don't call it yourself.
//
// ┌─ PREDICT ─────────────────────────────────────────────────┐
// │ The function returns early through `?`. Does "close" run? │
// │ What about a panic?                                       │
// └───────────────────────────────────────────────────────────┘
// ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<&'static str>>>);

impl Events {
    fn push(&self, event: &'static str) {
        // A panic while locked "poisons" a std Mutex. The list is still fine.
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event);
    }

    fn all(&self) -> Vec<&'static str> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

struct Guard {
    name_on_close: &'static str,
    events: Events,
}

impl Guard {
    fn open(name_on_open: &'static str, name_on_close: &'static str, events: &Events) -> Self {
        events.push(name_on_open);
        Guard {
            name_on_close,
            events: events.clone(),
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.events.push(self.name_on_close);
    }
}

fn use_guard_then_fail(events: &Events) -> Result<(), &'static str> {
    let _guard = Guard::open("open", "close", events);
    events.push("use");
    Err("boom")?;
    events.push("never reached");
    Ok(())
}

#[test]
fn lesson_4_drop_runs_on_early_return() {
    let events = Events::default();

    assert_eq!(use_guard_then_fail(&events), Err("boom"));

    assert_eq!(events.all(), vec!["open", "use", "close"]);
}

#[test]
fn lesson_4_drop_runs_on_panic() {
    let events = Events::default();

    #[expect(clippy::panic, reason = "the panic is what's being tested")]
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let _guard = Guard::open("open", "close", &events);
        panic!("bug");
    }));

    assert!(outcome.is_err());
    assert_eq!(events.all(), vec!["open", "close"]);
}

#[test]
fn lesson_4_resources_close_in_reverse_order() {
    let events = Events::default();
    {
        let _pool = Guard::open("open pool", "close pool", &events);
        let _session = Guard::open("open session", "close session", &events);
    }
    assert_eq!(
        events.all(),
        vec!["open pool", "open session", "close session", "close pool"]
    );
}

// Struct fields go the other way: they drop in the order you declare them. So a
// struct lists a session above the pool it came from.
#[test]
fn lesson_4_struct_fields_close_in_declaration_order() {
    struct Wired {
        _session: Guard,
        _pool: Guard,
    }

    let events = Events::default();
    {
        let _wired = Wired {
            _session: Guard::open("open session", "close session", &events),
            _pool: Guard::open("open pool", "close pool", &events),
        };
    }
    assert_eq!(
        events.all(),
        vec!["open session", "open pool", "close session", "close pool"]
    );
}

// ─────────────────────────────────────────────────────────────
// Lesson 5: async cleanup. GAP.
//
// `drop` is a plain fn, so it can't `.await`. Anything that must flush or say
// goodbye over the network needs an explicit `close().await`. A helper can
// make that hard to forget.
//
// ┌─ PREDICT ─────────────────────────────────────────────────┐
// │ with_session is cancelled by a timeout mid-use. Does the  │
// │ async close still run?                                    │
// └───────────────────────────────────────────────────────────┘
//
// Answer: no. Cancelling in Rust means dropping the future, so everything after
// the current `.await` never happens. Drop still runs, though, so that's the
// Rust-shaped fix: cleanup goes in Drop, and if it's async, Drop spawns it. That
// survives cancellation and not a runtime shutdown, so it still needs a backstop.
// Chapter 7's LockGuard is that pattern in full.
// ─────────────────────────────────────────────────────────────

struct Session {
    events: Events,
}

impl Session {
    async fn open(events: &Events) -> Self {
        events.push("open");
        Session {
            events: events.clone(),
        }
    }

    async fn close(self) {
        self.events.push("close");
    }
}

async fn with_session<T, E>(
    events: &Events,
    work: impl AsyncFnOnce(&Session) -> Result<T, E>,
) -> Result<T, E> {
    let session = Session::open(events).await;
    let outcome = work(&session).await;
    session.close().await;
    outcome
}

#[tokio::test]
async fn lesson_5_a_helper_closes_on_success_and_on_failure() {
    let events = Events::default();

    let success: Result<u32, &str> = with_session(&events, async |_| Ok(1)).await;
    let failure: Result<u32, &str> = with_session(&events, async |_| Err("boom")).await;

    assert_eq!(success, Ok(1));
    assert_eq!(failure, Err("boom"));
    assert_eq!(events.all(), vec!["open", "close", "open", "close"]);
}

#[tokio::test(start_paused = true)]
async fn lesson_5_cancellation_skips_async_cleanup() {
    let events = Events::default();

    let slow_work = with_session(&events, async |_| -> Result<(), &str> {
        tokio::time::sleep(Duration::from_secs(60)).await;
        Ok(())
    });
    let outcome = tokio::time::timeout(Duration::from_secs(1), slow_work).await;

    assert!(outcome.is_err());
    assert_eq!(events.all(), vec!["open"]); // no "close"
}

// ─────────────────────────────────────────────────────────────
// Lesson 6: use-after-close.
//
// Look at `close(self)` above. It takes `self` by value, so calling it moves
// the session away. Any later use isn't a runtime "connection closed" error.
// The program doesn't compile. See tests/compile_fail/ch03_use_after_close.rs.
//
// In most languages a closed handle is still an ordinary object, so this compiles
// and fails at runtime:
//
//     const conn = await open();
//     await conn.close();
//     await conn.query("SELECT 1");   // type-checks
//
// The limit: this needs a sole owner. Inside an Arc you can't call `close(self)`
// until the other clones have gone, as this test shows.
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn lesson_6_a_shared_session_cannot_be_closed_until_it_has_one_owner() {
    let events = Events::default();
    let first = Arc::new(Session::open(&events).await);
    let second = first.clone();

    // Two owners, so Arc refuses to hand the session back.
    let still_shared = Arc::try_unwrap(first).err();
    assert!(still_shared.is_some());
    drop(still_shared);

    // One owner left, so it can be closed.
    if let Ok(session) = Arc::try_unwrap(second) {
        session.close().await;
    }
    assert_eq!(events.all(), vec!["open", "close"]);
}

#[test]
fn lesson_6_using_a_closed_session_does_not_compile() {
    crate::assert_rejected("ch03_use_after_close", "E0382");
}

// ─────────────────────────────────────────────────────────────
// Exercise: `cargo test ch03 -- --ignored`
//
//   1. Give Session a Drop impl that records "dropped without close" when
//      close() was never called. That needs a flag, and it's a common pattern.
//   2. Rerun lesson 5's timeout and assert the warning appears.
//   3. Why does `close(self)` need to tell Drop not to fire?
// ─────────────────────────────────────────────────────────────

#[test]
#[ignore = "exercise: done when the timeout records the warning and a normal close does not"]
fn exercise_detect_a_session_dropped_without_close() {
    todo!("replace this with your test")
}
