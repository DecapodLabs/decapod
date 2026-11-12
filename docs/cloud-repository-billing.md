# Cloud account readiness and repository access

Decapod owns the agent-facing workflow, repository context, local governance and
private credential custody. Dactyl owns generic connector, authentication/account
transport and storage execution. The hosted service owns managed identity
validation, effective repository authorization and authoritative capacities and
assignments. Commercial configuration and lifecycle decisions are outside this
public repository.

## Current consumer boundary

The source in `Cargo.toml` consumes the exact Dactyl 0.12.0 registry version.
`src/decapod/core/cloud_session.rs` coordinates managed sign-in, single-use
handoff, serialized refresh and private custody through Dactyl's connector.
`cloud_account.rs` registers or resumes an idempotent project reference and
requires a current server authorization response. Native data explicitly uses
Dactyl's Supabase RPC configuration. Legacy session helpers remain available for
the explicitly selected legacy service route; they do not define native Auth.

Interactive cloud init starts or resumes managed setup. Proof, dry-run and
noninteractive init record local intent and print the session-resume command.
`decapod session acquire` and subsequent cloud operations can resume a headless
handoff. Completing authentication does not establish repository data access.

The optional `--cloud-repository` and `--github-users` flags and their config
fields describe requested context only. They do not purchase or assign capacity.
The native setup uses a server-returned account action URL; no fixed purchase
URL or commercial terms are compiled into the CLI. Source changes are not a
released client, hosted deployment or complete acceptance receipt. Those require
independently recorded proof of the actual artifact and deployed contract.

For an existing shared project, the agent may set machine-local
`DECAPOD_CLOUD_PROJECT_ID` to a server-issued reference before first setup in
that clone. The client requests current account status for that reference and
persists only a private routing binding. It does not create a personal project
as a fallback, silently switch an existing binding, or treat selection as access.
Workspace membership, grants, paid capacity and assignment remain server checks.

## Generic managed-session contract

A compatible consumer discovers a versioned public deployment profile, validates
the exact HTTPS service origin, and keeps the project publishable key separate
from the current user's access bearer. Discovery grants no user authorization.
Native data uses explicitly selected PostgREST RPC query and atomic batch routes;
supported legacy service routes retain their own envelopes.

Managed login/handoff, exchange, refresh and revoke must use the connector's
supported methods and the intended managed Auth endpoints. The client privately
persists PKCE/polling state and an in-flight marker before single-use exchanges.
It validates service/repository binding, expiry and handoff state. An interrupted
or lost rotation response is uncertain; the client must not replay a consumed
credential or overwrite a newer session.

The following records have different meanings:

| Record | Meaning |
| --- | --- |
| Managed human account | Stable identity independently validated by the service |
| Agent/work custody session | Local governed work and task ownership |
| Cloud authentication session | Managed user's current authentication lifecycle |
| Database connection/transaction | One transport or atomic storage execution context |
| Repository capacity and assignment | Service-owned capacity plus explicit authorized human/repository association |
| Historical decision session | Decision history, not an authentication credential |

A new access token must reach the next data request. If a Dactyl connection
captures its bearer when opened, refresh must replace or reopen that connection;
updating a token file alone is insufficient. Serialize private custody across
processes, use atomic replacement and restrictive permissions, and keep tokens
outside repositories and diagnostic output.

## Readiness and authority

Authentication completion means a session handoff succeeded. The client must
separately obtain server-confirmed account/project readiness and effective data
authorization. An invitation, incidental membership, requested roster, local
TOML entry, actor label or browser return cannot supply that authority.

The hosted service must check current managed identity/session, active native
organization/repository membership, applicable grant and effective capacity /
assignment at every authorization boundary. Session, membership, grant and
assignment revocation must deny the next operation even while a bearer remains
unexpired. A transaction already executing is not claimed canceled.

Repository grants distinguish read, write/contributor, maintainer and owner.
Capacity does not grant maintainer/owner, repair, source-control administration or
governance bypass. Expired or revoked direct contractor grants remain denial
fences over inherited permissions. Changing an agent label or local `actor_roles`
cannot restore cloud access.

Account administration and legitimately authorized recovery must remain distinct
from a data assignment. Service-returned action URLs are guidance only; the
service still verifies authority and readiness independently.

## Proof and failure boundaries

Required consumer proof includes origin/repository isolation, headless resume,
current bearer after refresh/reconnect, concurrent custody, live revocation,
physical read-only execution, ordered atomic batches, CAS conflicts and no local
fallback. A timed-out mutation may have committed: use durable operation identity
and reconciliation, without blind write/batch retries.

Distinguish synthetic HTTP fixtures, executing local database profiles, registry
artifact verification, clean consumer compilation, managed hosted authentication,
deployment and complete browser-to-CLI acceptance. Missing credentials/fixtures
are unavailable proof. A green unit suite, health probe or approval link cannot
establish the full chain.

Local SQLite use remains independently available without a cloud account or
network. A rejected cloud operation must not silently create local state.
