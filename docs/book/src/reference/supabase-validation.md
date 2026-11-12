# Supabase consumer validation

Ordinary project users select only `repo.backend = "local" | "cloud"`.
The managed cloud flow goes through Dactyl public discovery, sign-in, account
completion, and native data transport. Setup does not require a deployment
URL, physical driver selector, copied credential, or project-level provider
configuration. See [Cloud sign-in and task access](propodus.md) for the human
flow, headless continuation, private custody, and explicit recovery.

## Package and implementation prerequisites

The managed connector requires the public API delivered by
[Dactyl #100](https://github.com/DecapodLabs/dactyl/issues/100). The existing
`dactyl-db` 0.11.1 registry release provides the earlier explicit Supabase
adapter, not this connector API. The current required repository checks need
a published dependency containing the connector API. No governed source-overlay
recipe is configured for integration diagnostics before those checks pass; no
such diagnostic result is claimed. The release handoff remains
[#1380](https://github.com/DecapodLabs/decapod/issues/1380).
Do not replace a missing registry artifact with a committed Git/path override
or remove the release dependency checks.

Standard builds enable `supabase-cloud`; a build without that capability
fails closed when the managed cloud backend is selected. Local SQLite remains
independent. The logical cloud factory delegates its physical route to Dactyl.
Historical Neon and Supabase adapters remain explicit library and conformance
surfaces. Their legacy environment configuration does not select the ordinary
managed CLI route or supply its credentials.

The managed CLI fixture origin is available only with explicit
`DECAPOD_TEST_MODE=1` and `DECAPOD_CLOUD_TEST_ORIGIN` set to a literal-loopback
HTTP origin. It is a disposable test seam, never project policy or a hosted
operator configuration channel. Tests isolate HOME and both XDG directories;
synthetic credentials are written through the private custody API.

## Proof ownership

`tests/cloud_managed_session.rs` drives the actual binary through discovery,
bootstrap, exchanges, refresh, account completion, and an authorized task read.
It covers resumable headless invocation, exact binding, serialization, lost
responses, explicit denial, unresolved entitlement, and secret redaction.
Cloud command failures cannot initialize SQLite; unsupported repository
datastore commands fail before local startup. File-only control surfaces and
local-backend behavior remain separate.

The service owns schema, grants, membership, payment/seat readiness, and
authorization. Dactyl carries validated transport; Decapod carries the target
repository and opaque current bearer. A printed handoff, issued credential,
health response, or payment alone does not establish repository access.
Initialization checks service completion and an existing authenticated,
read-only task statement. Every later data request is authorized again.

The legacy production-command profiles below are currently unavailable with
the managed CLI. Explicitly selecting them fails with
`MANAGED_CLI_FIXTURE_UNAVAILABLE` before fixture reads, CLI execution or network
access; their endpoint/token environment variables cannot select the managed
origin or establish private custody. The retained fixtures, commands and
assertions describe the real-service acceptance contract that a separately
reviewed managed fixture must restore, not a runnable or completed proof. The
explicit `PropodusClient` test in `propodus_live` still targets its declared
legacy endpoint and does not establish managed CLI behavior. Native RPC and
managed-account hosted acceptance remain independent requirements; loopback
fixtures and the expected preflight failure do not satisfy them.

## Ownership and permissions

Git continues to hold governance plans, claims, trajectory, validation, specs,
and proof artifacts. Supported shared operational tasks and matching events
use the configured backend. There is no SQLite-file upload or synchronization.

The authenticated service resolves the principal, organization, group/team
membership, repository identity and effective permission. Client-supplied
scope is a request target, never a grant. Decapod exercises public allowed and
denied outcomes instead of copying the service's authorization rules.

The conformance profile covers the reviewed todo task/event schema. It does
not establish arbitrary Decapod catalog or schema portability. A compatible
service implements the current reviewed task SQL and ordered
three-operation batches: task mutation, matching event, final task read. It
must commit both state and event or neither. It must gate event insertion on
the preceding mutation affecting one row; a matching timestamp by itself is
insufficient. Concurrent claims have one winner. Stale or missing transitions
return a conflict/zero count and cannot append success events. A response lost
after commit is ambiguous: observe task state before deciding whether to
retry. This client contract does not add or claim mutation idempotency.

## Separate evidence profiles

- Default SQLite/Neon regression and cloud boundary tests need no hosted
  credentials. Local HTTP fixtures establish client composition and error
  handling only. They are not PostgreSQL, RLS, or hosted Supabase evidence.
- `cloud_supabase_boundary` with `supabase-cloud` checks actual CLI dispatch,
  bearer/context placement, normalized rows, safe failure diagnostics, no
  local fallback, and no credential forwarding across redirects. Actual CLI
  add/claim/release/done requests must use one ordered task-write, event-write,
  task-read batch with the same explicit ID and timestamp. Zero or non-singleton
  task/event counts must fail even when the response contains a task row.
  Get/show check keyed reads and explicit missing results. These scripted HTTP
  responses prove client composition only, never service atomicity or policy.
- `supabase_service::local_postgres_service_contract` is ignored by default.
  An external service harness must start actual PostgreSQL, apply a fresh
  schema, provision authenticated principals and grants, create a protected
  fixture, run the test, verify its receipt against the real event ledger,
  then deterministically destroy only its disposable database/roles.
- `supabase_service::protected_hosted_supabase_service_contract` is separately
  ignored. Run it only in an operator-approved protected environment with
  `DECAPOD_SUPABASE_HOSTED_PROOF=1`, HTTPS and disposable/namespaced data.
  The external fixture owner must guarantee cleanup on failure as well as
  success. No hosted endpoint, credential, or completed hosted result ships
  with this branch.

Invoke the local service test with:

```text
DECAPOD_SUPABASE_FIXTURE=/protected/temporary/fixture.json \
  cargo test --locked --features supabase-cloud --test supabase_service \
  local_postgres_service_contract -- --ignored --nocapture
```

The hosted command uses the other test name and hosted opt-in. Missing inputs
cause an explicit failure when an ignored test is selected; an unselected
ignored test is unavailable evidence, not a pass. A fixture declaration does
not prove its backend: the harness's actual PostgreSQL observations and cleanup
result must accompany the client receipt.

## Protected fixture contract

The fixture file is created outside Git with restrictive permissions. Do not
print it, attach it to CI artifacts, or include it in logs. Its schema is:

```json
{
  "profile": "local-postgres",
  "endpoint": "http://127.0.0.1:PORT",
  "repository": "fixture-owner/disposable-repo",
  "allowed_tokens": ["FIRST_USER_SESSION", "SECOND_USER_SESSION"],
  "denials": [
    {
      "case": "cross_org",
      "token": "DENIED_USER_SESSION",
      "repository": "fixture-owner/disposable-repo",
      "expected_error": "repository_not_authorized"
    }
  ],
  "disposable": true,
  "receipt_path": "/protected/temporary/client-receipt.json"
}
```

Required denial cases are `cross_org`, `wrong_group`, `wrong_team`,
`unauthorized_user`, `insufficient_access`, `revoked_access`, and
`expired_session`. Each must be provisioned by the service's public permission
contract. Insufficient-access fixtures may read but must deny
add/claim/release/done; other cases must also deny list/get/show. `expected_error` is a safe public error category.
The two allowed tokens must belong to independently authenticated users with
access to the same fresh repository slice, not two copies of one bearer.

The client proof creates one task, checks shared list/get/show and missing
rows, races two CLI claim processes, rejects a stale claim, releases/reclaims
and completes. Missing-task transitions and non-owner release/completion must
fail. Repeated transitions after completion must fail, and a subsequent read
by the other principal must still match the completed row. Denials must reveal
no task data and must leave task state and event counts unchanged. Its
non-secret receipt declares expected version 5 and exactly these events:
`task.add`, `task.claim`, `task.release`, `task.claim`, `task.done`.
The service harness verifies the ledger and adds separate deliberate-failure
rollback, equal-timestamp stale-CAS, type/count, fresh-schema and permission
proof. Those are service-side assertions, not conclusions inferred from a
successful client response.
