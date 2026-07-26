# Propodus Todo Boundary

Propodus is an optional remote service boundary for repo-scoped todos. It is
not compiled into Decapod and it does not replace local SQLite by default.
`repo.backend = "cloud"` is an explicit opt-in: in that mode the todo commands
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

The deployed onboarding/session shape is recorded separately in
`tests/fixtures/propodus/onboarding-contract-v1.json`: Decapod starts with the
canonical `repo_id`, prints/opens the one-time URL, polls status, consumes the
ready flow once, exchanges the returned code for a machine session, and
rotates that session through the refresh route. Credentials stay outside the
repository.

The command boundary is covered by `tests/cloud_command_path.rs`, which proves
that list, add, claim, and complete are routed through the backend-neutral
`TodoStore` adapter. Unsupported local-only operations return an explicit
error in cloud mode.

The production-dispatch proof is `tests/cloud_cli_boundary.rs`; it uses a
mock Propodus store factory and exercises the same `run_todo_cli` composition
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

## Credentials

Credentials are never read from `.decapod/config.toml`. Lookup precedence is:

1. an explicit client credential;
2. `DECAPOD_ACCESS_TOKEN` for controlled development and CI use;
3. the machine-local `~/.local/share/decapod/session_token.json`.

Use `decapod cloud status` to check whether a bearer is configured without
printing the token. `decapod cloud login` and the first cloud todo command use
the repository's configured cloud endpoint to start or resume onboarding.
Interactive terminals open the URL when a browser launcher is available and
always print it as a fallback; headless callers receive a bounded resume
instruction. A completed exchange is stored machine-locally with restrictive
permissions and expired sessions refresh through the provider. Bearer tokens
are sent only in the `Authorization` header; Decapod never logs or writes
them to repository configuration, generated governance, URLs, or commits.

## Repeatable dogfood setup

From a fresh checkout of the canonical repository:

1. Run `decapod init --backend cloud --proof` and confirm `.decapod/config.toml`
   contains `repo.backend = "cloud"`, `[cloud].enabled = true`, and the intended
   Propodus `api_url`. The endpoint is selected only from this project config;
   no service URL is inferred from credentials.
2. Ensure `origin` is an unambiguous GitHub remote. Decapod derives the
   canonical owner/name from it and sends that binding to the provider; the
   provider decides whether the authenticated session may use the repository.
3. Run `decapod todo list`, `add`, `get`, `show`, `claim`, or `done`. If no
   machine session exists, Decapod starts the one-time browser handoff and
   resumes it on the next invocation. A controlled `DECAPOD_ACCESS_TOKEN`
   remains available for protected proofs.
4. Cloud todo commands use the same `TodoStore` command boundary as local
   todo commands, but compose the Propodus adapter and JWT instead of local
   SQLite. They do not acquire the local agent session or create/migrate the
   local todo database. Missing credentials, cloud config, or a canonical
   GitHub remote fail closed and never fall back to SQLite.

## Repository identity

Cloud mode derives a canonical `owner/name` binding from the `origin` remote
and rejects non-GitHub or ambiguous remotes. Forks remain distinct identities
and are passed to the provider for authorization; Decapod does not maintain a
repo allowlist or treat `cloud.repo_id` as authority to select another
repository.

## v1 governance limits

Propodus v1 supports repo-scoped list/create/claim/complete operations. `get`
and `show` intentionally use list-and-filter because the v1 TodoStore contract
has no repo-scoped get route; a missing item returns `status = "not_found"`.
`todo done --validated` is intentionally rejected because v1 has no proof
capture or verification-artifact contract. This is an explicit unsupported
boundary, not full remote governance completion; a future proof-contract issue
must define the service-side evidence model before Decapod can compose it;
see [Decapod issue #1038](https://github.com/DecapodLabs/decapod/issues/1038).

## Delivery boundary

Wave 1 provided the Decapod-side contract, credential lookup, typed client,
storage adapter, and deterministic local proof. Wave 2 activates the explicit
cloud todo command path, remote-derived repository identity, deployed
onboarding/session exchange, machine-local refresh, adapter-level command
proof, and protected command-level live proof without moving hosted
authentication, repository authorization, persistence, or deployment into
Decapod. Local mode continues to use SQLite through the same backend-neutral
todo command boundary.
