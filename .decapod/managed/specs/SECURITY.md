# Security

Absence of `.git` does not establish checkout admission, including outside
`.decapod/workspaces`. `init --no-git` enables prerequisite diagnostics only.
The partial project-check route handles that compatibility explicitly; workspace
status and full validation retain their independent fail-closed boundaries.

## Diagnostic success grants no checkout authority (#1661)

Non-Git prerequisite diagnostics may inspect project configuration without a
task-bound Git workspace. They do not change `can_work=false`, mint a full
readiness judgment, or admit authoring/publication. Full validation continues to
reject that directory. CLI regression coverage exercises all three observations
using the real binary without Git-gate overrides. The custody transaction also
rejects second live bindings and rolls back binding persistence if event evidence
cannot be committed. Local actor labels and Git filesystem identities remain
host-local cooperative authority, not authenticated cloud or OS isolation.

## Agent-owned browser handoff

`authorization_url` exposes only Dactyl's validated, short-lived browser handoff.
It never includes the polling secret, verifier, exchanged code, access token or
refresh token. A task-add diagnostic preserves that handoff and its non-secret
operation ID without exposing task text or credentials. Recovery instructions
are fixed; raw authentication messages and recovery
text do not cross into task-operation diagnostics. Only the explicit validated
browser handoff field is carried for an onboarding-pending result. The agent performs an
explicit `cloud login --restart` after uncertain one-time outcomes; polling and
idempotent account confirmation do not authorize replaying those mutations.
The service remains authoritative for account requirements and repository access.

## Managed credential custody and server authority (#1628)

`core::cloud_session::SessionStore` is the only durable custody owner in the
managed CLI path. Its binding includes the validated service origin, exact
repository spelling used by the current service contract, and canonical main
checkout identity. The store rejects unsafe machine roots (including parent
traversal), paths within any Git checkout, symlinks/hard links, non-private
owners/modes, oversized records, invalid phase shapes, and binding mismatches.
Files use no-follow opened descriptors and synchronized atomic replacement;
refresh and rejection updates share a stable exclusive lock with a bounded
wait. Credential material and pending one-time secrets never enter Git.

`core::cloud_client` saves uncertainty before every one-time exchange or refresh
and never infers successful rotation after response loss. Explicit restart is
required for ambiguous one-time state. A rejection may invalidate only the
exact bearer used by that request, preserving a newer concurrent rotation.
Raw transport/service text and credentials are excluded from diagnostics.
The deliberately displayed browser handoff is validated by Dactyl and remains
a short-lived human sign-in capability, not repository authorization.

Dactyl alone performs discovery/auth/account/data transport in the ordinary
cloud path, with no redirect or ambient legacy token/provider adoption.
Decapod binds the private flow to the requested repository and validates the
handoff/session lifetimes. The published connector returns status/code strings
and a completion acknowledgement, without returning repository metadata to the
caller. Current access to the bound repository is therefore checked by the
authorized read rather than inferred from the sign-in result. Decapod does not
evaluate price, membership, seat/payment authority, or privileged grants. Service-confirmed
completion plus current authorized read is required before reporting setup
ready; every subsequent request still requires server authorization. Offline
fixtures cannot establish hosted PostgreSQL or managed-account enforcement.

## Historical validation trust boundary

`core::validation_history` accepts full execution evidence only from the
controller's completed runtime path. There is no command that imports a caller's
success JSON as validation authority. Records bind repository, immutable
candidate and original base, exact check plan and results, original task
contract, owner and lease lineage. SHA lookup is a bounded read-only operation;
malformed, oversized, ambiguous or digest-inconsistent records fail closed.

Historical completion independently checks integration and the complete supported
task lineage. Reassignment, renewed custody, intent edits later reverted,
missing history and concurrent task mutations cannot inherit an older pass.
It does not trust commit messages, branch names, a provider badge, or the legacy
tracked prerequisite receipt as proof of integration and successful execution.
Historical authority is limited to completing its original task; new publication
still requires current full validation. These local records retain Decapod's
cooperative same-user trust model: hashes detect inconsistent evidence but do
not protect against an unrestricted owner rewriting all state or impersonating
another local actor.

## macOS directory stream ownership

The bounded epoch reader opens an independent directory description relative to
a held descriptor, transfers it to `fdopendir` only on success, and closes it
through `closedir` on drop. Entries are classified with descriptor-relative
`fstatat(AT_SYMLINK_NOFOLLOW)`; observed paths are not reopened. Existing root and
ancestor no-follow checks, safe in-root file-link resolution, and resource bounds
remain enforced. The private stream owns its cursor and copies each entry before
the next `readdir`; EOF and read errors remain distinct. No trust, task custody,
cloud authorization or validation gate changes.

## Advisory intent reconciliation access

<!-- decapod:requirement REQ-INTENT-TRACEABILITY -->
`intent.reconcile` is a sessionless local inspection RPC over the existing
repository plan, canonical specs and source fingerprints. It bypasses mutation
custody and startup datastore writes, accepts an empty object or an optional
full immutable Git base commit ID, and adds no network access, credential
access or persistence. Filesystem read
permissions remain the access boundary. Reports expose repository paths,
requirement IDs, hashes and unresolved plan questions; callers must treat those
questions as repository content when sharing output. A digest is provenance for
observed bytes, not authenticated human approval. Markers, matching hashes and
review records cannot establish semantic correctness, resolve a pending human
decision or authorize validation/publication; `semantic_alignment` remains
`not_proven`. The existing mutation and publication gates remain authoritative.

## Immutable impact Git trust boundary

