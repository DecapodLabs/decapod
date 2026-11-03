# Non-destructive recovery acceptance

Issue #1473 requires a successful real CLI recovery, not only a passing injected
`ValidationSnapshot`. The opt-in test
`live_recovery::live_recovery_preserves_multiple_historical_completions` in
`tests/verify_deadlock_regression.rs` runs the candidate executable against an
explicitly supplied governed fixture. An ignored test is **not acceptance**.

## Agent-owned fixture and execution

The agent/harness, not the human, prepares and runs the fixture. The human only
runs `decapod init` and supplies intent or required approval. Use an isolated
fixture already authorized for full validation, including any configured branch
push. It needs an independently accepted target, reviewed proof contract and
real execution access. Never use production tasks merely to make a test pass.

The fixture must contain at least two historical `done` tasks with original
completion events and timestamps and absent verification artifacts. Every done
task must be explicitly included. Supply current, relevant task-bound files;
an unrelated sentinel file is not evidence of the task's work. The harness does
not delete existing proof, alter task state, approve governance, prune, reopen,
install credentials, or manufacture the historical fixture. Preserve a private
backup through the supported maintenance surface before live work.

The agent sets:

- `DECAPOD_RECOVERY_FIXTURE`: the absolute governed worktree path.
- `DECAPOD_RECOVERY_ARTIFACTS`: a JSON object such as
  `{"bugs_first":["src/first.rs"],"bugs_second":["src/second.rs"]}`.
- The fixture's established agent/session/task context, rather than an unrelated
  outer source-test task. No credential is recorded in the test or its output.

Through the configured validation runner, select the ignored integration test:

```text
--test verify_deadlock_regression live_recovery::live_recovery_preserves_multiple_historical_completions -- --ignored --exact
```

Do not run a shadow build outside the repository's configured validation flow.
The ordinary integration target compiles this harness and runs its environment
isolation assertion, but does not provision or mutate a live fixture.

## Required evidence

The test observes original task fields through `todo get` and uses the datastore's
read-only validation connection to compare complete historical event fields. It
requires the initial `PROOF_HOOK_FAILED`, then invokes real
`qa verify recover --artifact TODO_ID=FILE` for the whole batch, replays each task,
and runs ordinary unscoped `validate`. The final report must have
`publication_ready`, `ready` and `proof_ready` true. Inherited CI-local and
task-exemption flags are removed. No mocked provider, injected snapshot, local
Git transport replacement, fake executable or imported receipt is accepted.

All task fields, including completion timestamps, revision, custody and comments,
must remain identical; every original event must remain unchanged. New events
may only record verification and its journal checkpoint, never prune or another
completion lifecycle transition. Preservation is checked even after failed
recovery. The existing failure-path CLI test and unit tests for failed snapshots,
changed artifacts and concurrent task revisions remain independent and intact.

Record the exact candidate, fixture target, real validation report and complete
test result before calling the issue solved. A missing fixture, inaccessible
provider, unsupported controller or unsuccessful validation is a blocker, not a
skip that qualifies as a successful recovery. Current source CI cannot establish
this live acceptance unless the authorized fixture and real proof access are
explicitly provided to that test run.
