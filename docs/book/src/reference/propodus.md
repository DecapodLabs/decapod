# Cloud Todo Boundary

Supabase Auth owns the managed login session, Propodus owns hosted authorization
and task policy, and Dactyl owns authentication/data transport. Decapod orchestrates
the agent workflow and private session custody. `repo.backend = "cloud"` opens
a versioned repository context and never silently falls back to local SQLite.

## Dactyl storage contract

The Decapod bridge constructs local Dactyl SQLite or native Supabase RPC routes;
Neon HTTP remains an explicit compatibility alternative. It does not set or
trust ambient `DATASTORE`, `DATASTORE_ROUTE` or `DATASTORE_TOKEN` during route
construction. The current user bearer remains separate from serialized context.
SQL operations do not select a provider or interpret authorization.

The published `dactyl-db` 0.12.0 registry package supplies native RPC, managed
PKCE/refresh calls and bounded public deployment discovery. Its registry checksum
is pinned in `Cargo.lock`; no Git dependency or manual endpoint/key setup is
needed for the normal cloud route. Package availability does not prove hosted
OAuth, deployment or repository access. See
[Supabase consumer validation](supabase-validation.md).

| Operation | Native Dactyl request | Scope/authentication |
|---|---|---|
| Read/list | `POST /rest/v1/rpc/dactyl_query_v1` | SQL plus opaque versioned context |
| Add/claim/release/complete | `POST /rest/v1/rpc/dactyl_batch_v1` | ordered operations plus context |

Dactyl sends the bearer in `Authorization`, the public project key in `apikey`,
and the selected API schema separately from the request's repository context.
The service verifies identity, session, permissions and entitlement. Mutations
contain the conditional task write, matching event write and task observation.
The backend must commit the state and event together and reject stale transitions.
A transport failure does not prove rollback and never triggers a silent mutation
retry. A definite bearer rejection checkpoints only that exact saved credential;
the next agent invocation refreshes or reauthenticates without invalidating a
newer concurrent session. Permission denials remain access errors.

`tests/cloud_supabase_boundary.rs` covers native client composition and denial
propagation with synthetic HTTP. `tests/support/cloud_session_lifecycle.rs` covers init once,
agent handoff, resumed login, bearer rejection recovery and repository isolation.
Neither fixture establishes real hosted OAuth or database atomicity.
`tests/cloud_dactyl_boundary.rs`, `propodus_contract` and the historical onboarding
fixture retain explicit Neon/service compatibility coverage for `/query`,
`/batch`, `/api/todos` and custom machine-session exchange. Those legacy routes
are not used by managed Supabase sessions.

The command boundary is covered by `tests/cloud_command_path.rs`, which proves
that list, add, get, claim, release, and complete are routed through the
backend-neutral `TodoStore` adapter. Unsupported local-only operations return
an explicit error on the cloud backend.

The production-dispatch proof is `tests/cloud_cli_boundary.rs`; it uses a
mock Dactyl store factory and exercises the same `run_todo_cli` composition
used by the binary. It proves config discovery, canonical-origin validation,
credential preflight, list/add/get/show/claim/done routing, not-found behavior,
and the absence of local SQLite initialization for cloud todo commands.

The opt-in live proof is `tests/propodus_live.rs`. Run it only with
`DECAPOD_PROPODUS_LIVE=1`, `DECAPOD_PROPODUS_API_URL`,
`DECAPOD_PROPODUS_ACCESS_TOKEN`, and a
disposable `DECAPOD_PROPODUS_DISPOSABLE_REPO_ID`:

```text
DECAPOD_PROPODUS_LIVE=1 \
DECAPOD_PROPODUS_API_URL=https://your-stable-propodus.example \
DECAPOD_PROPODUS_ACCESS_TOKEN=... \
DECAPOD_PROPODUS_DISPOSABLE_REPO_ID=example/decapod-live-deny \
cargo test --test propodus_live -- --ignored --nocapture
```

The proof creates one uniquely named todo in the canonical repository,
claims it, completes it, and verifies that the disposable repository receives
`403 repository_not_authorized`. It also verifies that an invalid bearer token
is rejected with a 401 authentication failure. The command-level proof also
uses two Decapod agents to verify shared visibility and rejects a fork before
any request is sent. It does not delete the sentinel because the v1 client
contract has no delete operation; remove it with Propodus operator tooling
after the run. The test is ignored by default, and the CI job is manual,
environment-protected, and gated by the `DECAPOD_PROPODUS_LIVE` repository
variable.

Propodus also uses `403 organization_seat_required` when a valid GitHub bearer
token lacks the required organization seat for the canonical repository.

## Backend-neutral storage context

