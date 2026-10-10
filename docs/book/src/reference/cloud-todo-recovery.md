# Resuming cloud todo creation

Cloud `todo add` assigns a distinct operation token before authentication. A
fresh invocation means new work, even when its title and other inputs are
identical to an earlier task. Never issue a fresh add to retry an uncertain
operation.

When onboarding is pending or a response is lost, retain `operation_id` from the
JSON diagnostic. After fixing the reported prerequisite, repeat the original
arguments and add `--operation-id TOKEN`, for example:

```text
decapod todo add "Implement a feature" --priority high --operation-id TOKEN --format json
```

`TOKEN` is the exact value returned by that operation, not a title, task ID, or
credential. Repeat all original options. A restarted agent may resume the request; the
original creation event keeps its original audit actor. The
client refuses a token for changed arguments, repository, or service endpoint
before contacting the service. To change an unsubmitted request, intentionally
start a new operation; do not mutate the identity of the old one. Recovery text
does not interpolate task text into a shell command.

After an interrupted process, `decapod todo operations --format json` lists the
machine's retained receipt metadata. Match the original operation and replay its
original arguments with its token. This inspection never opens the repository's
local task database. It is metadata inspection, not authentication or a task
store: task descriptions and other request contents are not retained there.

## Outcomes

- `blocked_before_submit`: this attempt did not dispatch the mutation. Its
  failure category distinguishes authentication, onboarding pending,
  authorization, offline, transport, conflict, cancellation, and validation
- `outcome_unknown`: a dispatched mutation may have committed, or an interrupted
  attempt has not been reconciled. Never interpret it as rollback
- `succeeded`: the client observed the mutation result or an exact immutable
  creation receipt through the authorized service boundary

`attempt_status` records whether this individual attempt dispatched. Once an
operation is uncertain, a blocked retry cannot downgrade its overall `status`;
only an exact authorized receipt can establish success.

The failure JSON schema is `decapod.cloud.todo-operation.v1`. It contains only
operation identity, status, failure category, retryability, and a next action.
Task data, bearer tokens, refresh tokens, endpoints, and arbitrary service error
messages are excluded. Authentication/authorization failures remain distinct;
a retry token never provides repository access. The service continues to own
authenticated principal identity and authorization; Decapod neither parses
unverified bearer claims nor treats a local successful receipt as access.

## Duplicate protection and receipts

The existing Dactyl atomic batch inserts the task, its creation event, and reads
the task. A resumed operation reuses its task ID and deterministic event ID.
The creation event binds an immutable request fingerprint. Retries inspect that
event through the same authenticated Dactyl query boundary; subsequent changes
to a task's title, status, or assignee do not invalidate its creation receipt.
An ID collision or a changed request cannot overwrite an existing task. A
concurrent loser or lost acknowledgement can reconcile the winner's exact
receipt. Authentication or authorization errors are never bypassed by a second
reconciliation request.

Unknown outcomes stay unknown if the confirming read is unavailable. The client
never substitutes local SQLite, generates a new identity for a retry, or infers
rollback from an HTTP error. A successful local receipt still requires normal
authentication and authorized remote reconciliation when explicitly retried.

## Local metadata and retention

Non-secret receipt metadata lives under the machine configuration directory in
`decapod/todo-operations`. It stores token, fingerprint, task ID, outcome,
failure category, and update time. It contains no task payload or credentials,
and has no repository-local fallback. Private creation permissions, bounded
exclusive locking, and atomic replacement protect concurrent writers.

The journal is limited to 256 records and 1 MiB. Successful receipts older than
seven days are removed when preparing new work; the oldest successful receipt
may also be removed at capacity because the durable service event remains the
authority. Unresolved records are never silently evicted. When all slots are
unresolved, new operations fail before submission and ask for reconciliation.
An existing operation can still be retried at capacity.

## Proof boundary

Local SQLite adapter tests prove the client uses a task-plus-event atomic
transaction, handles transaction failure without partial state, and reconciles
an immutable creation event after task changes. Synthetic public Dactyl HTTP
fixtures exercise response loss, interrupted reconciliation, and denied receipt
reads. These are client/adapter proofs, not deployed service authorization or
hosted transaction-isolation proofs. Hosted parity and service atomicity remain
separate downstream acceptance work (#1377 and #1378).
