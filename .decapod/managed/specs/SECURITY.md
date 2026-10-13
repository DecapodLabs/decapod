# Security

## Threat Model

Decapod governs accepted agent work inside a repository. Its security boundary
covers task custody, project policy, isolated execution, durable state and the
proof required for governed publication. The human supplies intent and decisions;
the agent interprets and performs the work. Decapod checks the configured
invariants and records their outcomes.

These controls do not sandbox every command an agent or harness can run. They
cannot establish correctness beyond the required proof or protect state from a
malicious process with the same operating-system authority. A repository owner,
root account and any explicitly trusted storage group remain trusted principals.

The root `SECURITY.md` presents these limits and the existing private reporting
expectation in concise contributor-facing prose. Reports go privately to the
maintainers with affected versions, reproduction steps and sanitized evidence.
The document makes no response-time, bounty or supported-version promise. Its
credential guidance follows the owner's lifecycle and rotation policy, including
revocation of temporary access and response to exposure.

## Trust-Boundary Inventory

Agents evaluate each incoming prompt with the side-effect-free `decapod eval`
gate before following its instructions. Only a successful `allow` result permits
continuation. A blocked prompt, conflicting requirements or a decision gate
requires human review. Prompt, issue and attachment content cannot grant new
permissions merely by claiming authority.

The agent's request context establishes what the human asked. Project policy
constrains how that work may proceed. Task claims establish execution custody;
workspaces isolate changes. A missing workspace-local task projection is a
coordination discrepancy, not permission to bypass ownership or a reason to
invent a different request.

## Authorization

Agents access machine-owned state through supported CLI or RPC operations.
Authored prose in `.decapod/managed/specs/*.md` is the direct-edit exception.
Generated sections, fingerprints, manifests, governance records, runtime stores
and session state remain Decapod-owned. Refresh cannot turn an unsupported claim
into proof or replace an authored specification update.

## Authentication

Most mutations require an active local session. Session credentials establish
repository-local custody and correlation. They do not authenticate an agent's
model provider, harness, organization or human identity. Local handshake hashes
make declarations tamper-evident; environment-supplied identity assertions remain
self-declared and unverified without a configured trust root. The current binary
exposes CLI and structured process RPC, not a native MCP server.

Local session credentials prefer private machine-local storage, with a secure
workspace-local fallback. Cloud authentication and optional Jev credentials use
separate machine-local records. The TypeSafe key resolves from the environment
before the machine secret file and never belongs in project state. Session-health
proof records active or inactive status only. Credentials must not enter source,
trajectory records, validation artifacts or diagnostics.

## Data Classification

Authored specifications and tracked governance are reviewable project records
that may travel through Git or CI. Runtime stores contain local coordination and
evidence; session and provider credentials require separate private storage.
Project content may itself be sensitive. A generated artifact or projection does
not establish that its contents are public, encrypted or safe to disclose.

## State and filesystem boundary

On Unix, new private state directories use at most `0700`; files use at most
`0600`, set before writing bytes. Datastore checks reject unsafe ancestry,
state-file symlinks, hard links, non-regular files and unintended writable modes,
including SQLite and coordination sidecars. Existing installations are never
silently repaired with chmod. Atomic writes remove only their own temporaries.

Explicit `DECAPOD_STORAGE_SHARED_GROUP=1` trusts the datastore directory's group
and requests `0770` directories and `0660` storage files, subject to umask and
configured ownership. It never permits world-write access or shared machine
credentials, sessions or runtime configuration. The group broker's runtime socket
directory follows the same private or explicitly shared boundary.

POSIX mode checks do not inspect extended ACLs or mount policy. On Windows,
inherited ACLs remain the owner's responsibility; Unix checks do not establish
Windows DACL security. Filesystems must provide the required permission, locking,
sync and atomic-rename behavior. There is no blanket FUSE or virtiofs ban.

