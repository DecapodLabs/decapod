# Cloud account readiness and repository access

Decapod owns the agent-facing workflow, repository context, local governance and
private credential custody. Dactyl owns generic connector, authentication/account
transport and storage execution. The hosted service owns managed identity
validation, effective repository authorization and authoritative capacities and
assignments. Commercial configuration and lifecycle decisions are outside this
public repository.

## Current consumer boundary

The source in `Cargo.toml` still consumes Dactyl 0.11.1. Its
`src/decapod/core/cloud_backend.rs` contains legacy onboarding/session endpoint
builders and a fixed approval URL. Printing or opening that URL is a handoff;
it does not establish a deployed account page, a managed cloud session, confirmed
capacity or successful database access.

The existing init flags `--cloud-repository` and `--github-users`, and the
`[repo].cloud_repository` / `cloud_github_users` fields, describe requested context.
They do not purchase capacity, resolve an authoritative managed identity or assign
access. Changing a username roster or matching a Git remote never authorizes a
cloud operation. Interactive init can open the existing handoff; headless/proof/
dry-run execution prints it. This behavior is narrower than completed setup.

Dactyl 0.12.0 exposes the newer logical connector, managed-session transport and
explicit native RPC route. Adopting that interface in Decapod is tracked in
[#1628](https://github.com/DecapodLabs/decapod/issues/1628). The installed consumer,
actual registry artifact and deployed service must satisfy the same versioned
contract before native cloud setup is described as usable. A Git/path dependency
or a source-only connector test does not prove the released consumer.

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