<!-- decapod:requirement REQ-INTENT-CHANGE-IMPACT -->
The optional base-to-HEAD impact reader invokes bounded, repo-local Git
inspection on immutable commit objects with source replacement and inherited
Git directory/worktree/index overrides removed. It does not fetch network
data, execute repository hooks, infer code-to-requirement authority, disclose
source contents, or write repository state. Exact ancestry and complete
path accounting are required for complete observations; unsupported paths and
truncated results remain explicitly unproven. An uncommitted working tree is
outside the immutable comparison, and unresolved questions remain visible.

## Shared policy command regression

The Bazel release-download retry configuration uses only the official
`mirror.bazel.build` and `github.com` hosts, adds no credentials, and preserves
every repository rule's original pinned archive digest. Mirror samples were
checked against registry metadata bound by the committed Bazel lock. An alternate URL or
successful transfer cannot authorize different bytes or replace execution
evidence. Existing lockfile enforcement and publication gates remain required.

`tests/cli_contracts.rs::claim_boundaries` verifies that candidate-only policy
changes cannot enable shared claims, canonical opt-in preserves basic trust,
and the actual policy audit records the secondary caller, primary label and
config digest. Read-only datastore observation checks the stored trust and
audit event. Disabling canonical policy prevents new shared members without
changing primary ownership. These tests do not turn cooperative caller labels
into authenticated principals or authorize a trust-grant API.

## Execution identity and replay safety

Generic replay normalization masks only execution identity in named readiness
receipts: request IDs and binding evidence digests, plus the report evidence
path. Material revisions, model/governance/proof bindings, modes, decisions,
blockers and unknown fields remain significant. The normalizer is unchanged by
model retirement, preserving historical replay behavior without model-specific
code. Complete receipts remain intact in persisted historical evidence.

## Rust source-spelling fixture isolation

## Cooperative shared-claim opt-in

The optional basic-trust shared-claim setting is a cooperative policy, not an
authenticated trust grant. Same-user agents that can edit main config can
enable it, and caller labels can be impersonated. The runtime refuses the
opt-in if main-root resolution fails and does not use the candidate config for
the shared datastore. A bounded regular-file config snapshot is taken before
the write transaction; an in-flight operation can use its earlier snapshot.
Its transactional audit records the consumed config fingerprint, not proof of
human editing or verified identity. All other trust and approval gates remain.

## Broker caller context custody

`group_broker::CallerContext` transports only a fixed set of request-scoped
fields across the existing permission-restricted local socket. It is not a
general environment-forwarding facility. The subprocess clears inherited
identity/session fields before applying the caller values, retains ordinary
session/policy validation, and does not grant authorization based on request
metadata. Existing cooperative local-agent authentication and auto-acquisition
remain unchanged; this fix does not authenticate hostile same-user processes.

Session passwords remain ephemeral transport inputs, excluded from request
Debug output, replay hashes and durable request records. Dedupe binds actor and
working directory with non-secret context to prevent another caller's command
from accidentally reusing the leader's result. Restricted socket access remains
the existing trust boundary, not cryptographic isolation between local peers.

The required dependency check exposed RUSTSEC-2026-0386 in the accepted Tokio
1.52.3 lock entry. The minimal lock-only update to patched 1.53.3 changes its
version and registry checksum without relaxing advisory policy or changing
dependency edges. Fresh exact-candidate provider proof is still required.

## Native receipt and dependency repair boundaries

Native success receipts are produced only after the complete selected proof and
required readiness decision, bound to exact candidate/target and governance.
They cannot be imported to authorize publication or replace execution. Atomic
private completion staging prevents partial receipt/verdict writes from exposing
a successful pair; no tracked candidate bytes are changed by receipt output.
The root Cargo.lock updates Tokio1.52.3 to1.53.3 for RUSTSEC-2026-0386, changing
only that package's version and registry checksum. This addresses the actual
required dependency-check failure without suppressing advisories or modifying
other dependency edges. The pinned Bazel9.3.0 module dependency generator also
refreshes `MODULE.bazel.lock` from that Cargo input; byte review permits only
Tokio references/version/checksum and the Cargo input digest. Generation changes
no trust configuration, proof profile, registry or unrelated dependency graph.
It is not a claim that full validation has passed.