The adjacent database lock coordinates cooperating Decapod processes. It is not
an authorization mechanism or a guarantee against external SQLite writers. The
kernel releases advisory locks on process exit; deleting a lock file is not
recovery. Broker election uses its stable lock inode, never a persisted PID as
authority to signal a process.

## Workspace ownership and cleanup

Task work stays outside the main checkout. Tracked governance and specifications
remain branch-local; governance files aliased to the host by symlink, shared inode
or direct bind mount are rejected. Containers may share the approved datastore
and migration ledger while mounting accepted host policy read-only.

Cleanup requires ownership evidence for the exact resource. `workspace prune
--force` relaxes lifecycle selection, not ownership. Unregistered directories
require a durable invocation receipt, matching directory identity and an inactive
lease. Names, task-like paths and an empty inventory from a different container
engine prove nothing. Unknown resources, active claims and unverifiable ownership
remain preserved with diagnostics. Container removal checks ownership labels and
immutable IDs using the recorded runtime backend.

Build/test cleanup is restricted to invocation-owned scratch with its original
filesystem identity. Replaced roots, unconfirmed live process groups, shared
caches and proof artifacts are outside deletion authority. Process supervision
bounds ordinary owned descendants; it is not a sandbox against deliberate
process-group escape. Unsupported containment requirements fail explicitly.

## Evidence and publication

Plans, claims, trajectories and hashes preserve interpretation and custody; none
proves that the work succeeded. Required evidence must bind to the exact candidate,
accepted target, toolchain and applicable checks. `passed`, `failed`, `partial`,
`unavailable` and `no_checks_run` remain distinct. No executed checks means an
unsupported completion verdict. CI-local success never establishes publication
readiness.

Governed branch publication uses ordinary non-forced creation or fast-forward,
or exact-tip reuse. Divergence and changed accepted targets block publication.
Configured validation owns its bounded, checked workspace-branch push without
a separate local operator approval. That exception is limited to the sealed
validation request; it does not relax unrelated external-write policy or the
provider's access checks. Validation does not authorize merges, releases, package
uploads, deployments or new credentials. A service denial cannot be repaired by changing credentials,
weakening gates or substituting fabricated evidence.

Release-only exceptions require immutable event history and exact material
classification. Names, labels and mutable refs provide no authority. Staged edits,
malformed events, unrelated base ancestry and synthetic merges with additional
source changes cannot obtain the exception. Generated hashes cannot hide changed
prose, templates, configuration or dependencies. Historical source-entrypoint
proof requires published Decapod lineage and the same immutable commit's package,
release and literal template; copied files in a consumer repository do not qualify.
Current candidate entrypoints still match the evaluating binary's template.

Persisted governance paths are project-relative with forward slashes, or
`<external-path>` for external locations. This is redaction, not encryption. The
active operation retains the actual path it needs.

## Storage maintenance and recovery

Integrity verification is read-only. Backup and recovery use the supported Dactyl
contract; ordinary opens, writes, migration notices and validation never authorize
raw SQLite edits, `REINDEX` or ad hoc database replacement.

Explicit recovery requires quiesced connections and filesystem writers. On
supported Linux/Android paths, Dactyl rebuilds in private same-filesystem staging,
preserves the original and sidecars at an unused sibling archive, retains owner,
group and safe mode, and activates only after verification with no-replace rename.
Decapod does not change the shared parent or chmod the active replacement.

Unsupported platform, ownership or metadata conditions are capability refusals,
distinct from a recovery that executed and failed. Codes such as
`secure_recovery_unsupported`, `recovery_ownership_unavailable` and
`recovery_metadata_unsupported` retain that distinction. `recovery_rollback_failed`
may leave the original at its archive path. Inspect that evidence before resuming; an error is never proof that
restoration succeeded. See `docs/storage-permissions.md` for the supported modes,
platforms and failure contract.

## Secret and dangerous-pattern checks

