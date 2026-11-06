---
name: rust-payments
description: Builds Rust workflows that must not lose or repeat a side effect, using idempotency keys, a lock guard that survives cancellation, typestate stages and stable retry references. Use when writing Rust for payments, charges, refunds, payouts, or any external call that clients retry and that must happen once.
---

# Rust payments

You can't lose a penny and you can't charge twice, while clients retry, providers time out and requests get cancelled halfway.

## The workflow

Each stage is a type that consumes the last, private to one module, so outside code can only call the entry point. The provider call lives on `Locked`, so charging without the lock can't compile.

```rust
#[async_trait]
pub trait PaymentStore: Send + Sync {
    async fn find_by_key(&self, key: &str) -> Result<Option<String>, StoreDown>;
    async fn acquire_lock(&self, key: &str) -> Result<Option<LockToken>, StoreDown>;
    // Frees the lock only if `token` still holds it, as one atomic step.
    async fn release_lock(&self, key: &str, token: LockToken) -> Result<(), StoreDown>;
    async fn record_success(&self, key: &str, payment_id: &str) -> Result<(), StoreDown>;
    async fn record_failure(&self, key: &str) -> Result<(), StoreDown>;
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn charge(&self, request: &ChargeRequest) -> Result<String, ProviderError>;
}

pub struct PaymentDeps {
    pub store: Arc<dyn PaymentStore>,
    pub provider: Arc<dyn Provider>,
    pub retry: RetryPolicy, // max_attempts, first_delay, per_attempt
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct LockToken(pub u64); // proof of who holds a lock

struct LockGuard { store: Arc<dyn PaymentStore>, key: String, token: LockToken }
struct ValidPayment { key: String, charge: ChargeRequest } // TryFrom<PaymentRequest>
struct Locked { payment: ValidPayment, guard: LockGuard }
struct Charged { key: String, payment_id: String, _guard: LockGuard }
```

| Step | Signature | What it does |
|---|---|---|
| `lock` | `ValidPayment -> Result<Locked, PayError>` | `acquire_lock` and keep its token in the guard, or `Conflict` if another request holds it |
| `charge` | `Locked -> Result<Charged, PayError>` | Calls the provider in a loop, with a timeout per attempt, doubling the delay, and the same `ChargeRequest` each time. Returns at once on a rejection. Calls `record_failure` when attempts run out |
| `record` | `Charged -> Result<String, PayError>` | `record_success`, or `ChargedButNotRecorded` if that fails |

```rust
pub async fn create_payment(
    request: PaymentRequest,
    deps: &PaymentDeps,
) -> Result<String, PayError> {
    let payment = ValidPayment::try_from(request)?;

    if let Some(existing) = deps.store.find_by_key(&payment.key).await? {
        return Ok(existing);
    }

    let locked = payment.lock(deps).await?;

    // A racing request can finish between the lookup above and this lock.
    if let Some(existing) = deps.store.find_by_key(&locked.payment.key).await? {
        return Ok(existing);
    }

    let charged = locked.charge(deps).await?;
    charged.record(deps).await
}
```

## Name each failure

| Failure | Variant | Response |
|---|---|---|
| The caller made a mistake | `Invalid { field }` | 400, name the field |
| A retry raced the first request | `Conflict` | 409, try later |
| The provider said no | `Rejected(reason)` | 402, don't retry |
| The provider is down | `Unavailable` | Retry, record the failure, then 503 |
| Your database is down | `StoreDown` | 500, page someone |
| Money moved with no record | `ChargedButNotRecorded { payment_id }` | 500, page someone now |
| Your code has a bug | a panic | Fix the code |

`ChargedButNotRecorded` carries the provider's id, so a person can find the money.

## Three layers stop a double charge

| Layer | Stops | How |
|---|---|---|
| Idempotency key | A repeat after the first finished | `find_by_key` returns the stored payment, before the lock and again while holding it |
| Lock | A repeat while the first runs | `acquire_lock` turns the second away with `Conflict` |
| Stable reference | A timed-out attempt that took the money | Each attempt sends the same reference, set before the first |

## A lock that survives cancellation

A `release_lock().await` at the bottom gets skipped on cancellation. `Drop` always runs but can't wait, so it hands the work to a task:

```rust
impl Drop for LockGuard {
    fn drop(&mut self) {
        let store = self.store.clone();
        let key = std::mem::take(&mut self.key);
        let token = self.token;
        let Ok(runtime) = Handle::try_current() else {
            return; // shutting down, so the expiry frees it
        };
        runtime.spawn(async move {
            if store.release_lock(&key, token).await.is_err() {
                eprintln!("lock {key} not released, waiting for it to expire");
            }
        });
    }
}
```

Hold the guard in each stage until the record is written, so a lookup under the lock can trust its answer. The spawned release is best effort, so pair it with a lock expiry.

An expiry means the lock can pass to another request while the old guard still exists. The token stops that guard's late cleanup from freeing a lock it no longer holds. A real store checks the token and deletes in one atomic step, such as `DELETE ... WHERE key = ? AND token = ?`.

## Tests to write

Use hand-written fakes and a paused clock. Cover: a valid payment charges once, a bad request never reaches the provider, a repeat returns the first payment, a racing request gets `Conflict`, a request that takes the lock after a racer finished returns that payment with no charge, a rejection isn't retried, a brief outage charges once, a long outage records a failure, a cancelled request can be tried again, a late cleanup leaves the next request's lock alone, and a lost record reuses the reference.

## GAP

- A process crash skips `Drop`, and a runtime shutdown drops the spawned release, so a real lock also needs an expiry.
- A slow charge can outlive that expiry, so the expiry must exceed the longest charge, or the provider's reference has to protect you.
- The release lands a moment after the request ends, so an instant retry can see `Conflict`. That's safe.
- A charge with no record frees its key. A retry reaches the provider again, and only the stable reference prevents a second charge. If your provider ignores references, hold the key until a person reconciles the payment.

Optional background, with the tests behind these rules: https://github.com/jagreehal/rustacean-lessons/blob/main/guide/07-payment.md