Public Buildkite artifact redirects admit only the exact documented HTTPS
`buildkiteartifacts.com` host and legacy `s3.amazonaws.com/buildkiteartifacts.com`
bucket prefix, with unchanged four-UUID layout, exact producer-job binding,
content hashes, byte limits and original deadline. HTTP, explicit ports, userinfo,
lookalike hosts, malformed producer identities, fragments and encoded traversal
reject before following. Signed query strings remain opaque and never enter logs,
evidence or Git. Buildkite documents the current host in its
[network requirements](https://buildkite.com/docs/agent/self-hosted/security/network-requirements).
The fix grants no arbitrary storage, credential, publication or candidate
self-authorization capability. The original strict-collection failure remains
failed until an independently supported controller can execute that route.

The verification-only scalar F64 reference verifies immutable model/config hashes and calculates expected values without candidate tensor/model math, while sharing generic infrastructure, weights and authorship. Candidate `30015be0` executed three fixed 138/314/4,096-token Metal/CPU comparisons, the exact token boundary and all 16 memory slots; this does not establish all-input equivalence or independent human review. Raw tensors stay in bounded private exports. The final required scientific command rechecks referenced bytes and comparisons without importing earlier-run artifacts. The complete v2 collection nevertheless produced 23 false-ready proxy outcomes per backend. Proxy labels, incomplete supported coverage and 23 source lineages retain the unsatisfied readiness-calibration gate.

The new bounded plan projection preserves explicit obligations without treating
plan gate flags as proof. Its overflow path abstains; final governance identity
checks reject changed intent or obligations. The verification-only public review
capture contains source facts, not private sessions or controller evidence, and
is excluded from the published crate. Its Rust audit pins bytes and checks exact
review/commit binding, timestamps, edits and complete pagination. Selected review
verdicts and rationale are excluded from prospective model input; future CI is
not backfilled. Public `User`/maintainer identity is not proof of unaided human
authorship, and owner grouping is not a guarantee of statistical independence.
Structural eligibility alone never grants a calibration or publication pass.

## Explicit local task-transfer trust

The separate worker's vendored Metal subpackage preserves its exact upstream base/tree and explicit three-barrier source modification. Patched shader identity is separate from the upstream revision; the standard Apache text corrects a prior project-specific appendix without inventing an upstream holder. aa66c8e4 actually passed its repeated primitive and two model fixtures after earlier measured Metal failures. The new scientific collection context is ephemeral evidence bookkeeping, not a task control plane or authentication against a malicious worker/same-user forgery. Exclusive files, current-directory/runtime/model/corpus binding, no-follow single-link reads and strict final aggregation reject accidental replay/mixing, altered labels, inconsistent decisions and missing coverage. A collection-only successful exit cannot waive the last required command or any kernel gate.

`src/decapod/core/task_transfer.rs` accepts source custody only with an independently
supplied artifact digest and explicit operator acceptance. Git origin and agent
names are local bindings, not authenticated remote principals. The operator must
verify source yield/export through a trusted channel and ensure cooperating source
writers use the supporting controller. No signature, source liveness check or
global exclusivity across disconnected clones is implied.

The source task is sealed under the normal journal transaction before private
artifact publication. Only its previous yielding actor can export. Import refuses
existing identity/hash/history and replay, validates source chain and receiver,
and retains source proof solely as history. Current proof and pass timestamps are
cleared. The artifact excludes session tables and unrelated tasks, but task text
can itself be sensitive and must remain outside Git. Older binaries and privileged
writers are outside this cooperative local fence.

## Agent-delegated authority, local and cloud

The agent is the only operational caller. Human owners, maintainers,
contributors, employees, and contractors express intent to their agent; the
agent calls Decapod within the same repository-native work lifecycle.
Local developer mode retains cooperative actor labels and requires no cloud
authentication. These labels provide audit attribution and task custody
within the local user trust boundary, not OS principal isolation.

Cloud authority instead belongs to a verified Propodus principal and is
evaluated on the server against a live managed session, organization
membership, repository grant, and entitlement. The agent ID, local RBAC
mapping, task owner field, and asserted role can never authorize a cloud
operation. Access tiers distinguish read, contributor/write, maintainer,
and owner. A contractor uses an expiring direct user grant within one
repository, including an explicit expired-deny fence over group/team/org
inheritance. Only the schema owner can issue or revoke that delegation.

Cloud task custody binds the executing authenticated principal separately
from the agent's freely chosen collaboration label. Release/complete of
another principal's claim must fail even if the caller copies the exact
label. Reviewed maintainer/owner repair is an explicit, bounded exception.
An agent's self-asserted success or authority cannot become proof or
publication permission. Live service assertions are required; synthetic
client fixtures establish routing only.

## Threat Model

Decapod governs accepted agent work inside a repository. Its security boundary
covers task custody, project policy, isolated execution, durable state and the
proof required for governed publication. The human supplies intent and decisions;
the agent interprets and performs the work. Decapod checks the configured
invariants and records their outcomes.

### Provider-attested validation trust

The accepted CI-attested policy trusts the configured Buildkite executor and its
HTTPS metadata for the required validation results. Pipeline Trigger mode permits
exactly one keyless importer on the independently bound pipeline UUID. Its label
and count are consistency checks; the provider API does not prove plugin identity,
plugin version, or unchanged remote pipeline settings. Policy acceptance explicitly
includes that executor trust. The actual provider job UUID is retained under a
reserved bootstrap identity; no synthetic artifact or toolchain digest is claimed.
Branch-dependent concurrency keys are independently derived from reviewed source
and expected event context. Additional unkeyed jobs, unknown keyed jobs, retries,
failed checks and source/run mismatches cannot be accepted as green. A candidate
cannot change the policy used to authorize its own publication.

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

## Recovery cannot grant authority

Runtime checkpoint payloads preserve domain state and references, never session
passwords, environment dumps, credential values or provider tokens. Immutable
operation IDs, predecessor digests and generations prevent replay from silently
choosing a different successor. A valid content hash proves identity, not an
approval or the completeness of missing history.

A workunit's projection can be repaired without claiming its evidence is current.
Execution resume must recheck the accepted intent, material, plan and applicable
policy/approval boundary; revocation and expiry cannot be undone by restoring an
older checkpoint. Task proof bytes retain their original task-revision binding.
A contradictory live projection is preserved for explicit recovery rather than
overwritten by the journal's last timestamp. Git remains the authority for
accepted authored governance, and full validation still gates publication.

Execution capture and proof hashing bound file counts, bytes and elapsed work.
On Unix the shared reader pins the repository root and opens path components
without following directory symlinks, rejecting special files before reading.
Bounded regular-file symlinks must resolve within that root. Governance documents
are capped at 16 MiB before parsing. Unsupported execution-capture platforms
fail closed; atomic visibility alone does not imply durable continuation.

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

The Nix binary cache (#1177) is a release-publication boundary. A
credential-free `publication-owner` job requires the native GitHub-hosted
identity and an actual release-merge push to `master`. Buildkite imports,
manual dispatches, ordinary merges, pull requests and forks cannot pass it.
Only the downstream `publish` job statically references the repository's
`CACHIX_AUTH_TOKEN` secret, after the owner and credential-free build job
succeed. Missing or rejected write credentials fail the publication. Each
publisher independently rebuilds from the committed lock, verifies its native
system, toolchain and smoke check, and on Linux proves an identical rebuild.
No unverified derivation is intentionally uploaded. Consumers trust the
`decapod` cache signing key and its authorized CI builder; the public key is
never a write token. `x86_64-linux` entries can be checked independently with
`nix build --rebuild`. `aarch64-darwin` entries cannot yet, because the linker
UUID can differ.

## Authorization

Agents access machine-owned state through supported CLI or RPC operations.
Authored prose in `.decapod/managed/specs/*.md` is the direct-edit exception.
Generated sections, fingerprints, manifests, governance records, runtime stores
and session state remain Decapod-owned. Refresh cannot turn an unsupported claim
into proof or replace an authored specification update.

Declared scope and committed instruction integrity are checked independently of
provider execution results. These checks detect a candidate that violates an
explicit forbidden path/file budget or contains a noncanonical instruction
rewrite in its history, even if its final code tests pass. They do not prove that
an agent's recorded interpretation exhausts the original human request.

A new trajectory ID cannot erase an uncommitted failure journal through the
supported initializer: replacement requires the prior complete governance
document in Git. This protects recoverability for cooperative CLI/RPC writers;
it does not authenticate the truth of agent-authored observations or prevent a
principal with unrestricted filesystem/Git access from rewriting history.

## Authentication

Most mutations require an active local session. Session credentials establish
repository-local custody and correlation. They do not authenticate an agent's
model provider, harness, organization or human identity. Local handshake hashes
make declarations tamper-evident; environment-supplied identity assertions remain
self-declared and unverified without a configured trust root. The current binary
exposes CLI and structured process RPC, not a native MCP server.

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

Automatic container cleanup does not equate directory ownership with permission
to destroy unfinished work. Failed executions retain all clone files, including
ignored diagnostics, and cannot invoke push or PR creation. Successful PR requests
use the common exact-candidate governed publisher; child success and CI-local
validation cannot substitute for publication readiness. Local-only execution
cannot select a stale host branch for publication. Foldback never force-updates
a divergent host branch.

Successful disposal requires branch foldback, the expected attached HEAD, an
index without hidden entries, unchanged inherited refs outside the returned
branch, and no dirty tracked or non-ignored untracked files. New or changed
stash, side-branch and tag refs preserve the clone. Raw OID comparison includes
annotated-tag objects; index inspection preserves flags rather than clearing them.
The shared status check forces untracked visibility and submodule inspection.
Ignored build outputs remain disposable after successful otherwise clean runs.
Reflog-only/dangling objects, deleted ref names, arbitrary external writers and
races after inspection are outside this bounded guarantee. Retention does not
copy files into Git or proof. Review retained work before explicit prune; these
automatic-clone checks do not broaden the separate explicit-prune contract.

Build/test cleanup in `assets/tools/build_lifecycle.rs` is restricted to
invocation-owned scratch with its original filesystem identity (`Scratch::verify_root`).
When Unix `fs::remove_dir_all` fails with `PermissionDenied`—such as rootless
Podman overlay `work/work` mode-`0o000` or read-only layer directories created
under a test's disposable `HOME`—`Scratch::clean` runs bounded permission
recovery (`Scratch::recover_owned_permissions_with_bounds`) before a single
re-verified removal retry. Recovery locks the verified scratch root to `0o700`,
inspects entries via `fs::symlink_metadata` without following symlinks, rejects
replaced directories, inode drift, foreign owners (`uid != identity.uid()`),
cross-device mounts (`dev != identity.dev()`), and trees exceeding 128 levels
(`MAX_PERMISSION_RECOVERY_DEPTH`) or 65,536 entries (`MAX_PERMISSION_RECOVERY_ENTRIES`),
and restores `0o700` only on verified owned directories. `cleanup_diagnostics`
remains strictly read-only. Replaced roots, unconfirmed live process groups,
shared caches, symlink targets and proof artifacts remain outside chmod and
deletion authority. Process supervision bounds ordinary owned descendants; it is
not a sandbox against deliberate process-group escape. Unsupported containment
requirements fail explicitly.

## Evidence and publication

Plans, claims, trajectories and hashes preserve interpretation and custody; none
proves that the work succeeded. Required evidence must bind to the exact candidate,
accepted target, toolchain and applicable checks. `passed`, `failed`, `partial`,
`unavailable` and `no_checks_run` remain distinct. No executed checks means an
unsupported completion verdict. CI-local success never establishes publication
readiness.

Persisted task proof is scoped to an exact task identity and revision. A stored
`pass` label without matching subject custody cannot unlock dependencies,
exclusive completion, work-claim completion, or aggregate readiness. Verification
result writers compare their original task and baseline inside the mutation
transaction; an old verifier cannot bless or invalidate a newer recapture.
Missing historical bindings require actual recapture, not a migration that
labels old evidence current. Revision binding is a repository-local integrity
check, not authentication of an external producer or a claim of cloud parity.

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

## Reusable local connection custody

The opt-in storage owner retains the existing cooperative private/shared-group
socket boundary; caller metadata is attribution and is not an authenticated
claim of OS isolation. The existing #1476/#1485 command-child context replacement
continues to determine actor/session values used by policy and task writes.
The physical worker receives no session password or inherited actor identity.
Its protocol accepts only the canonical datastore path and bounded frames.
Read-only leases are enforced on the server, unsupported savepoint controls
are rejected, and every lease boundary attempts physical rollback. Known SQL
errors preserve their category; transport ambiguity poisons the proxy instead
of authorizing replay or another local execution path.

A mandatory parent-owned lifetime pipe and supervised worker shutdown bound
ownership; parent loss ends the worker even during SQL. Normal shutdown drains
an established lease for at most five seconds without admitting new leases,
then kills/reaps a worker that has not exited. Migration/restore and
physical ownership use the same stable sidecar lock inode. Backups must neither
copy nor replace runtime lock/election files or broker runtime directories.
No credential, daemon registration, new external audience, or remote service
is introduced. These are candidate invariants with regression coverage authored;
execution evidence remains required before publication readiness.

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `e0bd9605408db7af91f50878edf59777bd9ff32b5dd9932cb2e5b5fdae7acfb9`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (38 files), `docs/` (1 files), `src/` (234 files), `tests/` (233 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

## Inventory privacy and authority

Native inspection uses caller OS authority, the existing lifecycle lock and a
Dactyl read-only snapshot. It bypasses initialization and does not replay
migrations or grant hosted access/paid capacity. Raw values, task IDs, principals
and content fingerprints stay private. Event classes are fixed public labels.
Unknown formats, skipped JSON, unsupported virtual tables and exhausted budgets
remain explicit. Header recognition does not verify compressed bodies, proof
chains or safe deletion. Accepted managed-cloud authority remains server-owned.

## Quinn transitive dependency remediation

The exact published Dactyl 0.14.0 registry dependency removes the older SDK
constraint that retained quinn-proto 0.11.9 in the inactive HTTP/3 lock graph.
Repository and fresh registry-consumer metadata now resolve quinn-proto 0.11.19;
the release-policy regression rejects any older locked entry, including inactive
ones. The configured dependency audit must still evaluate the exact candidate
without ignored advisories. Metadata resolution and an upstream release do not
establish current-source or hosted security proof. The generated Bazel lock must
agree with Cargo, and release-bound projections must match the evaluating release.
HTTP/3 remains disabled under the existing SDK feature configuration. Inactive
entries retain the same dependency audit requirement. This adoption changes no
transport policy, CI trigger, provider permissions or accepted-executor helper.

The reviewed provider importer identity is buildkite-gha 0.102.7. The validator
accepts exactly its independently reviewed official compiler and platform-specific
executable digests in `ReviewedBuildkiteImporter`; observed green jobs do not
select or authorize versions. The 0.102.5-to-0.102.7 source review covers diagnostic
redaction and error reporting, including the workflow-concurrency/output-derived
runs-on classification change from `E_EXPRESSION_INVALID` to `E_GRAPH_INVALID`.
Production runtime execution, serialized plan models and v2 schemas are unchanged.
Linux x86_64 release asset 642891563 and Darwin arm64 asset 642891564 were checked
against official archive digests before hashing their named executable members.
Old 0.102.5, observed 0.102.6, unknown versions, mixed tuples and swapped platform
identities remain rejected. Source, plan, runtime, artifact and accepted-executor
verification remain required. There is no provider-setting change, dynamic latest
acceptance or automatic controller bootstrap in this repair.

## Reviewed repository command boundary

Repository-local proof is admitted only for the exact supported profile after both
source and command/execution contracts match their reviewed digests. Its capability
permits reviewed native language/build executables without relaxing the existing
compiled Decapod profile. Program arguments are passed separately; workflow YAML
and shell text are never imported as commands. Direct shell, privilege/container
and recursive validator entrypoints remain denied. Cleared environment and owned
scratch avoid ambient credentials; declared installed bytes are measured around
execution. This is bounded execution and custody, not an OS sandbox or a proof that
arbitrary repository code has no external side effects. Review must exclude daemon,
network, external writes and publication operations. Unsupported behavior blocks
rather than producing a local substitute for required provider evidence.

Repository command authorization is a sealed exact-invocation operation minted
by the validated host, distinct from the generic executable allowlist. Existing
authored proof policy continues to deny disallowed programs; missing relevant
permissions in a present policy and malformed policy fail closed. No policy
file is rewritten as a repair, and generic callers cannot request the sealed
capability without the reviewed contract and measured invocation.

### External engines remain outside local proof

The native local-command boundary has no Dagger engine exception. Direct Dagger
invocations are rejected; retired Dagger execution and authorization fields are
unknown configuration errors, not silently ignored checks. This does not make
the local runner an OS sandbox: reviewed commands remain subject to its existing
external-access and non-daemon constraints. Agents own external engine tooling
and its permissions separately; arbitrary external CI results are not imported
as native proof. Buildkite custody and final readiness remain unchanged.

The root Cargo lock remediation changes only Tokio 1.52.3 to 1.53.3 and its
registry checksum. The derived Bazel lock must be regenerated under the pinned
Bazel toolchain before publication so dependency identities agree. Dependency
resolution is not audit/test success; required hosted checks remain mandatory,
with no ignored advisory or weakened validation requirement.

## CPU companion and pre-inference startup custody

Both local executable pins are frozen and verified with native-format, owner,
mode, single-link, bounded-size and SHA-256 checks. Setup verifies the optional
CPU companion before replacing its private binding; no shared policy path or
implicit executable discovery selects it. An absent ELF interpreter or empty
exit 127 can select only an explicitly pinned CPU companion, before a model
request is supplied. Typed hardware-only startup failures may select the same
worker's CPU path. Malformed capability reports, other exits/signals, timeout,
compute failure and negative/abstaining/malformed inference remain non-ready.
Attempt phase/runtime hashes distinguish startup from inference. A registered
native executable remains operator-trusted code; headers and self-reports are
not independent build provenance, dispatch, numerical or calibration evidence.

## Supplemental Rust worker verification

Supplemental Rust-worker proof preserves source-CI authority. It cannot execute
before a complete strict source verdict, replace that graph, import caller receipts
or authorize its own controller adoption. Current-source configuration, committed
manifest/lock, immutable snapshot, measured installed tools and fresh target reads
bind execution. Accepted recipes cannot be omitted or rewritten by candidate
policy. Typed host authorization remains required for both tool probes and checks.
Only fixed read-only xcrun/xcode-select probes are added; compiler execution,
developer-directory changes, arbitrary tools and extra probe arguments are rejected
by that capability. Actual Cargo execution uses the existing proof-exec boundary.

Native execution uses fresh private tool state and no ambient credentials or PATH
inheritance. This remains the existing reviewed-source model, not an OS sandbox
against malicious build scripts sharing the account. SDK metadata/tool hashes do
not attest all dynamic libraries, driver state or GPU behavior. Private output
objects retain existing permission/link defenses and byte bounds. A worker's
successful exit or self-reported backend cannot by itself establish model parity,
calibration, physical dispatch or publication authority.

### Fixture alias custody (#1457)

The reviewed fixture exception permits only a fixed source path and literal `target.txt` link payload with a separately measured regular file under the same fixture directory. It does not follow a caller-selected path, permit chains, accept a symlink target, skip sensitivity scanning or bypass denied paths. Committed, staged and working material must each satisfy its relevant alias/target checks, preventing a valid working link from laundering a malicious staged or committed link. Existing independent identity remeasurement, source immutability, private scratch, process and byte budgets remain required. This follows the existing reviewed toolchain-alias trust model; it is not an OS sandbox against concurrent malicious filesystem mutation or candidate code.

GPU one-shot ownership consumes the entire model before inference and discards it after synchronized completion, including failures, avoiding unsupported CUDA-only global reset and any uncleared-state reuse. The worker does not expose a reusable cross-request cache. The independent collector preserves every phase error and returns nonzero unless all required phases pass; malformed or missing private receipts fail closed. Sharing the strict typed CPU retry classifier with diagnostics adds no gate authority: inference errors, ordinary abstention, malformed IPC, stale assets or disabled fallback still cannot authorize another attempt. Real alternate-API unavailability in a diagnostic is not evidence that a working preferred GPU failed or that the full kernel publication/custody protocol ran.

### Public diagnostics and semantic evidence limits

The historical completed76 validation exposed false-ready outcomes against proxy labels and unexecuted/token-overflow cases on both backends. Those failed results remain preserved under their original criterion, which was superseded by the explicitly approved functional delivery scope in #1445/#1460; they are not passing accuracy evidence or a revived statistical publication prerequisite. The reference's zero-memory omission is repaired from independently pinned specification/config and own scalar block outputs, with analytic anchors rather than candidate-derived expected values. Shared authorship/parser/weights and512-token scope still limit independence. Actual CUDA-unavailable-to-CPU component execution does not establish preferred-Metal failure or full production custody/fallback routing. All raw numeric inputs and receipts remain private outside Git.

Public provider metadata and logs may diagnose a failed bootstrap without credentials. They remain untrusted diagnostic content: reading a sibling success or downloading a failure log cannot import a passed observation, alter the required graph or grant publication. A pre-parser helper launch failure establishes no source-test or native execution. Provider recovery must preserve failed history and bind the next real execution to the exact ordinary branch candidate. No authentication state, raw machine logs or model inputs belong in Git.

## Immutable configuration bootstrap trust boundary

The installed controller independently pins accepted CI carrier bytes. This PR
does not change those actions or replace their execution with an unmerged
controller. A diagnostic Rust parser migration was prepared privately but is
not activated or counted as current production proof. Existing immutable source
context and accepted executor checks remain authoritative. New actual-command
release tests execute the Rust candidate in owned temporary repositories, keep
session credentials outside Git, and distinguish bounded project prerequisites
from full publication readiness.

## Dependency advisory remediation

The separate native worker pins Parquet 59.3.1, whose reader no longer depends
on Thrift. This removes the Thrift 0.17.0 dependency reported by Dependabot #4
(GHSA-2f9f-gq7v-9h6m), rather than suppressing or dismissing the advisory.
Maintained Parquet 57/58 releases still require the affected dependency. The
existing pinned corpus size/hash checks and reader remain the execution boundary;
dependency resolution alone does not prove successful corpus decoding or model
execution. The model, tokenizer, engine, calibration and Git runtime pins and
numerical tolerances are preserved.

Scientific collection test fixtures explicitly request owner-only directory
permissions at creation instead of depending on the invoking shell's umask.
Production collection ownership/permission and unsafe-link checks are unchanged;
a public-directory regression verifies both creation and loading fail closed.

Compatible rustls-webpki 0.103.16 and webpki-roots 1.0.10 updates in the root and
worker locks address RUSTSEC-2026-0382 and RUSTSEC-2026-0381. Bazel metadata is
regenerated from the normal root Cargo lock and checked in strict lockfile mode.
Required hosted dependency checks and full validation still govern publication;
neither a lock update nor a public CI log is a passing receipt. Prior failed
validation and provider bootstrap/metadata rate-limit errors remain recorded.

## Approved ownership recovery

Handoff resolves policy from the actual caller, never from the optional expected
owner or intended receiver. A basic caller cannot acquire operator authority by
putting an operator label in either field. The transactional handoff gate uses
the same existing local cooperative operator trust interpretation as the broker;
other task operations and stored trust levels are unchanged. These role labels
are self-declared local authority, not OS authentication or remote credentials.

Cross-owner recovery requires explicit expected owner and an exact task-bound
live approval, consumed atomically on successful custody transfer. Prior owner,
actual actor, receiver and consumed approval IDs remain in the task event. Wrong,
expired, broad and already-consumed grants cannot authorize this recovery, and
failed preconditions do not consume authority. Dependency, lease, overlap and
completion-proof gates remain independently required. The new regressions are
unexecuted until official candidate validation succeeds.

Recovery never requires a human to execute Decapod beyond `decapod init`.
The AI agent/harness obtains the human decision and records its exact bounded
approval before acting under its already established authorized request identity.
Approval attribution does not elevate the executing caller; a harness must not
self-select a reserved role, promote its trust, or impersonate the previous owner
to evade a denial. Existing trust, exact-target single-use approval and custody
checks remain unchanged; this is an execution-contract correction, not new authority.

## Prompt context and HTTP diagnostics

The side-effect-free evaluator in `src/decapod/core/agent_eval.rs` distinguishes
an ordinary HTTP client reference from unsafe execution context: a read-only
public request in a diagnostic, API example or restored command history is not
itself a remote-execution finding. The original bytes remain the hashed subject,
and CLI responses never echo the prompt. Allowing inspection does not authorize
network access or command execution; those actions retain their normal gates.

This is a conservative common-marker detector, not a shell interpreter or a
semantic proof of benign intent. It inspects the entire prompt, including quoted,
fenced and historical material. Such labels grant no authority or exemption.
Known destructive/privileged actions, instruction overrides and credential
requests remain blocked. HTTP clients combined with shell execution, pipelines,
chained commands, process/command substitution, upload/authentication options,
credential headers and non-read-only request methods require review. File-output options are conservatively review-gated too: writing a fetched
program and invoking it on a later line must not escape detection merely because
the executable has an unfamiliar name. Ambiguous combinations may still require
review rather than silently suppressing evidence.

HTTP-specific context is bounded by inline-code and unquoted line boundaries;
every span is inspected, and adjacent shell operators and multiline substitutions
remain relevant across those boundaries. Quoted shell argument newlines remain
within the argument context. This prevents a prose semicolon after a closed
inline diagnostic, or an unrelated mention of Python/source, from inventing an
execution edge. It is deliberately not a general prose/shell grammar: unmarked
same-line prose and ambiguous shell syntax can still require review. Dynamic request arguments (shell command/process substitution, parameter
expansion and embedded backticks) are review-gated before lexical normalization
can erase their delimiters. This does not classify an arbitrary variable or
literal public query value as a known secret; it preserves uncertainty about
values that may be computed from local data. Dollar signs in prose outside a
closed inline diagnostic do not become request arguments. Existing whole-prompt
instruction/authority/destructive/credential markers remain intact.

HTTP options are classified through a narrow read-only allowlist, with
client-specific short flag meanings and existing method/header checks. Unknown
options request review rather than inheriting a benign classification: client
versions can add file-reading and opaque configuration features. Explicit static
`--url-query name=value` pairs are classified, but file-backed query arguments
are not. The [curl option contract](https://curl.se/docs/manpage.html#--url-query)
defines `name@filename` as file-backed input; attached header-file values and
variable/expanded-option mechanisms likewise remain review-gated. This bounded
classification deliberately does not promise a complete curl/wget interpreter.

`tests/unit/core/agent_eval_tests.rs` and `tests/agent_eval.rs` specify safe
report/history and adversarial combinations, including spacing, quoting and
escape variants. The public sanitized report is covered; the reporter's private
historical summary was not supplied and has not been reproduced. These authored
regressions require execution under full candidate validation before this change
can claim publication readiness.

The shared CI repair updates root Tokio from 1.52.3 to 1.53.3 for
RUSTSEC-2026-0386 and synchronizes its derived Bazel lock metadata. Both deltas
were reviewed and their exact before/after hashes verified locally; pinned Bazel
9.3 generated the shared metadata with the existing system JDK and normal TLS.
No required graph, trust, model, or execution policy changes. This dependency
repair is not runtime proof: the prompt-gate candidate still requires fresh full
validation against its exact published commit.

## Precise claim paths and unchanged trust

The bounded file-scope change addresses the disjoint-work portion of issue 1480.
Trust-grant administration remains unresolved and is not added by this change.
There is no new config trust mapping, elevation API, trust-row mutation or human
CLI requirement. Existing caller names and local approvals remain cooperative
assertions, not authenticated principals or independent proof of human consent.

`fleet_coord::declared_paths` recognizes bounded ASCII literal JSON file-set scopes.
Explicit footprint matching conservatively folds ASCII case on every platform;
Unicode scopes are rejected rather than assuming filesystem alias semantics.
`todo::add_task` rejects traversal/symlink scopes and avoids fuzzy consolidation
of explicitly scoped tasks. Claim/handoff overlap checks, fleet diagnostics and
`work_claim::from_todo_with_lease` use the same footprints. Legacy/invalid scopes
remain conservative. File sets do not bypass independent category ownership,
dependency, lease, shared-claim trust or approval gates and do not establish
verified isolated-worktree custody. Tests cover disjoint/intersecting/parent paths,
malformed declarations, unchanged shared-claim trust and journaled scopes;
source declarations are not a claim that tests have run.

## Historical gap observations do not confer authority

History inspection is read-only and remains reachable without claiming a task.
Recording recovery is a governed mutation in the current actor's verified
checkout, not an administrator capability. It cannot authorize an exception,
replace a prior checkpoint, alter another actor's task, or certify earlier
history. It requires a distinct workstream with source ancestry rooted in the
accepted target. Ordinary resolution retains a digest/reference to the archived
observation; it does not make the old missing checkpoint pass verification.
A human-intent reference or local core label is not independently verified
maintainer authority. Offline exception authorization and project-scoped database
maintenance need separate accepted contracts; Propodus #88 owns future connected
authority. No such capability is silently enabled here.

## Exceptional RBAC and self-verification custody

The main repository's `[governance.rbac]` configuration declares separate
default-deny permissions for reviewed task-journal repair and explicit local
validation exceptions. Invalid, oversized, absent, symlinked or ambiguously
resolved main policy fails closed. Audited operations record actor label,
reviewed rationale and configuration digest. `DECAPOD_AGENT_ID` and local
reserved actor labels remain cooperative assertions, not cryptographic human
or cloud authentication; protect policy edits through human review.

A validation exception leaves proof and publication readiness false, and
cannot supply `validate_passes`, bypass CI or authorize publication. The
Nix-wrapped verification path permits only the exact canonical currently
running executable, never arbitrary similarly named executables. Readiness
hash normalization strips only per-run IDs and evidence digests, preserving
material, policy, decision and blocker semantics. Laya requires explicit
post-proof configuration and cannot waive deterministic validation.

## Current model policy and private registration

`laya_readiness::verify_policy_acceptance` rejects candidate-side disablement or
repinning of accepted required Laya policy. Private registration version 2 uses
`decapod.laya-local-runtime.v2`; unsupported versions produce explicit setup
instructions and leave the record untouched. Native format, canonical path,
ownership, single-link, byte-size and digest checks remain enforced. Model setup
uses bounded credential-free public asset transport. Opaque user configuration
extensions survive refresh but cannot enable evaluation or grant authority.

## Explicit managed project routing

The private project record is a routing cache, never authorization. Every selected route checks current server account/project status before a native operation, and the native operation independently authorizes membership/grant/assignment and current session. Forged metadata, account switches and denied references cannot silently select a different project or SQLite. The auxiliary file uses private no-follow regular-file checks, exact custody binding, bounded decoding and atomic durable replacement under the credential lock. A rejected data bearer is compared under the original Git checkout custody scope, even when the data route uses a managed project reference.

The resolved native project also supplies the repository field of newly created tasks. Before the first possible write dispatch, the existing machine-private todo operation journal atomically binds the operation to a digest of its resolved service/project route. Preflight login and initial project selection retain the same operation ID; subsequent selection changes cannot replay an uncertain or successful write into another project. Older unknown records remain bound to their original canonical repository semantics. The real PostgreSQL consumer exposed the former Git-label write mismatch; that failed candidate supplies no successful native proof.

## Versioned cloud mutation receipts

Operation-route binding must survive clients of different releases. Versioned receipt envelopes reject older array-only writers before dispatch, while the stable lock serializes migration with older in-flight writes. Both record count and actual encoded bytes are bounded before atomic replacement; an upgrade that would exceed the byte cap preserves the original evidence instead of truncating it.

## Canonical terminal rule context and proof-backed completion custody

Terminal rule evaluation (`core/validation_rules.rs::evaluate_terminal_rules`) derives `task_type` and `git_ref` from verified trajectory/branch task identities and canonical `refs/heads/<branch>` names without creating or mutating `.decapod/data` SQLite state. Plural store categories (`features`, `bugs`, `tests`) normalize to the built-in rule vocabulary (`feature`, `bugfix`, `test`) so pre-execution and publication scope/custody/evidence rules cannot be bypassed by category naming differences, while unmapped administrative categories fall back to `None` so `task_types`-scoped constitutional rules remain enforced. Undeclared workunits never block ordinary first-validation bootstrap, whereas any durably declared workunit remains fail-closed (`TERMINAL_WORKUNIT_UNPROVEN` or `UNVERIFIED_COMPLETION`) until its checkpointed execution binding and required proof-plan gates pass.

## PR 1666: accepted v0.143.2 source integration

Header parsing is bounded and accepts only unambiguous numeric quota hints from verified HTTP200 transport responses. A quota hint authenticates no evidence and authorizes no fallback identity. Exact repository/candidate/target and provenance checks remain necessary.

## PR1666 current-master integration

Preserve exact source/base checks, public-reader denial, final remote binding verification and accepted path/custody gates. GitHub PR metadata must identify the actual master tip before a fresh event is emitted. A successful process or historical test does not establish current independent readiness.

## PR1663 integrated storage behavior

Missing components, corrupt references, incompatible readers/writers and unauthorized custody must remain rejected. Storage migration1200 and format-v2 behavior stay confined to source and hosted disposable fixtures during this update. No direct machine datastore edit or candidate migration is authorized.

## PR 1662: accepted v0.143.2 source integration

Do not delete lock sidecars, signal foreign processes, steal claims, replay ambiguous writes or infer cloud privilege from local actor labels. Backup destinations and audit output remain bounded and confined. Default denial and accepted-policy revision checks survive integration with current master.

## PR1662 current-master preparation

Worktree policy edits cannot self-grant capabilities. db.recover remains owner-only and cloud authority fails closed when it cannot be authenticated. Retain non-destructive lock diagnosis, current-process-only release and confined backups. Coordination receipts are not independent acceptance proof.

## Candidate repinning is not accepted policy (#1658)

The accepted target's local proof requirements remain authoritative even when a
candidate supplies a valid new digest for different commands, execution inputs
or coverage. `repository_profile::preserve_accepted` rejects those replacements
before selection; required failures cannot disappear through candidate review.
Accepted external limitations cannot become unsupported local-equivalence claims.

The regression boundary uses real Git targets and native subprocess checks,
including a replacement command that only prints claimed success. These tests
exercise kernel admission; a model refusal or a predetermined provider error
would not establish the guarantee. Accepted-target observation and the configured
executor remain trust roots. This repair does not claim OS isolation from an
unrestricted agent or add authenticated cloud authority.

### Hosted source-check substitution control

Buildkite9472 on candidate059dec1b actually returned local `ready=true` and
`proof_ready=true` after a candidate corrupted source, replaced its inventoried
check script with a success-printing script and repinned the source digest.
Command-only preservation was insufficient. The runtime now also preserves the
accepted local source-definition digest before selection. The failed observation
remains historical; fresh hosted execution must establish the repair. This is a
local-readiness finding, not evidence of bypassing an independent remote merge
service or of OS isolation from an unrestricted producer.

## Workunit producer and consumer boundary (#1660)

Missing, altered, contradictory, unbound or stale canonical observations cannot
be applied through supported workunit acceptance/completion/publication consumers.
The verifier rechecks candidate/base/material/policy/tool/input identities, live
task custody and durable workunit projection. Fresh complete terminal observation
is distinct from a previously declared success, avoiding a first-proof cycle.
History is retained after invalidation. Local hashes and a shared-UID datastore
are not an authenticated independent security domain; unrestricted direct SQL or
filesystem mutation requires separate isolation/enforcement. No signature, cloud
identity, arbitrary filesystem prevention or remote merge authority is asserted.
