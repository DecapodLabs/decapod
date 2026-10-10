# Supabase consumer validation

The cloud backend selects the public Dactyl Supabase HTTP capability by
default. The client uses the published `dactyl-db` 0.11.0 registry release.
This dependency alignment does not migrate historical state, provision
Supabase, or prove a production deployment. Local storage remains SQLite; Neon is an explicit cloud alternative.

## Fresh activation

1. Decapod consumes `dactyl-db` 0.11.0 from crates.io, including the Supabase
   client capability reviewed in [Dactyl #92](https://github.com/DecapodLabs/dactyl/pull/92)
   and released in [v0.11.0](https://github.com/DecapodLabs/dactyl/releases/tag/v0.11.0).
   `Cargo.lock` records the registry source and checksum; there is no Git or
   local-path dependency override. Build with `cargo build --locked`; the
   default features include `supabase-cloud`. A `--no-default-features` build
   retains SQLite and explicit Neon, and fails closed if the unavailable
   Supabase route is selected. Nix vendoring uses the same lockfile checksum.
   Before publishing Decapod, run `cargo package --locked` and
   `cargo publish --locked --dry-run` to verify the packaged registry dependency.
2. An operator must first provide a fresh compatible authenticated service
   backed by PostgreSQL, its task/event schema, and resource permissions.
   The client does not initialize hosted schemas or connect to PostgreSQL.
3. In machine-local runtime configuration, set `DECAPOD_PROPODUS_API_URL` to
   that service's HTTPS origin. No datastore override is required: omitted
   `DECAPOD_CLOUD_DATASTORE` selects Supabase. Explicit `supabase` is also accepted. Literal-loopback HTTP is supported for disposable
   test services only. This is neither a PostgreSQL DSN nor Supabase REST.
4. Select `repo.backend = "cloud"`. Production use requires a service that
   also implements the existing `decapod init --backend cloud` onboarding,
   session exchange and refresh contract. A task-only `/query` and `/batch`
   service does not provide those authentication endpoints. The isolated
   conformance profile supplies disposable ordinary user tokens directly; it
   does not establish production onboarding. Ordinary clients receive only
   user/session credentials. Database passwords, operator credentials, and
   service-role keys never enter Decapod.
5. Run normal `todo list`, `add`, `get`, `show`, `claim`, `release`, and
   `done` commands. `todo done --validated` remains unsupported pending the
   separate remote proof-capture contract.

The selector is distinct from `repo.backend = local|cloud` and from the
hosting-provider label. Its default is `supabase`. Set
`DECAPOD_CLOUD_DATASTORE=neon` to select the retained Neon route explicitly; only
that selection can use the legacy Neon service endpoint by default. An unknown
selector, a build without `supabase-cloud`, an invalid endpoint, or a missing
Supabase service endpoint fails closed before
onboarding or todo I/O. Authentication, authorization, timeout, transport,
protocol, and storage failures never initialize or fall back to local todos.

Dactyl validates the Supabase endpoint before session onboarding or refresh.
Both Supabase authentication and data requests refuse redirects so an HTTP
redirect cannot forward a refresh body or user bearer to another origin.
Supabase authentication passes bearer headers and JSON through the child
transport's private stdin, not process arguments or a secret temporary file;
ambient curl configuration is disabled for that path.
Supabase sessions are stored in endpoint-scoped machine-local files. Switching
service endpoints never reuses or refreshes the legacy global Neon session or
another endpoint's session. A new endpoint requires its own onboarding. An
explicit `DECAPOD_ACCESS_TOKEN` remains a controlled test/operator override;
set it only for the selected service. Test fixtures isolate `HOME` and both
XDG directories and use synthetic/disposable session material exclusively.
No credentials appear in the versioned context, debug output, or repository
configuration. The client selector is also omitted from the wire context.

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
