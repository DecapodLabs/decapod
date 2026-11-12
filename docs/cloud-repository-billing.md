# Cloud sign-in and account readiness

Select cloud storage with the same initialization flow used to switch an existing
local repository:

```sh
decapod init --backend cloud
```

Project configuration requires only the logical backend:

```toml
[repo]
backend = "cloud"
```

Decapod resolves repository identity from the checkout's Git origin. Project users
do not supply a deployment URL, provider, physical driver, copied token, price or
membership roster. Dactyl owns public discovery and all authentication, account
and cloud data transport. It returns the browser handoff for the selected service
origin; Decapod prints headless instructions and durably saves pending progress
and credentials outside the repository. Rerun the same command after signing in.
See [the managed cloud client contract](book/src/reference/propodus.md).

The hosted service owns pricing, account membership, repository authority, payment
and seat assignment. A printed URL, browser return or payment alone does not
establish access. The client keeps unresolved entitlement or denial explicit and
requires server-confirmed completion followed by an authorized repository read
before reporting readiness. A service or credential failure never initializes or
falls back to SQLite. Local selection works without cloud credentials.

The client consumes the exact published Dactyl 0.14.0 registry
connector/authentication/session API, retaining SQLite, Neon and default
Supabase capabilities. Adoption and the patched Quinn protocol lock floor
are tracked in Decapod #1638; HTTP/3 activation is outside this change. Offline fixtures do not prove real PostgreSQL, hosted authentication,
billing deployment, production activation or protected hosted acceptance. Those
service and release gates remain separate from #1628.

## Cloud access and task authority

These actor labels are cooperative local identities, not authenticated users.
For cloud repositories, a Decapod agent carries an authenticated user's bearer;
server-side Propodus grants determine its permitted operations. Cloud repository
access has `read`, `write`, `maintainer`, and `owner` tiers, with expiring
direct user grants for contractors. A client-selected agent ID or local
`actor_roles` entry cannot elevate cloud authority. A contractor is a bounded
delegation for the same agent interface, with no separate human CLI workflow.
Propodus rechecks expiration and revocation on each operation. An expired direct
grant remains an explicit denial over inherited group, team, or organization
access; changing an agent ID or local role mapping cannot restore that access.
