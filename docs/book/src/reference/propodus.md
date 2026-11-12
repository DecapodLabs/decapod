# Cloud sign-in and task access

Select `repo.backend = "cloud"` to use the managed cloud connector. The
boundary is **agent → Decapod → Dactyl → hosted service**. Decapod owns the
system interface, repository binding, and private session custody. Dactyl
owns public discovery, the physical connector, sign-in and session transport,
and subsequent data requests. The service decides who may access a repository.

Project configuration needs only the logical choice:

```toml
[repo]
backend = "cloud"
```

No deployment URL, physical driver, copied token, price, membership roster, or
role assignment belongs in this configuration. Local mode works without cloud
credentials or cloud discovery.

## Human setup and headless continuation

From a repository with a GitHub `origin`, run:

```text
decapod init --backend cloud
```

The same flow is used after manually changing the backend to `cloud`. A normal
cloud task command or `decapod session acquire` starts or resumes the same
repository-bound session. An existing unexpired session is reused; a session
near expiry is refreshed while holding its private lock.

If sign-in is needed, Decapod prints a validated, one-time browser handoff and
returns a non-success `onboarding_pending` result with `authorization_url`.
The agent shares that link and runs `decapod cloud login` every two seconds for
up to five minutes. It resumes the task as soon as sign-in completes, without
waiting for another user message. If still pending, it preserves the saved flow.
The human runs init once per repository and completes browser actions; later
CLI operations belong to the agent. Each invocation polls at most once.

The URL and a successful browser redirect are not initialization success.
Decapod accepts the issued session, asks the service to complete the account
handoff, and performs an authenticated, read-only repository task lookup.
Only a successful authorized read produces the setup-ready message. It proves
current read access; the service independently authorizes every later write,
claim, completion, or privileged operation.

The current service reports an unresolved account entitlement as
`action_required`. The client does not guess whether payment or seat
assignment will resolve it, and it does not invent an account-action URL.
An explicit access denial is `repository_denied`. Both remain non-success
outcomes. When a session has already been issued, a retry can recheck account
completion with that same session without consuming another sign-in code.

`decapod cloud status` inspects private local state without network requests
or printing a token. Its phase is a custody state; `server_readiness` is
`not_checked`, even when the saved phase is `ready`. Use initialization or a
normal cloud command or agent-owned `decapod cloud login` to obtain a current
server decision.

`decapod init --backend cloud --proof` prepares configuration only. It does
not authenticate, create a synthetic credential, or claim server readiness.

## Recovery and private custody

Pending sign-in state and credentials are stored outside Git, under the
machine data directory. Each record is bound to the selected service origin,
the GitHub repository, and the canonical main checkout. Linked worktrees of
that checkout share custody; separate clones do not. A different origin or
repository cannot reuse the record.

Directories and files must have private ownership and permissions (`0700`
and `0600` on Unix). The store rejects symlink and hard-link credential files,
oversized or malformed records, binding mismatches, and storage located in a
Git checkout. A stable, separate lock serializes updates and refresh. Atomic
replacement synchronizes the file and parent directory before continuing.

Decapod saves an in-flight marker before a one-time bootstrap exchange,
session exchange, or refresh. It saves each received code or rotated session
before making the next request. A later process can resume safe status polls
and account-completion checks. If a one-time response was lost, or the process
stopped before saving it, the outcome is explicitly uncertain. Decapod does
not replay that request or treat an old refresh token as reusable.

For an uncertain exchange, expired handoff, or rejected managed credential,
the agent follows the reported recovery instruction:

```text
decapod cloud login --restart
```

This explicitly clears the selected private flow and starts a new sign-in.
It does not clear another repository's custody. A known rejected bearer is
marked unusable; a late rejection of an old bearer cannot invalidate a newer
rotation saved by another command.

Access tokens, refresh tokens, PKCE verifiers, polling secrets, and exchanged
codes never enter project configuration, Git artifacts, diagnostic messages,
or process arguments. The browser handoff is intentionally shown to the human;
treat it as a short-lived sign-in link. Dactyl sends credentials only through
the selected validated transport and refuses cross-origin redirects.

## Cloud task boundary

