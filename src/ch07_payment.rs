// ═════════════════════════════════════════════════════════════
// Chapter 7: capstone. A payment nobody can lose or charge twice.
//
// New to this? Read guide/07-payment.md first. It explains the ideas and why they exist.
//
// Run:  cargo test ch07
//
// Everything so far in one workflow. Failures come in kinds, and each kind gets
// a different response:
//
//   the caller's mistake    Invalid                 say what's wrong
//   a race with a retry     Conflict                ask them to try later
//   the provider said no    Rejected                don't retry
//   the provider is down    Unavailable             retry, then record it
//   our database is down    StoreDown               page someone
//   money moved, no record  ChargedButNotRecorded   page someone, now
//   a bug                   panic                   not a typed failure
//
//   request -> ValidPayment -> Locked -> Charged -> payment id
//
// Two patterns carry the reliability:
//
//   stable retry identity   every attempt sends the same reference (ch05 lesson 8)
//   a finalizer that survives cancellation   LockGuard frees the key on every exit
//
// Each arrow consumes the stage before it. So you can't charge without a lock
// (tests/compile_fail/ch07_charge_without_lock.rs) or charge the same stage twice.
//
// The tests below are executable stories: they run as tests and also write a
// report a non-Rust reader can follow. See the README for how to generate it.
// ═════════════════════════════════════════════════════════════

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use executable_stories::Story;
use serde_json::json;
use tokio::sync::{Mutex, Notify};
use tokio::time::timeout;

use payment::{
    ChargeRequest, PayError, PaymentDeps, PaymentRequest, PaymentStore, Provider, ProviderError,
    RetryPolicy, StoreDown, create_payment,
};

// ─────────────────────────────────────────────────────────────
// The workflow. The module boundary matters: the stage types are private, so code
// outside can only call `create_payment`, and can't skip a stage.
// ─────────────────────────────────────────────────────────────

mod payment {
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use thiserror::Error;
    use tokio::runtime::Handle;
    use tokio::time::{sleep, timeout};

    // Untrusted input, as it arrived.
    pub struct PaymentRequest {
        pub client_id: String,
        pub amount_minor: i64,
        pub currency: String,
        pub idempotency_key: String,
    }

    #[derive(Debug, PartialEq, Clone, Copy)]
    pub enum Currency {
        Gbp,
        Eur,
        Usd,
    }

    #[derive(Debug, PartialEq, Clone)]
    pub struct ChargeRequest {
        pub amount_minor: u32,
        pub currency: Currency,
        // Stable retry identity: the same on every attempt, so a provider that took
        // the money during a timed-out attempt won't take it again.
        pub reference: String,
    }

    #[derive(Debug, PartialEq)]
    pub struct StoreDown;

    #[derive(Debug, PartialEq)]
    pub enum ProviderError {
        Rejected(String),
        Unavailable,
    }

    #[derive(Debug, PartialEq, Error)]
    pub enum PayError {
        #[error("invalid {field}")]
        Invalid { field: &'static str },
        #[error("another request holds this idempotency key")]
        Conflict,
        #[error("provider rejected the payment: {0}")]
        Rejected(String),
        #[error("provider unavailable")]
        Unavailable,
        #[error("payment store unavailable")]
        StoreDown,
        #[error("charged as {payment_id}, but it wasn't recorded")]
        ChargedButNotRecorded { payment_id: String },
    }

    impl From<StoreDown> for PayError {
        fn from(_: StoreDown) -> Self {
            PayError::StoreDown
        }
    }

    impl PayError {
        // The boundary. Adding a variant fails to compile until it has a status.
        pub fn http_status(&self) -> u16 {
            match self {
                PayError::Invalid { .. } => 400,
                PayError::Conflict => 409,
                PayError::Rejected(_) => 402,
                PayError::Unavailable => 503,
                PayError::StoreDown | PayError::ChargedButNotRecorded { .. } => 500,
            }
        }
    }

