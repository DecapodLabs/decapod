# Propodus Todo Boundary

Propodus is an optional remote service boundary for repo-scoped todos. It is
not compiled into Decapod and it does not replace local SQLite by default.

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

The opt-in live proof is `tests/propodus_live.rs`. Run it only with
`DECAPOD_PROPODUS_LIVE=1`, `DECAPOD_PROPODUS_API_URL`,
`DECAPOD_PROPODUS_ACCESS_TOKEN`, and a disposable
`DECAPOD_PROPODUS_DISPOSABLE_REPO_ID`:

```text
DECAPOD_PROPODUS_LIVE=1 \
DECAPOD_PROPODUS_API_URL=https://your-stable-propodus.example \
DECAPOD_PROPODUS_ACCESS_TOKEN=... \
DECAPOD_PROPODUS_DISPOSABLE_REPO_ID=DecapodLabs/propodus-live-deny \
cargo test --test propodus_live -- --ignored --nocapture
```

The proof creates one uniquely named todo in the canonical repository,
claims it, completes it, and verifies that the disposable repository receives
`403 repository_not_authorized`. It does not delete the sentinel because the
v1 client contract has no delete operation; remove it with Propodus operator
tooling after the run. The test is ignored by default, and the CI job is
manual, environment-protected, and gated by the `DECAPOD_PROPODUS_LIVE`
repository variable.

## Credentials

Credentials are never read from `.decapod/config.toml`. Lookup precedence is:

1. an explicit client credential;
2. `DECAPOD_ACCESS_TOKEN` for controlled development and CI use;
3. the machine-local `session_token.json` written by `decapod cloud login`.

Use `decapod cloud status` to check availability without printing the token.
Bearer tokens are sent only in the `Authorization` header. The current
machine-local file is a credential boundary, not provider authentication proof;
backend verification remains a Propodus deployment responsibility.

## Delivery boundary

Wave 1 provided the Decapod-side contract, credential lookup, typed client,
storage adapter, and deterministic local proof. Wave 2 adds the client health
probe and a credential-gated live integration proof without moving hosted
authentication, repository allowlisting, stable URL ownership, persistence, or
deployment into Decapod. Those remain Propodus responsibilities. The client
stays configurable so deployment decisions do not become hardcoded repository
assumptions.
