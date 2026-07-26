# Propodus Todo Boundary

Propodus is an optional remote service boundary for repo-scoped todos. It is
not compiled into Decapod and it does not replace local SQLite by default.
`repo.mode = "cloud"` is an explicit opt-in: in that mode the todo commands
use Propodus directly and never silently fall back to local SQLite.

## Contract v1

The Decapod client uses the versioned contract `decapod.propodus.todo/v1`:

| Operation | Route | Required inputs |
|---|---|---|
| Health | `GET /api/health` | none |
| List | `GET /api/todos?repo_id=<repo>` | `repo_id` |
| Create | `POST /api/todos` | `repo_id`, `title` |
| Claim | `PATCH /api/todos?id=<todo>` | `status=in_progress`, `actor` |
| Complete | `PATCH /api/todos?id=<todo>` | `status=completed`, `actor` |

The checked-in compatibility fixture is
`tests/fixtures/propodus/todo-contract-v1.json`. Its consumer proof is the
local `propodus_contract` test, which uses an injectable fake transport and
never contacts Vercel, Neon, or production data.

The command boundary is covered by `tests/cloud_command_path.rs`, which proves
that list, add, claim, and complete are routed through the backend-neutral
`TodoStore` adapter. Unsupported local-only operations return an explicit
error in cloud mode.

The opt-in live proof is `tests/propodus_live.rs`. Run it only with
`DECAPOD_PROPODUS_LIVE=1`, `DECAPOD_PROPODUS_API_URL`,
`DECAPOD_PROPODUS_ACCESS_TOKEN`, `DECAPOD_GITHUB_REPOSITORY_ID`, and a
disposable `DECAPOD_PROPODUS_DISPOSABLE_REPO_ID`:

```text
DECAPOD_PROPODUS_LIVE=1 \
DECAPOD_PROPODUS_API_URL=https://your-stable-propodus.example \
DECAPOD_PROPODUS_ACCESS_TOKEN=... \
DECAPOD_GITHUB_REPOSITORY_ID=... \
DECAPOD_PROPODUS_DISPOSABLE_REPO_ID=DecapodLabs/propodus-live-deny \
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

## Credentials

Credentials are never read from `.decapod/config.toml`. Lookup precedence is:

1. an explicit client credential;
2. `DECAPOD_ACCESS_TOKEN` for controlled development and CI use;
3. the machine-local `session_token.json`.

Use `decapod cloud status` to check availability without printing the token.
Bearer tokens are sent only in the `Authorization` header. The current
machine-local file is a credential boundary, not provider authentication proof;
backend verification remains a Propodus responsibility. Propodus PR #31
defines the active bearer contract as a GitHub-subject JWT with repository and
seat authorization. Decapod can consume such a machine credential, but the
GitHub login/token exchange remains deferred until Propodus issue #24 exposes
that stable route; the legacy Auth0 device flow is not part of the active
Propodus todo path.

## Repository identity

Cloud dogfood is fail-closed to the canonical `DecapodLabs/decapod` origin.
Decapod derives owner/name from `origin`, rejects non-GitHub, ambiguous, and
fork remotes, and requires the immutable GitHub repository identity supplied by
`DECAPOD_GITHUB_REPOSITORY_ID`. The project file's `cloud.repo_id` is not an
authority to select another repository. Propodus currently authorizes the
canonical name; the immutable value is retained at the Decapod identity
boundary until the service contract exposes its final wire field.

## Delivery boundary

Wave 1 provided the Decapod-side contract, credential lookup, typed client,
storage adapter, and deterministic local proof. Wave 2 now activates the
explicit cloud todo command path, verified repository identity, adapter-level
command proof, and protected command-level live proof without moving hosted
authentication, repository allowlisting, stable URL ownership, persistence, or
deployment into Decapod. Those remain Propodus responsibilities. The client
stays configurable so deployment decisions do not become hardcoded service
policy.