    #[async_trait]
    pub trait PaymentStore: Send + Sync {
        async fn find_by_key(&self, key: &str) -> Result<Option<String>, StoreDown>;
        async fn acquire_lock(&self, key: &str) -> Result<bool, StoreDown>;
        async fn release_lock(&self, key: &str) -> Result<(), StoreDown>;
        async fn record_success(&self, key: &str, payment_id: &str) -> Result<(), StoreDown>;
        async fn record_failure(&self, key: &str) -> Result<(), StoreDown>;
    }

    #[async_trait]
    pub trait Provider: Send + Sync {
        async fn charge(&self, request: &ChargeRequest) -> Result<String, ProviderError>;
    }

    pub struct RetryPolicy {
        pub max_attempts: u32,
        pub first_delay: Duration,
        pub per_attempt: Duration,
    }

    pub struct PaymentDeps {
        pub store: Arc<dyn PaymentStore>,
        pub provider: Arc<dyn Provider>,
        pub retry: RetryPolicy,
    }

    // Stage 1. Holding one of these proves the input was checked.
    struct ValidPayment {
        key: String,
        charge: ChargeRequest,
    }

    impl TryFrom<PaymentRequest> for ValidPayment {
        type Error = PayError;

        fn try_from(request: PaymentRequest) -> Result<Self, PayError> {
            if request.client_id.is_empty() {
                return Err(PayError::Invalid { field: "client_id" });
            }
            if request.idempotency_key.is_empty() {
                return Err(PayError::Invalid {
                    field: "idempotency_key",
                });
            }
            let amount_minor = u32::try_from(request.amount_minor)
                .ok()
                .filter(|amount| *amount > 0)
                .ok_or(PayError::Invalid {
                    field: "amount_minor",
                })?;
            let currency = match request.currency.as_str() {
                "GBP" => Currency::Gbp,
                "EUR" => Currency::Eur,
                "USD" => Currency::Usd,
                _ => return Err(PayError::Invalid { field: "currency" }),
            };

            Ok(ValidPayment {
                charge: ChargeRequest {
                    amount_minor,
                    currency,
                    reference: request.idempotency_key.clone(),
                },
                key: request.idempotency_key,
            })
        }
    }

    impl ValidPayment {
        async fn lock(self, deps: &PaymentDeps) -> Result<Locked, PayError> {
            if deps.store.acquire_lock(&self.key).await? {
                let guard = LockGuard {
                    store: deps.store.clone(),
                    key: self.key.clone(),
                };
                return Ok(Locked {
                    payment: self,
                    guard,
                });
            }
            Err(PayError::Conflict)
        }
    }

    // A finalizer that survives cancellation. Every `.await` in this workflow is an
    // exit, and an `async` cleanup line would be skipped on the one that cancels.
    // Drop runs on all of them, but it can't await, so it hands the work to a task
    // that a cancelled request can't stop.
    //
    // Two limits. Nothing runs if the process dies, so a real lock also needs an expiry.
    // And the release lands a moment later, so an instant retry can see Conflict,
    // which is safe.
    struct LockGuard {
        store: Arc<dyn PaymentStore>,
        key: String,
    }

    impl Drop for LockGuard {
        fn drop(&mut self) {
            let store = self.store.clone();
            let key = std::mem::take(&mut self.key);
            let Ok(runtime) = Handle::try_current() else {
                return; // shutting down, so the expiry frees it
            };
            runtime.spawn(async move {
                if store.release_lock(&key).await.is_err() {
                    eprintln!("lock {key} not released, waiting for it to expire");
                }
            });
        }
    }

    // Stage 2. Holding one of these proves this request owns the key.
    struct Locked {
        payment: ValidPayment,
        guard: LockGuard,
    }

    impl Locked {
        async fn charge(self, deps: &PaymentDeps) -> Result<Charged, PayError> {
            let Locked {
                payment: ValidPayment { key, charge },
                guard,
            } = self;
            let mut delay = deps.retry.first_delay;

            // ponytail: no jitter, so clients retry in step. Add it for real traffic.
            for attempt in 1..=deps.retry.max_attempts {
                let call = deps.provider.charge(&charge);
                match timeout(deps.retry.per_attempt, call).await {
                    Ok(Ok(payment_id)) => {
                        return Ok(Charged {
                            key,
                            payment_id,
                            _guard: guard,
                        });
                    }
                    Ok(Err(ProviderError::Rejected(reason))) => {
                        return Err(PayError::Rejected(reason));
                    }
                    Ok(Err(ProviderError::Unavailable)) | Err(_) => {
                        if attempt < deps.retry.max_attempts {
                            sleep(delay).await;
                            delay *= 2;
                        }
                    }
                }
            }

            deps.store.record_failure(&key).await?;
            Err(PayError::Unavailable)
        }
    }

