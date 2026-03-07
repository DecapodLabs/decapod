# CONCURRENCY.md - Concurrency & Parallelism Architecture

**Authority:** guidance (concurrency patterns, async discipline, and coordination models)
**Layer:** Guides
**Binding:** No
**Scope:** concurrency models, async patterns, background task discipline
**Non-goals:** language-specific runtime details, OS-level threading

---

## 1. The Oracle's Verdict: Concurrency as a High-Stakes Game

*Concurrency is the source of the most subtle, non-deterministic, and expensive bugs in software. If you can solve a problem sequentially, do it.*

### 1.1 The CTO's Strategic View
- **Simplicity over Scale:** Don't reach for microservices or complex actor models until you've exhausted the limits of a single-threaded or simple multi-threaded monolith. Complexity is a tax on every future developer.
- **The "Cost of Coordination":** Coordination is the bottleneck of scaling. Amdahl's Law is a business law. If 10% of your task is sequential, you can't speed it up more than 10x, no matter how many cores you add.

### 1.2 The SVP's Operational View
- **Blast Radius:** A concurrency bug in one component can bring down the entire system (e.g., a deadlock that starves the thread pool). Isolate concurrent workloads.
- **Backpressure as a First-Class Citizen:** If a system cannot say "no" when it's overloaded, it's not production-ready. Every concurrent queue must be bounded.

### 1.3 The Architect's Structural View
- **Immutability is the Cure:** The easiest way to solve concurrency issues is to eliminate shared mutable state. Use persistent data structures and message passing.
- **State Machines for Coordination:** Complex concurrent workflows must be modeled as explicit state machines. "Ad-hoc" coordination with boolean flags is a recipe for disaster.

### 1.4 The Principal's Execution View
- **The "Lock-Free" Trap:** Unless you are building a low-level primitive (like a queue or a memory allocator), stay away from lock-free programming. It is incredibly difficult to get right and even harder to test.
- **Async is not "Free":** Async runtimes have overhead. For CPU-bound tasks, use a dedicated thread pool. For I/O-bound tasks, use async, but watch your stack sizes and allocation patterns.
- **Deterministic Simulation:** For critical concurrent logic, use deterministic simulation testing (like Loom or FoundationDB's simulator) to find edge cases that traditional testing will miss.

---

## 2. Concurrency Models

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

## 5. Anti-Patterns

| Anti-Pattern | Why It's Dangerous | Alternative |
|---|---|---|
| **Locks held across async** | Deadlocks, contention | Short-lived lock scopes |
| **Unbounded channels** | Memory leak under load | Bounded channels with backpressure |
| **Silent spawn failures** | Invisible bugs, lost work | Log all errors from spawned tasks |
| **No timeouts on I/O** | Hung tasks, resource exhaustion | Timeout every external call |
| **Shared mutable state** | Race conditions | Message passing or lock discipline |
| **Thread-per-request** | Resource exhaustion at scale | Thread pools with bounded concurrency |

---

## 6. Coordination Patterns

### 6.1 Fan-Out / Fan-In
Distribute work across workers, collect results. Use bounded concurrency to prevent resource exhaustion.

### 6.2 Pipeline
Chain processing stages with channels between them. Each stage runs independently. Backpressure propagates naturally through bounded channels.

### 6.3 Circuit Breaker
When an external service fails repeatedly, stop calling it temporarily. Prevents cascade failures and gives the service time to recover.

---

## Links

- `methodology/ARCHITECTURE.md` - binding architecture
- `architecture/ALGORITHMS.md` - Algorithm selection
- `architecture/CLOUD.md` - Cloud infrastructure patterns
- `architecture/OBSERVABILITY.md` - Monitoring and debugging
