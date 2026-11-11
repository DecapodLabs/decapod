# Supabase consumer validation

Project configuration selects `repo.backend = "local" | "cloud"`. The normal
cloud path is AI ↔ Decapod ↔ Dactyl ↔ Supabase. Dactyl resolves the deployment
profile and owns managed Auth and PostgreSQL RPC transport; Propodus is the
infrastructure-as-code for those services. Local storage remains Dactyl-backed
SQLite. Explicit Neon compatibility is retained for existing integrations.

## Connector activation

The coordinated connector change consumes an immutable reviewed Dactyl Git
revision in `Cargo.toml` and `Cargo.lock`. Registry `dactyl-db` 0.11.1 does not
contain these new logical connector/auth APIs. Dactyl must release the reviewed
APIs and Decapod must adopt that registry release before a normal registry-based
Decapod release. Development source proof is not proof of a registry handoff.
No local-path override, database password or service-role key is required.

This development candidate is pinned to Dactyl commit
`a85607e8a8cbfce10cee1daca249efd58f8d58f3`. The accepted registry-only
`tests/release_dependency_policy.rs` policy remains unchanged and will reject
this temporary source pin. Nix vendoring still requires a registry checksum or
an explicitly reviewed Git output hash, and Bazel's committed extension lock
must reflect the final release dependency. Neither packaging path is declared
ready here; their checks are preserved. A published connector release and a
normal registry dependency/lock update are required before Decapod adoption can
pass its complete release contract. No merge or crate publication is performed
by this source-integration work.

1. Select `repo.backend = "cloud"` in `.decapod/config.toml`, or use the normal
   `decapod init --backend cloud` setup flow. Default builds enable
   `supabase-cloud`; a build without that feature fails closed for this route.
2. Dactyl uses its built-in cloud origin and fetches the bounded public
   `/functions/v1/propodus-onboarding/config` profile. Discovery carries no user
   bearer. The versioned response contains only project origin, publishable key
   and API schema; Dactyl validates it against the selected origin. Ordinary
   project config contains no endpoint or publishable-key requirement.
3. Decapod supplies the current user session separately from its repository
   context. If no session is available, Decapod invokes Dactyl's managed browser
   authentication calls. A headless invocation prints a sign-in URL and returns
   onboarding pending; complete the browser flow and rerun the agent command.
   No standalone task server or direct PostgreSQL client is used.
4. Dactyl sends native requests to `/rest/v1/rpc/dactyl_query_v1` and
   `/rest/v1/rpc/dactyl_batch_v1` with the public project key, API schema,
   contract version, user bearer and opaque repository context. No cloud failure
   creates or falls back to local replacement task state.
5. Account registration/status and the org-admin portal establish tenant/project
   setup and active assigned paid-seat readiness. Successful sign-in alone does
   not prove paid-seat access. Those coordinated backend/init slices have their
   own issues and service proofs; this client transport change does not claim
   hosted OAuth, billing, deployment or full cloud-governance parity.

Machine-only `DACTYL_CLOUD_ORIGIN` and `DACTYL_CLOUD_PUBLISHABLE_KEY` overrides
support an explicitly selected private or test deployment. Their values are not
written to repository configuration. `DECAPOD_CLOUD_DATASTORE=neon` retains the
explicit legacy Neon route using `DECAPOD_NEON_API_URL`; ordinary users select
only local or cloud. Invalid configuration or an unavailable connector feature
fails before onboarding or task I/O. Authentication, authorization, timeout,
transport, protocol and storage failures remain visible.

## Human session custody

All profile, Auth and data HTTP requests pass through Dactyl. Discovery and
managed Auth refuse redirects, and diagnostics do not contain raw service
response bodies. Decapod owns the human-facing browser handoff and private
machine-local persistence. Endpoint-scoped session files isolate deployments
and the legacy Neon session. A supplied `DECAPOD_ACCESS_TOKEN` takes precedence
without consuming or overwriting saved onboarding state; use this explicit
override only for the selected service.

Pending browser flows retain PKCE/poll secrets privately so headless setup can
resume. One-time exchange and rotating refresh requests are checkpointed before
submission and are not automatically replayed after an unknown result. Private
records are bounded to 64 KiB, reject symlinks and use atomic private writes,
including Unix file and directory syncing. A bounded per-service lock serializes
refresh across repositories and processes. An optional sign-in provider token
is retained atomically with the session, outside ordinary data credentials and
Debug output. Sign-in may use Google; a provider token is never inferred to be
an SCM credential or automatically forwarded for SCM linking.

Test fixtures isolate `HOME` and both XDG directories and use synthetic or
explicit disposable session material. Credentials never enter the versioned
storage context, repository config, Debug output or Git.

## Ownership and permissions

Git holds governance plans, claims, trajectory, validation, specs and proof
artifacts. Supported operational tasks and matching events use the selected
backend. There is no SQLite-file upload or synchronization.

Ordinary runtime authority is an authenticated Supabase user UUID with an active
assigned paid seat in the correct tenant/customer/project. Linked SCM identities
are verified server-side when needed; usernames, email and local RBAC mappings
are not grants. Code-host repository/PR/merge permissions are not mirrored into
the ordinary app access hierarchy. Privileged governance controls stay separate.
Cloud task custody also binds the authenticated principal independently from an
agent's freely chosen label. Client fixtures establish composition and denial
propagation, not actual backend seat policy or tenant isolation.

The reviewed task/event RPC contract commits a task mutation, its matching event
and final task read as an ordered batch. The service must commit both task and
event or neither, gate event insertion on one affected task row, and return one
winner for concurrent claims. Stale or missing transitions cannot append success
events. Lost responses remain ambiguous and require the existing operation
receipt/reconciliation protocol; an HTTP error is not a rollback receipt.

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
  "rpc_v1": {
    "publishable_key": "sb_publishable_DISPOSABLE_PROFILE_KEY",
    "schema": "dactyl_api"
  },
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

The retained compatibility permission profile requires denial cases
`cross_org`, `wrong_group`, `wrong_team`,
`unauthorized_user`, `insufficient_access`, `revoked_access`, and
`expired_session`. Each must be provisioned by the service's public permission
contract. These legacy grant-policy cases do not define the normal paid-seat
access hierarchy; the companion identity/seat proof must cover the new tenant
and assigned-seat contract separately. Insufficient-access fixtures may read but must deny
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