    // Stage 3. Money has moved, so losing track of this is the worst outcome.
    //
    // GAP. If the record fails, the guard still drops. The key frees while money has
    // moved with no record of it, so the lock does not cover this window. What covers
    // it is stable retry identity: a retry charges again with the same reference, and
    // a provider that honours references takes no more money. If yours doesn't, keep
    // the key held until a person has reconciled the payment.
    struct Charged {
        key: String,
        payment_id: String,
        _guard: LockGuard, // held until the record is written
    }

    impl Charged {
        async fn record(self, deps: &PaymentDeps) -> Result<String, PayError> {
            match deps.store.record_success(&self.key, &self.payment_id).await {
                Ok(()) => Ok(self.payment_id),
                Err(StoreDown) => Err(PayError::ChargedButNotRecorded {
                    payment_id: self.payment_id,
                }),
            }
        }
    }

    // fn(args, deps): reads as the happy path, and every `?` is a way off it.
    pub async fn create_payment(
        request: PaymentRequest,
        deps: &PaymentDeps,
    ) -> Result<String, PayError> {
        let payment = ValidPayment::try_from(request)?;

        if let Some(existing) = deps.store.find_by_key(&payment.key).await? {
            return Ok(existing);
        }

        let locked = payment.lock(deps).await?;
        let charged = locked.charge(deps).await?;
        charged.record(deps).await
    }
}

// ─────────────────────────────────────────────────────────────
// Hand-written fakes, as in chapter 2.
// ─────────────────────────────────────────────────────────────

#[derive(Default)]
struct FakeStore {
    existing: Option<String>,
    held_locks: Mutex<HashSet<String>>,
    down_when_recording: AtomicBool,
    recorded: Mutex<Vec<String>>,
    lock_released: Notify,
}

#[async_trait]
impl PaymentStore for FakeStore {
    async fn find_by_key(&self, _key: &str) -> Result<Option<String>, StoreDown> {
        Ok(self.existing.clone())
    }

    async fn acquire_lock(&self, key: &str) -> Result<bool, StoreDown> {
        Ok(self.held_locks.lock().await.insert(key.to_string()))
    }

    async fn release_lock(&self, key: &str) -> Result<(), StoreDown> {
        self.held_locks.lock().await.remove(key);
        self.lock_released.notify_one();
        Ok(())
    }

    async fn record_success(&self, _key: &str, payment_id: &str) -> Result<(), StoreDown> {
        if self.down_when_recording.load(Ordering::SeqCst) {
            return Err(StoreDown);
        }
        self.recorded.lock().await.push(payment_id.to_string());
        Ok(())
    }

    async fn record_failure(&self, key: &str) -> Result<(), StoreDown> {
        self.recorded.lock().await.push(format!("failed {key}"));
        Ok(())
    }
}

enum Reply {
    Accept,
    Reject,
    Down,
    Hang,
}

#[derive(Default)]
struct ScriptedProvider {
    replies: Mutex<VecDeque<Reply>>,
    calls: Mutex<Vec<ChargeRequest>>,
}

impl ScriptedProvider {
    fn replying(replies: impl IntoIterator<Item = Reply>) -> Arc<Self> {
        Arc::new(ScriptedProvider {
            replies: Mutex::new(replies.into_iter().collect()),
            calls: Mutex::default(),
        })
    }