Decapod's versioned `core::backend::StorageContext` is the handoff from
logical backend selection to a physical driver. A local context contains only
the repository-local SQLite route and has no organization, user, or cloud
repository fields. A remote context contains the opaque service route and the
logical repository scope derived from `origin`; it requires an authenticated
session bearer, but the bearer is memory-only and is omitted from serialized
context data.

The Dactyl bridge forwards the explicit route, versioned context envelope, and
opaque bearer without interpreting membership or authorization. The bearer
never enters ambient Dactyl environment variables. Context debug formatting
redacts the bearer and endpoint; serialization excludes the bearer.
The versioned context carries the target org/repo scope unchanged. Propodus remains
responsible for resolving the authenticated principal, organization membership,
and repository access. The Decapod bridge opens the canonical local
`decapod.db` directly through Dactyl's local adapter and opens the cloud route
through Dactyl's Supabase adapter by default (or explicit Neon); it does not create a snapshot or bundled
compatibility database. Existing SQLite state is inspected and migrated by
Decapod through the same Dactyl-backed facade. This keeps one canonical Dactyl
authority and prevents cloud operations from silently falling back to local
storage. See [Decapod
#1254](https://github.com/DecapodLabs/decapod/issues/1254), [Dactyl
#64](https://github.com/DecapodLabs/dactyl/issues/64), and [Propodus
#79](https://github.com/DecapodLabs/propodus/issues/79).

## Credentials and the agent handoff

The human runs `decapod init` once per repository, selecting local or cloud.
Cloud init records setup and provides the existing purchase approval link when
needed. Afterward the human chats with their coding agent. The agent invokes
`decapod session acquire` and cloud task commands, coordinates browser sign-in
and resumes the original command after the user finishes. No repeated init,
manual token export or separate human login command is needed.

Supabase Auth owns access tokens and rotating refresh credentials. Decapod owns
private durable custody; Dactyl 0.12.0 owns deployment discovery, managed PKCE
exchange, refresh and native RPC transport. A successful browser login becomes
a saved usable session only after server-confirmed repository authorization.
Authentication does not activate payment or grant repository permissions.

Credentials never come from `.decapod/config.toml`. An explicit client credential
or `DECAPOD_ACCESS_TOKEN` test override takes precedence. Normal Supabase
sessions live under `$XDG_DATA_HOME/decapod/services/<origin-hash>/repositories/<repo-hash>/session_token.json`,
defaulting to `~/.local/share/decapod`. The canonical GitHub owner/name selects
the stable repository scope across clones and worktrees. Other repositories,
service endpoints and the legacy Neon store are isolated. `cloud status` reads
the same selected repo scope without network requests; presence is not proof of
current server authorization.

The agent receives `decapod.cloud.auth.v1` with `onboarding_pending` and
`authorization_url` when a human browser action is necessary. It shares the
link, retains the task and retries its command after sign-in. Pending PKCE/poll
secrets survive process restarts privately. A bounded repository lock serializes
refresh, atomic private writes preserve rotated tokens, and uncertain one-time
responses are never silently replayed. Valid 15-minute tokens are reused; refresh
starts within 60 seconds of expiry. Normal requests carry only the current
access bearer, never refresh material. Errors never fall back to SQLite.

See [Supabase consumer validation](supabase-validation.md) for native RPC,
configuration, protected proof profiles and the distinction between local
client evidence and hosted OAuth/service execution.

## Repeatable dogfood setup

From a fresh checkout of the canonical repository:

1. Run `decapod init --backend cloud --proof` and confirm `.decapod/config.toml`
   contains only the backend selection `repo.backend = "cloud"` for cloud
   composition. The Propodus endpoint and provider are binary-owned; no service
   URL or project identifier is stored in project configuration.
2. Ensure `origin` is an unambiguous GitHub remote. Decapod derives the
   canonical owner/name from it and sends that binding to the provider; the
   provider decides whether the authenticated session may use the repository.
3. Run `decapod todo list`, `add`, `get`, `show`, `claim`, or `done`. If no
   machine session exists, the agent starts or resumes browser sign-in and
   retries its command when the human returns. A controlled `DECAPOD_ACCESS_TOKEN`
   remains available for protected proofs.
4. Cloud todo commands use the same `TodoStore` command boundary as local
   todo commands, but compose Dactyl storage with the Propodus-authenticated
   context instead of local SQLite. They do not acquire the local agent
   session or create/migrate the local todo database. Missing credentials or a canonical
   GitHub remote fail closed and never fall back to SQLite.

## Repository identity

The cloud backend derives a canonical `owner/name` binding from the `origin` remote
and rejects non-GitHub or ambiguous remotes. Forks remain distinct identities
and are passed to the provider for authorization; Decapod does not maintain a
repo allowlist or accept a project-configured repository identifier.

## Authenticated delegated agent authority

The agent is the operational caller. In local mode, a solo developer does not
need cloud authentication: the local agent/session label is cooperative custody,
not a verified human identity. In cloud mode, the same agent interface uses the
user's managed Auth bearer. Propodus determines authorization independently
from the bearer, active GitHub-linked identity, membership, repository grant,
and entitlement. `DECAPOD_AGENT_ID`, project config, and task fields are
never sources of cloud privileges.

Propodus repository tiers are `read`, `write` (contributors), `maintainer`,
and `owner`. Contractor access is a direct, repository-scoped user grant with
a finite expiration; it is not a new agent type. Direct user grants take
precedence over team, group, and organization inheritance, even after expiry,
so revocation cannot silently restore inherited access. Operator-only
provisioning/revocation functions do not enter the agent RPC surface.

Cloud tasks store the agent label for coordination but enforce claim release
and completion against the authenticated principal. Copying another agent's
label cannot transfer custody. Maintainer/owner repair is a separately
authorized, audited exception, and remote publication still requires its
independent proof and merge checks.

The canonical enforcement and regression tests live in Propodus schema
migration 007. An ordinary client contract test cannot establish actual
PostgreSQL or hosted permission enforcement; the protected
`supabase_service` profile must exercise two independently authenticated users
with a deliberately spoofed agent label.

For that protected profile, first claim a task with one authenticated session.
Then reuse the same agent label with a different principal's session and
attempt both release and completion. Neither operation may mutate the task or
append a successful event, and a rejected cloud call must never fall back to
local SQLite. This tests authority independently of the agent's chosen name.

## Current governance limits

The cloud Dactyl todo slice supports repo-scoped list/add/get/claim/release/
complete operations. `get` and `show` use keyed reads through Dactyl rather
than list-and-filter. Add, claim, release, and complete each use one Dactyl
atomic batch for the conditional task transition, matching event, and final
task observation. A compatible service must enforce that a zero-row task
transition cannot create an event, even if a timestamp marker matches an older
operation. Client checks of returned counts occur after the remote transaction
and cannot alone enforce rollback. The Supabase service proof tests this
server-side requirement; timestamps alone are not compare-and-swap authority. A missing item returns `status = "not_found"`.
`todo done --validated` is intentionally rejected because v1 has no proof
capture or verification-artifact contract. This is an explicit unsupported
boundary, not full remote governance completion; a future proof-contract issue
must define the service-side evidence model before Decapod can compose it;
see [Decapod issue #1038](https://github.com/DecapodLabs/decapod/issues/1038).

## Delivery boundary

## Local SQLite maintenance

Local operators can inspect and maintain the canonical store explicitly:

```text
decapod data db verify
decapod data db backup --destination /same-directory-or-approved-backup/decapod.backup.db
decapod data db recover --preserve-original-at /same-directory/decapod.before-recovery.db
```

`verify` is read-only and calls Dactyl v0.10.0's typed native integrity
verification. `backup` uses Dactyl's SQLite online-backup contract, including
WAL/SHM correctness, and publishes a standalone destination after verification.
`recover` is an explicit operator action only: it invokes Dactyl's logical
dump/reload workflow, preserves the original at the requested unused sibling
path, and atomically replaces the active database only after the replacement
passes verification. Dactyl reports rollback or recovery failures without
silently repairing the store. The recovered database uses DELETE journal mode
and preserves application rows, schema, `user_version`, and `application_id`.

The canonical Decapod sidecar lock is held for each operation. Recovery also
requires all cooperating Decapod writers and same-process Dactyl connections
to be quiesced; backup requires normal cooperating-client coordination. The
archive must not already exist and must be on the same filesystem where Dactyl
can perform its atomic replacement. The sidecar does not coordinate arbitrary
external SQLite writers or make unreliable network/container mounts safe.
Lock contention, unavailable runtimes, malformed/corrupt stores, unsupported
capabilities, and recovery/rollback failures remain explicit diagnostic
outcomes. Decapod never runs repair during startup, validation, event append,
or connection creation.

This Decapod slice activates the explicit cloud todo command path,
remote-derived repository identity, Propodus onboarding/session exchange,
machine-local refresh, Dactyl 0.10.0 adapter-level command and event-atomicity proof, and a wire-level
`/query` proof without moving hosted authentication, repository authorization,
persistence, or deployment into Decapod. Local mode continues to use the same
backend-neutral todo command boundary, with physical canonical storage owned
by Dactyl. Live Neon/Vercel availability, Propodus/Dactyl service deployment,
cross-organization isolation, schema/migration parity, and hosted event
atomicity remain deployment-dependent proof gates; the client-side batch
contract and local rollback proof do not claim those hosted properties.

For interrupted cloud creation and auth-blocked custody, see
[Resuming cloud todo creation](cloud-todo-recovery.md). A returned operation token
is a retry identity, never an authorization grant or local task-store fallback.
