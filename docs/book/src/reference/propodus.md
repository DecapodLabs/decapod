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

This phase provides the Decapod-side contract, credential lookup, typed client,
storage adapter, and deterministic local proof. The following require the next
wave and explicit deployment evidence: hosted authentication and repository
allowlisting, a stable production alias, and an opt-in live integration proof.
The client remains configurable so those deployment decisions do not become
hardcoded repository assumptions.
