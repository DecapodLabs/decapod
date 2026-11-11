# Cloud init and paid repository approval

Cloud init collects the GitHub repository and requested named users before
generating context. Use either init surface, with headless flags when needed:

```sh
decapod init --backend cloud --cloud-repository example/repo --github-users alice,bob
```

The non-secret config is:

```toml
[repo]
backend = "cloud"
cloud_repository = "example/repo"
cloud_github_users = ["alice", "bob"]
```

Two users on one repository cost USD30 monthly. The owner/admin manages one
repository subscription. The payer need not occupy a seat unless they also use
cloud storage. An existing GitHub origin must match this purchase repository.
A new project can name its intended GitHub repository before configuring origin,
but the repository must exist when the admin approves it.

Init provides a hosted approval link and opens it for human interactive runs.
Headless, proof and dry-run runs print the link. The page signs in with GitHub
through Supabase Auth, verifies the numeric account identity and actual GitHub
repository admin permission, resolves the requested users, and displays the
roster and total before an explicit purchase. A non-admin gets the link to send
to their admin. Added names remain requests until that admin buys the extra seats.
Changing TOML does not confer a seat, admin status or cloud authorization.

The CLI stores no Stripe keys, purchase sessions, GitHub tokens or Supabase
credentials in project config. It does not treat a browser return as proof of
payment. Propodus owns Stripe Checkout, paid increases, webhook reconciliation,
repository-specific named seat records and authorization on each native RPC.
The generated specs include the repository name while preserving the authored
product summary. Refresh preserves the normalized roster.

The coordinated Propodus implementation is migration 009 and
`supabase/functions/propodus-onboarding/billing.js`. It requires a separate
restricted billing executor, Stripe monthly USD15 price/server key/webhook
secrets, Auth redirect configuration, deployment and sandbox acceptance before
this CLI change is rolled out. Init's purchase handoff replaces its legacy login
bootstrap; released Dactyl/Decapod native RPC credential and route support must
also be accepted before presenting paid cloud as usable. An approval link alone
does not establish a working cloud session.

Seat removal, subscription restart and refund/dispute policies are outside this
initial-purchase/add-seat contract and need an explicit lifecycle design.

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
