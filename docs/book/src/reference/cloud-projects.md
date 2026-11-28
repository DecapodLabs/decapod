# Managed Cloud repository and project routing

A repository administrator sets `[repo]` and `backend = "cloud"` in
`.decapod/config.toml` and authorizes contributor GitHub handles for that
repository in [Cloud Admin](https://decapodlabs.ai/admin). A contributor clones
the repository and gives their coding agent the engineering request. Decapod
resolves the Git remote and asks the Cloud service to activate the existing grant
for the verified GitHub identity. A trusted session can be reused. If GitHub
sign-in is needed, the agent presents the sign-in link and resumes the original
action. Missing or revoked approval requires the repository administrator.

Explicit project commands support separately authorized setup and project
management. Decapod stores an explicit routing override privately alongside its
repository-scoped session, outside Git. Local mode works independently.

When overriding or disambiguating across multiple projects after signing in with `decapod init --backend cloud`, an agent or a human can inspect available projects:

```sh
decapod cloud projects
```

Choose an exact project name, or a numbered entry from that saved directory:

```sh
decapod cloud project use "Team repository"
decapod cloud project use 1
```

Numbers refer to the exact directory previously shown for this managed account. A later directory reorder does not silently select a different project. Duplicate names require a numbered choice. The service rechecks the selected project; a selection cannot manufacture membership, assignment or data access.

For a new project, explicitly create it:

```sh
decapod cloud project create "New repository"
```

Creation uses the existing authenticated account API. It does not purchase access or elevate resource permissions. The command saves its request identity before sending the mutation. If a response is lost, rerun the same command and name to reconcile that request. A changed name conflicts while the outcome is unknown. A received creation result remains saved, so an immediate retry does not create another project. Do not restart with a different request to guess whether the earlier write committed.

A project may be selected while access is pending. Resolve that requirement in [Organization Admin](https://decapodlabs.ai/admin) (subscriptions, repository enrollment, seat assignments, teammate access policies, and billing) or review plan capacity at [Cloud Pricing](https://decapodlabs.ai/pricing) (**$50/contributor/repo/mo**, **$0 surprise overages**, and **multi-repo reservations**), or coordinate with your project administrator. Then retry initialization or the original command. The next operation checks current account/project eligibility and uses the native authenticated data route. A cached selection, browser return or valid bearer does not prove current access.

Project commands remain reachable before data eligibility. They display names and status; JSON output is available with `--format json`. Credentials, request custody and routing metadata stay in private machine storage. Repository configuration contains no bearer, refresh credential or authoritative project grant.

Reauthentication as a different managed account requires an explicit new selection. Revocation or denial stops the next authorization boundary and cannot silently select another project or SQLite. A transaction already executing retains its server-defined transaction boundary.

Existing explicitly supported canonical-repository native routes continue to work when no managed selection is recorded. Discovery and authentication still travel through Dactyl. Each command constructs its data connection after refresh, so the next request carries the current bearer.

Offline CLI fixtures establish routing and recovery behavior. Disposable PostgreSQL/PostgREST, released artifacts and protected hosted sign-in remain separate evidence; none is inferred from a local fixture or successful selection.

Cloud task retry receipts use a versioned private cache so older clients cannot
rewrite away a selected-project binding. Upgrade an older client when it refuses
the cache; keep the receipts for reconciliation. Legacy records migrate under the
existing lock, and unsupported future versions or oversized upgrades preserve
the original file. Deleting a receipt is not evidence that a timed-out write
rolled back. Local storage remains independent of this cloud-only cache.
