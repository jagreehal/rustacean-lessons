// ═════════════════════════════════════════════════════════════
// Chapter 2: services. How a function tells you what it needs.
//
// New to this? Read guide/02-services.md first. It explains the ideas and why they exist.
//
// Run:  cargo test ch02
//
// Most code needs the outside world: a database, a clock, a payment provider.
// If a function reaches out and grabs those, its signature lies and its tests
// need the real thing. So make the need an argument:
//
//     async fn signup(args: SignupArgs, deps: &SignupDeps) -> Result<A, E>
//
// args is the data for this call. deps is everything from the outside world.
// That signature now answers all three questions: what you get (A), how it can
// fail (E), and what it needs (SignupDeps).
//
//   a service's contract  ->  trait (only at I/O edges: db, provider, clock)
//   using a service       ->  deps.db
//   an implementation     ->  a struct that implements the trait
//   wiring it up          ->  passing &deps
//
// The outcome: you can't run code with a dependency missing, and a test swaps
// any dependency without touching the code it's testing.
//
// House style: one small deps struct per function, holding Arc<dyn Trait>.
// Business logic stays as plain functions, not traits.
//
// `#[async_trait]` is needed because a trait with a native `async fn` can't be
// used as `dyn Trait` (E0038).
// ═════════════════════════════════════════════════════════════

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;

// ─────────────────────────────────────────────────────────────
// Lesson 1: a service is a name and a shape. The function takes it as an argument.
// ─────────────────────────────────────────────────────────────

// A trait lists methods with no implementation, like an interface.
// `Send + Sync` promises implementations are safe to share across threads.
#[async_trait]
trait Db: Send + Sync {
    async fn get_name(&self, id: u32) -> String;
}

// `dyn Db` means any type implementing Db, chosen at runtime. `Arc` is a shared
// pointer, so several owners can hold the same service.
struct GreetDeps {
    db: Arc<dyn Db>,
}

// Read the signature: returns a String, can't fail, needs a Db.
async fn greet(id: u32, deps: &GreetDeps) -> String {
    format!("Hello {}", deps.db.get_name(id).await)
}

struct DbReal;

#[async_trait]
impl Db for DbReal {
    async fn get_name(&self, id: u32) -> String {
        if id == 1 { "Alice" } else { "Bob" }.to_string()
    }
}

#[tokio::test]
async fn lesson_1_the_caller_hands_over_what_the_function_needs() {
    let deps = GreetDeps {
        db: Arc::new(DbReal),
    };
    assert_eq!(greet(1, &deps).await, "Hello Alice");
}

// ─────────────────────────────────────────────────────────────
// Lesson 2: a stub. `greet` doesn't change.
// ─────────────────────────────────────────────────────────────

struct DbStub;

#[async_trait]
impl Db for DbStub {
    async fn get_name(&self, _id: u32) -> String {
        "Stubby".to_string()
    }
}

#[tokio::test]
async fn lesson_2_swap_the_implementation_not_the_code() {
    let real = GreetDeps {
        db: Arc::new(DbReal),
    };
    let stub = GreetDeps {
        db: Arc::new(DbStub),
    };

    assert_eq!(greet(2, &real).await, "Hello Bob");
    assert_eq!(greet(2, &stub).await, "Hello Stubby");
}

// ─────────────────────────────────────────────────────────────
// Lesson 3: a slow implementation. `greet` still doesn't change.
//
// `start_paused` gives the test a fake clock. The sleep below takes no real time.
// ─────────────────────────────────────────────────────────────

struct DbSlow;

#[async_trait]
impl Db for DbSlow {
    async fn get_name(&self, _id: u32) -> String {
        tokio::time::sleep(Duration::from_secs(30)).await;
        "Alice".to_string()
    }
}

#[tokio::test(start_paused = true)]
async fn lesson_3_async_needs_no_change_and_no_real_waiting() {
    let deps = GreetDeps {
        db: Arc::new(DbSlow),
    };
    assert_eq!(greet(1, &deps).await, "Hello Alice");
}

// ─────────────────────────────────────────────────────────────
// Lesson 4: a spy.
//
// The test keeps one Arc and gives a clone to deps, so it can look inside
// afterwards. Keeping access to a dependency is one `.clone()`.
// ─────────────────────────────────────────────────────────────