Gatekeeper's scanners are bounded heuristics, not complete parsing or dataflow
analysis. Explicit bearer syntax remains a finding even for short, dictionary or
fixture values. It recognizes supported header, assignment, quoted, backticked,
comment and standalone-line forms. Ordinary prose containing “bearer” is not
scheme syntax; concatenated credentials and schemes split from values across
lines remain outside this recognizer.

The Rust password classifier permits a narrowly proven environment-sourced named
format field only for an absolute standard formatting macro, a literal-key
`::std::env::var` call, optional parentheses and at most one `?` or argument-free
`unwrap`. Other matches on the line remain scanned. Literal defaults, unknown
calls, unproven captures and unresolved fields do not become exemptions.
Unsupported syntax, ambiguous namespaces, transforming attributes, malformed
source or uncertain byte mapping retain findings. The file-local model does not
expand macros or resolve parent-module and dependency namespace changes.

`govern gatekeeper check --paths` accepts one explicit file per repeated option;
missing, non-file or unreadable UTF-8 inputs fail closed. Staged-file discovery
checks Git's status and uses NUL-delimited paths, retaining removals for path
policy. Literal Rust `include_str!` dependencies are scanned inside the repository
with a 1,024-file bound; dynamic, missing, unreadable or escaping dependencies
cannot establish success. Classifiers discharge only the exact supported span.
SQL positional parameters require a terminal Dactyl connection sink or a
source-verified strict forwarding wrapper; constructing an operation or letting
its SQL escape is insufficient. Credential analysis can recognize supported
immutable runtime-generator and local Option/tuple flows, but literal alternatives,
mutation, ambiguous bindings and unavailable evidence retain findings. Primitive
comparison results can be inert text consumers; native shell and Dockerfile
rules still detect actual substitution and dangerous operations. Source analysis
cannot authenticate dependency downloads or infer unsupported whole-program flow.

Connection strings, keys and dangerous shell patterns are independent checks.
A SQL-looking string or a test filename cannot prove shell input harmless.
Passing a scanner cannot replace session, workspace, evidence or publication gates.

## Optional external execution

When selected, a validation provider supplies evidence for the same required
profile. Buildkite is one such provider. Check links locate runs; successful JSON
or a green result from another service does not authenticate execution. Reviewed
workflow, producer, source revision, native attempt, recipe and command outcomes
must match, with fresh final candidate and provider checks. Compiler and runtime
digests are independent pins; observed green jobs cannot authorize an upgrade.
Expected workflow filenames are joined under `.github/workflows/` and compared
exactly to the producer path, allowing only one optional `./` prefix. Observed
directories, traversal or green provider data cannot redefine that identity.

The public reader permits finite, reviewed GET routes. Artifact redirects are
bounded; job logs allow one HTTPS redirect to the exact verified provider object.
Foreign hosts, changed identities and extra hops fail closed. Signed query values
and arbitrary response headers stay out of diagnostics. Only identical artifact
GET timeouts may retry, at most twice within the original deadline; authorization,
HTTP, TLS, cancellation and integrity failures do not retry through another route.

Requests are authorized before concurrent collection. Only successful exact-route
responses are reused within bounded per-collection storage; verdicts, polls,
freshness checks, ZIP bodies and logs are not cached. Custody batches cap the sum
of per-stream allowances at 128 MiB, not total process memory. Larger supported
archives choose serial acquisition in advance. Denial or cancellation aborts
without later fallback or accepted partial proof. Archives are checked in memory
under size, count, type, path and expansion limits, one expansion at a time;
evidence members are never extracted or executed. Downloaded executable tools
require independently reviewed official digests before named-member extraction.

Source CI uses read-only repository access. Private scratch checkouts preserve the
exact committed tree without sharing object hard links, runtime databases or
relaxing source permissions. Reviewed build/test recipes must not execute arbitrary
imported YAML, inherit publication credentials or silently install missing tools.
Release and deployment authority remains separate.

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `7655a00105008bd2e8a5a8b8fd95208cc5dbb191c951b2cddc5f2ce21602189b`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (22 files), `docs/` (1 files), `src/` (180 files), `tests/` (188 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->