Cloud `todo list`, `add`, `get`, `show`, `claim`, `release`, and `done` use the
same `TodoStore` interface as local commands. The active cloud factory obtains
the current managed session and passes its opaque bearer and versioned
repository context to Dactyl's logical connector. It does not choose a hosting
provider or physical database. It does not initialize or fall back to SQLite
when discovery, authentication, authorization, or a data request fails.

Dactyl carries the service's native query and batch protocol. Decapod supplies
reviewed task SQL and ordered atomic operations; the service must enforce
repository scope, conditional transitions, event consistency, and transaction
rollback. The startup read uses the existing authorized task-get statement,
not a health endpoint or an unauthenticated `SELECT 1` shortcut.

`todo done --validated` remains unsupported because the current cloud task
contract does not provide remote proof capture and verified completion. This
client change does not establish full remote governance parity.

For a data mutation with a lost response, use the operation identity described
in [Resuming cloud todo creation](cloud-todo-recovery.md). Session recovery
does not replay a task mutation. A retry identity is not an authorization grant.

## Identity and service-owned authority

Decapod derives the requested `owner/name` from the GitHub `origin`. Forks
remain distinct repositories. A project-configured identifier, email address,
mutable SCM username, or agent label cannot grant access. The response must
match the requested repository before it is accepted.

The hosted service resolves the managed principal, linked identity, grants,
membership, entitlement, and effective privilege. It also decides whether an
account action is still pending. The client neither implements a price table
nor infers authority from payment or seat assignment.

`DECAPOD_AGENT_ID` identifies the operational caller for coordination. Copying
another agent's label cannot transfer authenticated cloud custody. Exceptional
maintainer or owner actions require their separate service authorization and
audit trail. Publication and merge retain their independent governance gates.

## Implementation and evidence boundaries

The active orchestration is `core::cloud_client`; `core::cloud_session` owns
private custody. `DactylBridge` resolves the managed route through
`dactyl_db::CloudConnector`. Historical explicit adapters and Propodus client
types remain compatibility APIs, not the ordinary cloud setup path. Ambient
legacy endpoint, physical-selector, and access-token variables do not select
or authenticate the managed CLI flow.

`tests/cloud_managed_session.rs` exercises the actual CLI against disposable
loopback HTTP fixtures: missing and reused sessions, headless continuation,
serialized refresh, interrupted exchanges, account requirements and denial,
binding isolation, redaction, and absence of SQLite fallback. Custody unit
tests exercise private files, locks, phase shapes, and crash recovery.
Existing task command, wire, idempotency, and explicit adapter tests retain
their narrower contracts.

These fixtures establish client behavior only. They are not real PostgreSQL,
RLS, hosted policy, or production readiness evidence. Those separate gates
remain in [#1377](https://github.com/DecapodLabs/decapod/issues/1377),
[#1378](https://github.com/DecapodLabs/decapod/issues/1378), and
[#1369](https://github.com/DecapodLabs/decapod/issues/1369). See
[Supabase consumer validation](supabase-validation.md) for protected service
profiles and their explicit evidence requirements.

Connector-enabled builds consume the published `dactyl-db 0.13.0` API, retaining
the cloud interface introduced in
[Dactyl #102](https://github.com/DecapodLabs/dactyl/pull/102). Its registry
version and checksum are checked by the release dependency policy. Required
dependency, compilation and fixture checks still have to pass on the exact
candidate; a release alone does not establish client readiness. The dependency
handoff remains tracked in [#1380](https://github.com/DecapodLabs/decapod/issues/1380).
No Git/path override or source-overlay diagnostic substitutes for those gates.

## Local SQLite maintenance

Local operators retain explicit integrity, backup, and recovery commands:

```text
decapod data db verify
decapod data db backup --destination /approved-backup/decapod.backup.db
decapod data db recover --preserve-original-at /same-directory/decapod.before-recovery.db
```

Dactyl owns physical integrity checks, online backup, and logical recovery.
Decapod owns the canonical file, coordination lock, authorization, and approved
destination. Recovery preserves the original and publishes a verified
replacement only after quiescing cooperating writers. These commands require
the local backend. Cloud sign-in never performs local database maintenance.