    async fn references(&self) -> Vec<String> {
        let calls = self.calls.lock().await;
        calls.iter().map(|c| c.reference.clone()).collect()
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    async fn charge(&self, request: &ChargeRequest) -> Result<String, ProviderError> {
        self.calls.lock().await.push(request.clone());
        match self.replies.lock().await.pop_front() {
            Some(Reply::Accept) => Ok("pay_1".to_string()),
            Some(Reply::Reject) => Err(ProviderError::Rejected("card declined".to_string())),
            Some(Reply::Hang) => {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                Err(ProviderError::Unavailable)
            }
            Some(Reply::Down) | None => Err(ProviderError::Unavailable),
        }
    }
}

fn deps(store: &Arc<FakeStore>, provider: &Arc<ScriptedProvider>) -> PaymentDeps {
    PaymentDeps {
        store: store.clone(),
        provider: provider.clone(),
        retry: RetryPolicy {
            max_attempts: 3,
            first_delay: Duration::from_millis(200),
            per_attempt: Duration::from_secs(2),
        },
    }
}

fn request() -> PaymentRequest {
    PaymentRequest {
        client_id: "client_1".to_string(),
        amount_minor: 499,
        currency: "GBP".to_string(),
        idempotency_key: "key_1".to_string(),
    }
}

// ─────────────────────────────────────────────────────────────
// The stories. The `start_paused` clock means retries cost no real time.
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_valid_payment_is_charged_once_and_recorded() {
    let mut story = Story::new("A valid payment is charged once and recorded");
    story.given("a valid request with a new idempotency key");
    let store = Arc::new(FakeStore::default());
    let provider = ScriptedProvider::replying([Reply::Accept]);

    story.when("the payment is created");
    let outcome = create_payment(request(), &deps(&store, &provider)).await;
    let references = provider.references().await;
    let recorded = store.recorded.lock().await.clone();

    story.expect_step("the caller gets the payment id", || {
        assert_eq!(outcome, Ok("pay_1".to_string()));
    });
    story.expect_step("the provider was called once", || {
        assert_eq!(references, vec!["key_1".to_string()]);
    });
    story.expect_step("the payment was recorded", || {
        assert_eq!(recorded, vec!["pay_1".to_string()]);
    });
}

#[tokio::test]
async fn a_bad_request_never_reaches_the_provider() {
    let mut story = Story::new("A bad request never reaches the provider");
    story.given("a request for a negative amount");
    let store = Arc::new(FakeStore::default());
    let provider = ScriptedProvider::replying([Reply::Accept]);
    let bad = PaymentRequest {
        amount_minor: -1,
        ..request()
    };

    story.when("the payment is created");
    let outcome = create_payment(bad, &deps(&store, &provider)).await;
    let references = provider.references().await;

    story.expect_step("the caller is told which field is wrong", || {
        let error = PayError::Invalid {
            field: "amount_minor",
        };
        assert_eq!(error.http_status(), 400);
        assert_eq!(outcome, Err(error));
    });
    story.expect_step("the provider was not called", || {
        assert!(references.is_empty());
    });
}

#[tokio::test]
async fn a_repeated_request_returns_the_first_payment() {
    let mut story = Story::new("A repeated request returns the first payment");
    story.given("a payment that already exists for this idempotency key");
    let store = Arc::new(FakeStore {
        existing: Some("pay_0".to_string()),
        ..FakeStore::default()
    });
    let provider = ScriptedProvider::replying([Reply::Accept]);

    story.when("the client retries the same request");
    let outcome = create_payment(request(), &deps(&store, &provider)).await;
    let references = provider.references().await;

    story.expect_step("the caller gets the existing payment id", || {
        assert_eq!(outcome, Ok("pay_0".to_string()));
    });
    story.expect_step("nobody is charged again", || {
        assert!(references.is_empty());
    });
}

#[tokio::test]
async fn a_racing_request_is_turned_away() {
    let mut story = Story::new("A racing request is turned away");
    story.given("another request holds the lock for this key");
    let store = Arc::new(FakeStore {
        held_locks: Mutex::new(HashSet::from(["key_1".to_string()])),
        ..FakeStore::default()
    });
    let provider = ScriptedProvider::replying([Reply::Accept]);

    story.when("the payment is created");
    let outcome = create_payment(request(), &deps(&store, &provider)).await;
    let references = provider.references().await;

    story.expect_step("the caller gets a conflict", || {
        assert_eq!(outcome, Err(PayError::Conflict));
    });
    story.expect_step("the provider was not called", || {
        assert!(references.is_empty());
    });
}

#[tokio::test(start_paused = true)]
async fn a_rejected_card_is_not_retried() {
    let mut story = Story::new("A rejected card is not retried");
    story.given("a provider that rejects the card");
    let store = Arc::new(FakeStore::default());
    let provider = ScriptedProvider::replying([Reply::Reject, Reply::Accept]);

    story.when("the payment is created");
    let outcome = create_payment(request(), &deps(&store, &provider)).await;
    let references = provider.references().await;

    story.expect_step("the caller sees the rejection", || {
        let error = PayError::Rejected("card declined".to_string());
        assert_eq!(error.http_status(), 402);
        assert_eq!(outcome, Err(error));
    });
    story.expect_step("the provider was called once", || {
        assert_eq!(references.len(), 1);
    });
}

#[tokio::test(start_paused = true)]
async fn a_brief_outage_is_retried_without_double_charging() {
    let mut story = Story::new("A brief outage is retried without double charging");
    story.given("a provider that fails, then hangs, then recovers");
    let store = Arc::new(FakeStore::default());
    let provider = ScriptedProvider::replying([Reply::Down, Reply::Hang, Reply::Accept]);

    story.when("the payment is created");
    let outcome = create_payment(request(), &deps(&store, &provider)).await;
    let references = provider.references().await;
    story.state(Some("Provider calls"), json!({ "references": references }));

    story.expect_step("the payment succeeds on the third attempt", || {
        assert_eq!(outcome, Ok("pay_1".to_string()));
        assert_eq!(references.len(), 3);
    });
    story.expect_step("every attempt carries the same reference", || {
        assert!(references.iter().all(|reference| reference == "key_1"));
    });
}

#[tokio::test(start_paused = true)]
async fn a_long_outage_is_recorded_as_a_failure() {
    let mut story = Story::new("A long outage is recorded as a failure");
    story.given("a provider that stays down");
    let store = Arc::new(FakeStore::default());
    let provider = ScriptedProvider::replying([]);

    story.when("the payment is created");
    let outcome = create_payment(request(), &deps(&store, &provider)).await;
    let references = provider.references().await;
    let recorded = store.recorded.lock().await.clone();

    story.expect_step("the caller is told to try later", || {
        assert_eq!(PayError::Unavailable.http_status(), 503);
        assert_eq!(outcome, Err(PayError::Unavailable));
    });
    story.expect_step("it gave up after three attempts", || {
        assert_eq!(references.len(), 3);
    });
    story.expect_step("the failure was recorded", || {
        assert_eq!(recorded, vec!["failed key_1".to_string()]);
    });
}

#[tokio::test]
async fn money_taken_without_a_record_is_never_lost_silently() {
    let mut story = Story::new("Money taken without a record is never lost silently");
    story.given("a provider that accepts, and a store that then goes down");
    let store = Arc::new(FakeStore {
        down_when_recording: AtomicBool::new(true),
        ..FakeStore::default()
    });
    let provider = ScriptedProvider::replying([Reply::Accept]);

    story.when("the payment is created");
    let outcome = create_payment(request(), &deps(&store, &provider)).await;

    story.expect_step("the error carries the provider's payment id", || {
        assert_eq!(
            outcome,
            Err(PayError::ChargedButNotRecorded {
                payment_id: "pay_1".to_string()
            })
        );
    });
}

#[test]
fn charging_without_the_lock_does_not_compile() {
    crate::assert_rejected("ch07_charge_without_lock", "E0599");
}

// LockGuard releases from its own task, so wait until that has happened. A release
// can't be missed if it lands first: `notify_one` stores a permit, and `notified`
// consumes it at once. If no release comes, the fake clock ends the wait with false.
async fn lock_released(store: &FakeStore) -> bool {
    let released = store.lock_released.notified();
    timeout(Duration::from_secs(1), released).await.is_ok()
}

#[tokio::test(start_paused = true)]
async fn a_finished_payment_frees_its_key() {
    let mut story = Story::new("A finished payment frees its key");
    story.given("a provider that accepts the payment");
    let store = Arc::new(FakeStore::default());
    let provider = ScriptedProvider::replying([Reply::Accept]);

    story.when("the payment is created");
    let outcome = create_payment(request(), &deps(&store, &provider)).await;
    let released = lock_released(&store).await;
    let still_held = store.held_locks.lock().await.len();

    story.expect_step("the payment succeeded", || assert!(outcome.is_ok()));
    story.expect_step("no lock is left behind", || {
        assert!(released);
        assert_eq!(still_held, 0);
    });
}

#[tokio::test(start_paused = true)]
async fn a_failed_payment_can_be_tried_again() {
    let mut story = Story::new("A failed payment can be tried again");
    story.given("a provider that rejects the card, then accepts another");
    let store = Arc::new(FakeStore::default());
    let provider = ScriptedProvider::replying([Reply::Reject, Reply::Accept]);
    let deps = deps(&store, &provider);

    story.when("the client retries with the same key");
    let first = create_payment(request(), &deps).await;
    let released = lock_released(&store).await;
    let second = create_payment(request(), &deps).await;

    story.expect_step("the first attempt failed", || assert!(first.is_err()));
    story.expect_step("the key was free for the second", || {
        assert!(released);
        assert_eq!(second, Ok("pay_1".to_string()));
    });
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_request_can_be_tried_again() {
    let mut story = Story::new("A cancelled request can be tried again");
    story.given("a provider that hangs, then recovers");
    let store = Arc::new(FakeStore::default());
    let provider = ScriptedProvider::replying([Reply::Hang, Reply::Accept]);
    let deps = deps(&store, &provider);

    story.when("the request is cancelled during the provider call, then retried");
    let work = create_payment(request(), &deps);
    let cancelled = timeout(Duration::from_secs(1), work).await;
    let released = lock_released(&store).await;
    let retried = create_payment(request(), &deps).await;

    story.expect_step("the first request never finished", || {
        assert!(cancelled.is_err());
    });
    story.expect_step("the key was free for the retry", || {
        assert!(released);
        assert_eq!(retried, Ok("pay_1".to_string()));
    });
}

#[tokio::test(start_paused = true)]
async fn a_retry_after_a_lost_record_reuses_the_reference() {
    let mut story = Story::new("A retry after a lost record reuses the reference");
    story.given("a charge that worked, and a store that failed to record it");
    let store = Arc::new(FakeStore {
        down_when_recording: AtomicBool::new(true),
        ..FakeStore::default()
    });
    let provider = ScriptedProvider::replying([Reply::Accept, Reply::Accept]);
    let deps = deps(&store, &provider);
    let first = create_payment(request(), &deps).await;
    let released = lock_released(&store).await;

    story.when("the store recovers and the client retries");
    store.down_when_recording.store(false, Ordering::SeqCst);
    let second = create_payment(request(), &deps).await;
    let references = provider.references().await;

    story.expect_step("the first attempt reported the lost record", || {
        let payment_id = "pay_1".to_string();
        assert_eq!(first, Err(PayError::ChargedButNotRecorded { payment_id }));
    });
    story.note("The lock does not protect this window. The key was already free.");
    story.expect_step("the provider was asked again", || {
        assert!(released);
        assert_eq!(second, Ok("pay_1".to_string()));
    });
    story.expect_step("with the same reference, so it can spot the repeat", || {
        assert_eq!(references, vec!["key_1".to_string(), "key_1".to_string()]);
    });
}

// ─────────────────────────────────────────────────────────────
// Exercise: `cargo test ch07 -- --ignored`
//
// LockGuard covers every exit except the process dying, which skips Drop too.
//   1. Add an expiry to the fake store's lock, and use `std::mem::forget` on a
//      guard to stand in for a crash.
//   2. Prove on the fake clock that the key frees itself after the expiry.
//   3. Now the hard part. A slow charge can outlive its lock, so a second request
//      gets in. What stops the double charge? Write the story that proves it.
// ─────────────────────────────────────────────────────────────

#[test]
#[ignore = "exercise: done when stories show a crashed lock expiring and a slow charge that outlives its lock"]
fn exercise_survive_a_crash() {
    todo!("replace this with your story")
}
