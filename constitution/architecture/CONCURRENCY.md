# CONCURRENCY.md - Concurrency & Parallelism Architecture

**Authority:** guidance (concurrency patterns, async discipline, and coordination models)
**Layer:** Guides
**Binding:** No
**Scope:** concurrency models, async patterns, background task discipline
**Non-goals:** language-specific runtime details, OS-level threading

---

## 1. Concurrency Models

### 1.1 Shared Memory vs Message Passing

| Model | Pros | Cons | Use When |
|-------|------|------|----------|
| **Shared memory** | Fast, low overhead | Race conditions, deadlocks | Hot paths, read-heavy workloads |
| **Message passing** | Safe, composable | Overhead, channel complexity | Distributed state, coordination |
| **Actor model** | Isolated state, fault tolerant | Complexity, debugging difficulty | Distributed systems, agent loops |
| **CSP (channels)** | Explicit coordination | Channel management | Pipeline processing, fan-out/fan-in |

### 1.2 Threads vs Async

**Threads:** Use for CPU-bound work, blocking I/O, or when simplicity matters more than scale.

**Async:** Use for I/O-bound work with many concurrent connections. Understand the cost: async runtimes add complexity, stack traces become harder to read, and cancellation semantics require care.

### 1.3 Production Mindset
Concurrency is one of the highest-leverage and highest-risk categories of engineering decisions:

- **Sequential first:** Do not reach for concurrent architectures until the sequential baseline is exhausted. The simplest correct program is single-threaded. Concurrency is justified by measured need, not anticipated scale.
- **Coordination is the bottleneck:** Amdahl's Law is a hard limit. If 10% of a workload is sequential, no amount of parallelism yields more than 10× improvement. Design to minimize the sequential fraction, and be explicit about where it lives.
- **Blast radius isolation:** A concurrency bug — deadlock, live-lock, data race — can bring down an entire process or starve a thread pool. Isolate concurrent subsystems behind clear boundaries so failures cannot cascade.
- **Backpressure is a correctness property:** A system that cannot say "no" when overloaded is not production-ready. Every concurrent queue must be bounded. Unbounded queues are memory leaks with a delayed fuse.
- **Immutability eliminates the problem class:** Shared mutable state is the root cause of most concurrency bugs. Prefer immutable data, message passing, and copy-on-write semantics. When mutable state is unavoidable, make lock discipline explicit and reviewed.
- **Explicit state machines over ad-hoc coordination:** Complex concurrent workflows modeled with boolean flags and informal protocols will contain bugs that cannot be reproduced or proven correct. Model them as explicit state machines with defined transitions.
- **Lock-free is not "free":** Lock-free data structures are expert territory. Unless implementing a low-level primitive where profiling justifies it, lock-free code introduces correctness hazards that testing rarely catches. Use well-tested library implementations.
- **Async is not free either:** Async runtimes have scheduling overhead. For CPU-bound work, async adds overhead without benefit; use dedicated thread pools. Watch stack sizes, allocation rates, and wake-up patterns under load.

---

## 2. Async Discipline

### 2.1 Lock Hygiene

**Never hold locks across await points.** Acquire the lock, read or write the value, drop the lock, then perform async I/O.

```
// WRONG: lock held across await
let guard = mutex.lock().await;
let result = do_network_call(&guard.value).await;  // lock held during I/O
drop(guard);

// RIGHT: short-lived lock scope
let value = {
    let guard = mutex.lock().await;
    guard.value.clone()
};  // lock dropped here
let result = do_network_call(&value).await;
```

### 2.2 Cancellation Safety

Async tasks can be cancelled at any await point. Design for this:
- Use `CancellationToken` or `select!` for cooperative cancellation
- Ensure cleanup runs even on cancellation (use `Drop` or scope guards)
- Document cancellation semantics for public async APIs

### 2.3 Timeouts

Every external call (network, disk, subprocess) must have a timeout. Unbounded waits are bugs.

---

## 3. Background Task Discipline

### 3.1 Error Handling

Every spawned background task must handle errors. Fire-and-forget without error logging is forbidden.

```
// WRONG: silent failure
spawn(async move { do_work().await; });

// RIGHT: errors are logged
spawn(async move {
    if let Err(e) = do_work().await {
        tracing::error!(error = %e, "Background task failed");
    }
});
```

### 3.2 Bounded Channels

No unbounded channels. Use bounded `mpsc` with backpressure. Unbounded channels are memory leaks waiting to happen under load.

### 3.3 Task Lifecycle

- Every spawned task should be cancellable
- Track active tasks for graceful shutdown
- Log task start and completion at debug level
- Log task failure at error level

---

## 4. Dependency Bundle Pattern

As systems grow, function signatures accumulate parameters. Bundle shared dependencies into structs:

```
// WRONG: parameter proliferation
fn validate(store: &Store, broker: &Broker, config: &Config, root: &Path) -> Result<()>

// RIGHT: dependency bundle
struct ValidateContext {
    store: Store,
    broker: Broker,
    config: Config,
    root: PathBuf,
}
fn validate(ctx: &ValidateContext) -> Result<()>
```

Rules:
- Optional fields for graceful degradation (e.g., `user_store: Option<Store>`)
- Bundles are passed by reference, not consumed
- Keep bundles focused — one per domain, not a god struct

---

## 5. Coordination Patterns

### 5.1 Fan-Out / Fan-In
Distribute work across workers, collect results. Use bounded concurrency to prevent resource exhaustion.

### 5.2 Pipeline
Chain processing stages with channels between them. Each stage runs independently. Backpressure propagates naturally through bounded channels.

### 5.3 Circuit Breaker
When an external service fails repeatedly, stop calling it temporarily. Prevents cascade failures and gives the service time to recover.

---

## 6. Anti-Patterns

| Anti-Pattern | Why It's Dangerous | Alternative |
|---|---|---|
| **Locks held across async** | Deadlocks, contention | Short-lived lock scopes |
| **Unbounded channels** | Memory leak under load | Bounded channels with backpressure |
| **Silent spawn failures** | Invisible bugs, lost work | Log all errors from spawned tasks |
| **No timeouts on I/O** | Hung tasks, resource exhaustion | Timeout every external call |
| **Shared mutable state** | Race conditions | Message passing or lock discipline |
| **Thread-per-request** | Resource exhaustion at scale | Thread pools with bounded concurrency |

---

## Links

- `methodology/ARCHITECTURE.md` - binding architecture
- `architecture/ALGORITHMS.md` - Algorithm selection
- `architecture/CLOUD.md` - Cloud infrastructure patterns
- `architecture/OBSERVABILITY.md` - Monitoring and debugging