#[derive(Default)]
struct DbSpy {
    calls: Mutex<Vec<u32>>,
}

#[async_trait]
impl Db for DbSpy {
    async fn get_name(&self, id: u32) -> String {
        self.calls.lock().await.push(id);
        "Spy".to_string()
    }
}

#[tokio::test]
async fn lesson_4_a_spy_records_how_the_service_was_used() {
    let spy = Arc::new(DbSpy::default());
    let deps = GreetDeps { db: spy.clone() };

    greet(42, &deps).await;

    assert_eq!(*spy.calls.lock().await, vec![42]);
}

// ─────────────────────────────────────────────────────────────
// Lesson 5: a service that depends on a service.
//
// Greeter holds its deps, so callers of `greeter.greet(1)` never hear about Db.
// It's fn(args, deps) with `self` as deps. Use it when callers shouldn't see
// what's underneath.
//
// ┌─ PREDICT ─────────────────────────────────────────────────┐
// │ You build a Greeter and forget its Db. A DI container     │
// │ would start up, then crash when it's first used.          │
// │ What happens here?                                        │
// └───────────────────────────────────────────────────────────┘
//
// Answer: tests/compile_fail/ch02_missing_dependency.rs. Without a Db there is
// no Greeter value at all, so nothing exists to run, and no error can ever reach
// runtime.
// ─────────────────────────────────────────────────────────────

struct Greeter {
    db: Arc<dyn Db>,
}

impl Greeter {
    async fn greet(&self, id: u32) -> String {
        format!("Hi {}!", self.db.get_name(id).await)
    }
}

#[tokio::test]
async fn lesson_5_callers_of_greeter_never_see_db() {
    let greeter = Greeter {
        db: Arc::new(DbStub),
    };
    assert_eq!(greeter.greet(1).await, "Hi Stubby!");
}

#[test]
fn lesson_5_a_greeter_without_a_db_does_not_compile() {
    crate::assert_rejected("ch02_missing_dependency", "E0063");
}

// ─────────────────────────────────────────────────────────────
// Lesson 6: a stub can fail, so sad paths need no broken database.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
struct UserNotFound {
    id: u32,
}

#[async_trait]
trait Users: Send + Sync {
    async fn find_name(&self, id: u32) -> Result<String, UserNotFound>;
}

struct UsersFound;

#[async_trait]
impl Users for UsersFound {
    async fn find_name(&self, _id: u32) -> Result<String, UserNotFound> {
        Ok("Alice".to_string())
    }
}

struct UsersMissing;

#[async_trait]
impl Users for UsersMissing {
    async fn find_name(&self, id: u32) -> Result<String, UserNotFound> {
        Err(UserNotFound { id })
    }
}

struct LookupDeps {
    users: Arc<dyn Users>,
}

async fn greet_or_guest(id: u32, deps: &LookupDeps) -> String {
    let name = match deps.users.find_name(id).await {
        Ok(name) => name,
        Err(UserNotFound { .. }) => "guest".to_string(),
    };
    format!("Hello {name}")
}

#[tokio::test]
async fn lesson_6_stubs_drive_both_paths() {
    let found = LookupDeps {
        users: Arc::new(UsersFound),
    };
    let missing = LookupDeps {
        users: Arc::new(UsersMissing),
    };

    assert_eq!(greet_or_guest(1, &found).await, "Hello Alice");
    assert_eq!(greet_or_guest(1, &missing).await, "Hello guest");
    // An error is an ordinary value, so you assert on it like any other.
    assert_eq!(
        missing.users.find_name(7).await,
        Err(UserNotFound { id: 7 })
    );
}

// ─────────────────────────────────────────────────────────────
// Lesson 7: a stateful fake.
//
// Each test builds its own, so nothing leaks between tests. State is shared only
// if you share the Arc.
// ─────────────────────────────────────────────────────────────

#[async_trait]
trait Store: Send + Sync {
    async fn save(&self, id: u32, name: String);
    async fn get(&self, id: u32) -> Option<String>;
}

#[derive(Default)]
struct StoreInMemory {
    rows: Mutex<HashMap<u32, String>>,
}

#[async_trait]
impl Store for StoreInMemory {
    async fn save(&self, id: u32, name: String) {
        self.rows.lock().await.insert(id, name);
    }

    async fn get(&self, id: u32) -> Option<String> {
        self.rows.lock().await.get(&id).cloned()
    }
}

