# OBSERVABILITY.md - Observability Architecture

**Authority:** guidance (observability patterns, structured logging, and audit discipline)
**Layer:** Guides
**Binding:** No
**Scope:** logging, metrics, tracing, event sourcing, mechanical verification
**Non-goals:** specific monitoring tool configuration, alerting thresholds

---

## 1. Observability Principles

### 1.1 The Three Pillars

| Pillar | Purpose | Use For |
|--------|---------|---------|
| **Metrics** | Aggregate numerical data | Dashboards, alerting, capacity planning |
| **Logs** | Discrete events with context | Debugging, audit trails, forensics |
| **Traces** | Request flow across services | Understanding latency, dependencies |

### 1.2 Core Mandates

- **Structured logging is required; string parsing is prohibited.** Every log entry must be machine-parseable (JSON, key-value pairs, or structured format).
- **Alert on symptoms, not causes.** Users experience symptoms (latency, errors); investigate causes after alerting.
- **Sampling is acceptable for high-volume data.** 100% capture at low volume, statistical sampling at high volume.
- **Cost of observability < cost of not observing.** If you can't see it, you can't fix it.

---

## 2. Structured Logging

### 2.1 Requirements

Every log entry must include:
- **Timestamp** (UTC, ISO8601)
- **Level** (error, warn, info, debug, trace)
- **Message** (human-readable summary)
- **Structured fields** (machine-parseable context)

### 2.2 Anti-Patterns

```
// WRONG: unstructured string
log!("User {} failed to login after {} attempts", user_id, count);

// RIGHT: structured fields
info!(user_id = %user_id, attempts = count, "Login failed");
```

### 2.3 What NOT to Log

- Secrets, tokens, passwords, API keys
- Full request/response bodies in production (use trace level)
- PII without explicit consent and retention policy

---

## 3. Event Sourcing for Audit

### 3.1 The Broker Pattern

All state-mutating operations should go through an event broker that:
1. **Records the event** before applying the mutation
2. **Includes actor identity** (who initiated the change)
3. **Includes intent reference** (why the change was made)
4. **Supports replay** (events can rebuild state deterministically)

### 3.2 Event Log Discipline

- Events are append-only. Never edit or delete events.
- Events have a stable schema. New fields are additive; old fields are never removed.
- Event logs are bounded. Cap at a reasonable limit and archive older events.
- Every event includes: `event_id`, `timestamp`, `actor`, `operation`, `status`.

### 3.3 Deterministic Replay

The gold standard for event sourcing: replaying all events from an empty state must produce identical results to the current state. This is a testable invariant.

---

## 4. Transition History on State Machines

Every state machine (task lifecycle, claim status, policy approval) should maintain a transition history:

```
{
    "from": "pending",
    "to": "active",
    "timestamp": "2026-02-14T10:30:00Z",
    "actor": "agent-claude",
    "reason": "Starting implementation of feature X"
}
```

Rules:
- Every transition is recorded, including reverts
- Reason field is mandatory (not just "state changed")
- History is bounded (cap at 200 entries, archive older)
- History is queryable (find all transitions for a given entity)

---

## 5. Mechanical Verification

### 5.1 Grep-Based Checks

Automated checks that don't require human judgment:

```bash
# No panics in production code
grep -rnE '\.unwrap\(|\.expect\(' src/ --include='*.rs'

# No secrets in source
grep -rnE '(sk-|AKIA|ghp_|password\s*=)' src/ --include='*.rs'

# All state enums have transition tables
grep -rn 'can_transition_to' src/ --include='*.rs'
```

### 5.2 Validation as Observability

The validation harness (`decapod validate`) is itself an observability tool. It makes invisible invariants visible:
- Store integrity (deterministic rebuild from events)
- Health purity (no manual status values)
- Namespace hygiene (no legacy references)
- Schema determinism (stable output across runs)

### 5.3 Continuous Verification

Run mechanical checks in CI, not just locally. Every merge must pass:
- Compilation (no broken references)
- Clippy (no warnings)
- Tests (all pass)
- Validation harness (all gates pass)

---

## 6. Metrics Patterns

### 6.1 USE Method (for resources)
- **Utilization**: How busy is the resource?
- **Saturation**: How much work is queued?
- **Errors**: How many errors occurred?

### 6.2 RED Method (for services)
- **Rate**: Requests per second
- **Errors**: Error rate
- **Duration**: Latency distribution

### 6.3 Four Golden Signals
- **Latency**: Time to serve a request
- **Traffic**: Demand on the system
- **Errors**: Rate of failed requests
- **Saturation**: How full the system is

---

## 7. Anti-Patterns

| Anti-Pattern | Why It's Dangerous | Alternative |
|---|---|---|
| **Unstructured logs** | Can't query, can't alert | Structured logging with typed fields |
| **Logging secrets** | Security breach | Redact or use SecretString wrappers |
| **No event sourcing** | Can't audit, can't replay | Broker pattern for all mutations |
| **Manual health values** | Drift from reality | Derive health from proof events |
| **Alert fatigue** | Real alerts ignored | Alert on symptoms, tune thresholds |
| **No transition history** | Can't debug state issues | Record every state transition |

---

## Links

- `methodology/ARCHITECTURE.md` - CTO-level architecture
- `architecture/SECURITY.md` - Security patterns
- `architecture/CONCURRENCY.md` - Concurrency patterns
- `specs/SYSTEM.md` - System definition