#[tokio::test]
async fn lesson_7_save_then_read_back() {
    let store = StoreInMemory::default();
    store.save(1, "Alice".to_string()).await;
    assert_eq!(store.get(1).await, Some("Alice".to_string()));
}

#[tokio::test]
async fn lesson_7_the_next_test_starts_empty() {
    let store = StoreInMemory::default();
    assert_eq!(store.get(1).await, None);
}

// ─────────────────────────────────────────────────────────────
// Lesson 8: several dependencies.
//
// Needing two things is a struct with two fields. fn(args, deps) in full: args is the data for this call, deps is everything
// the function needs from the outside world.
// ─────────────────────────────────────────────────────────────

#[async_trait]
trait Logger: Send + Sync {
    async fn log(&self, message: String);
}

#[derive(Default)]
struct LoggerSpy {
    messages: Mutex<Vec<String>>,
}

#[async_trait]
impl Logger for LoggerSpy {
    async fn log(&self, message: String) {
        self.messages.lock().await.push(message);
    }
}

struct SignupArgs {
    id: u32,
    name: String,
}

struct SignupDeps {
    store: Arc<dyn Store>,
    logger: Arc<dyn Logger>,
}

async fn signup(args: SignupArgs, deps: &SignupDeps) {
    let message = format!("registered {}", args.name);
    deps.store.save(args.id, args.name).await;
    deps.logger.log(message).await;
}

#[tokio::test]
async fn lesson_8_in_memory_store_and_spy_logger() {
    let store = Arc::new(StoreInMemory::default());
    let logger = Arc::new(LoggerSpy::default());
    let deps = SignupDeps {
        store: store.clone(),
        logger: logger.clone(),
    };

    let args = SignupArgs {
        id: 1,
        name: "Alice".to_string(),
    };
    signup(args, &deps).await;

    assert_eq!(store.get(1).await, Some("Alice".to_string()));
    assert_eq!(
        *logger.messages.lock().await,
        vec!["registered Alice".to_string()]
    );
}

// ─────────────────────────────────────────────────────────────
// Lesson 9: three ways to state a need, side by side.
//
// All three are checked at compile time. You'll meet the other two in other
// people's code. This repo uses the first and nothing else.
//
//   deps struct    Plain and readable. Costs async-trait and a vtable call.
//   generic        No crate needed, but the type parameter spreads to everything
//                  that calls it or stores it.
//   HasDb          `impl HasDb + HasLogger` lists the needs in the signature.
//                  One accessor trait per service, implemented on every context.
//
// GAP. None of them infers. Some type systems work this out for you. Here you add
// the field and pass it down yourself, and the compiler checks it. That's more typing, not less safety.
// ─────────────────────────────────────────────────────────────

async fn greet_with_generic(id: u32, db: &impl Db) -> String {
    format!("Hello {}", db.get_name(id).await)
}

trait HasDb {
    fn db(&self) -> &dyn Db;
}

impl HasDb for GreetDeps {
    fn db(&self) -> &dyn Db {
        self.db.as_ref()
    }
}

async fn greet_with_has_db(id: u32, context: &impl HasDb) -> String {
    format!("Hello {}", context.db().get_name(id).await)
}

#[tokio::test]
async fn lesson_9_three_spellings_of_the_same_requirement() {
    let deps = GreetDeps {
        db: Arc::new(DbStub),
    };

    assert_eq!(greet(1, &deps).await, "Hello Stubby");
    assert_eq!(greet_with_generic(1, &DbStub).await, "Hello Stubby");
    assert_eq!(greet_with_has_db(1, &deps).await, "Hello Stubby");
}

// ─────────────────────────────────────────────────────────────
// Exercise: `cargo test ch02 -- --ignored`
//
//   1. Add a `Notifier` trait with `send(to, message)`.
//   2. Write `welcome(args, deps)`, saving to Store and notifying.
//      What fields does WelcomeDeps need?
//   3. Write a spy Notifier and assert one message was sent.
//   4. Bonus: make `send` return a Result and decide what `welcome` should
//      do when it fails after the save worked.
// ─────────────────────────────────────────────────────────────

#[test]
#[ignore = "exercise: done when one test proves the user was saved and exactly one message was sent"]
fn exercise_welcome_with_a_spy_notifier() {
    todo!("replace this with your test")
}
