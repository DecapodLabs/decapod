# Security

Vega's tracked team policy pins model identity separately from each maintainer's
private runtime binding. Explicit setup registers native executable bytes without
running them or proving their source. Unix runtime checks reject foreign ownership,
writable-by-others files, scripts, incompatible architecture and multiple links;
private registration records reject malformed or mismatched identities. Operator
selection of a trusted reviewed runtime remains a trust boundary. Validation
freezes the binding before proof and rechecks binding/model/runtime identity before
consuming an acceptance. Local registration cannot bless source proof, fabricate
Metal/CPU execution or establish numerical parity. CPU fallback is restricted to a
typed hardware-unavailable response without a model answer, then a fresh process
under the original deadline with reverified identical pins. Malformed output,
arithmetic failure, timeout, crash, negative verdict and abstention do not trigger
fallback. Runtime labels must match the explicit physical-backend request. New
CPU and CUDA paths still require actual execution and numerical verification;
neither a CPU answer nor a capability declaration proves Metal execution.

The verification-only scalar F64 reference verifies immutable model/config hashes and calculates expected values without candidate tensor/model math, while sharing generic infrastructure, weights and authorship. Candidate aa66c8e4 executed the138/314-token Metal/CPU subset; neither that success nor the new4096resource cap establishes all-input equivalence or independent human review. New exact-limit and phased-collection code is unexecuted. Raw tensors stay in bounded private exports; the final required scientific command rechecks referenced bytes and comparisons without importing earlier-run artifacts. Proxy labels, observed false-ready outcomes, incomplete coverage and23lineages retain the unsatisfied readiness-calibration gate.

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

Vega's required publication configuration is inherited from the accepted target.
`vega_readiness::verify_policy_acceptance` rejects candidate-side disablement,
runtime/model repinning, installation-path changes or calibration-setting changes
when accepted policy is required. Such a policy transition needs independent
acceptance; setup cache refresh cannot authorize it. A disabled accepted policy
keeps normal behavior and may add a required assessment. The same check runs in
initial and post-inference project discovery.

Most mutations require an active local session. Session credentials establish
repository-local custody and correlation. They do not authenticate an agent's
model provider, harness, organization or human identity. Local handshake hashes
make declarations tamper-evident; environment-supplied identity assertions remain
self-declared and unverified without a configured trust root. The current binary
exposes CLI and structured process RPC, not a native MCP server.

Local session credentials prefer private machine-local storage, with a secure
workspace-local fallback. Cloud authentication uses separate machine-local
records. Active Jev credential persistence and lookup are removed; historical
secret files are left untouched and are never read by vega. Session-health proof
records active or inactive status only. Credentials must not enter source,
trajectory records, validation artifacts or diagnostics.

Vega setup accepts only bounded public official-source artifacts with explicit
revision, size and SHA-256 pins. It does not accept credentials, agreements or
gated-model access. Runtime performs no network operation and invokes only the
canonical pinned native executable. Private create-new bounded request/proof
files may contain repository context and numeric features; they are local
diagnostics, never uploaded or treated as trusted policy. Repository text is
untrusted evidence, and a model verdict cannot overrule deterministic proof.

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

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `372fd097843e5e54a843b67b8012c0feab3f8d643a30eab9e8504e14d479574a`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (38 files), `docs/` (1 files), `src/` (210 files), `tests/` (193 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

## Quinn transitive dependency remediation

The locked dependency graph uses quinn-proto 0.11.19 through Dactyl's reqwest
resolution. The Cargo lock change updates only that package version and registry
checksum. The generated Bazel lock must be regenerated with the repository-pinned
Bazel version so both build systems resolve the same patched dependency;
release-bound projections must match the evaluating Decapod release.
No HTTP/3 feature is enabled: Cargo feature-tree inspection across default,
no-default and all Decapod features finds no active Quinn path. That limits the
identified runtime exposure but does not excuse vulnerable lockfile entries.
The dependency audit must pass without ignored advisories; the existing yanked
chacha20 warning remains separately visible. No transport policy, CI trigger,
provider permissions or accepted-executor helper is changed by this remediation.

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

The completed76 validation exposed false-ready outcomes against proxy labels and unexecuted/token-overflow cases on both backends. These remain required acceptance failures; no labels, sample denominator, calibration or thresholds are adjusted to make them pass. The reference's zero-memory omission is repaired from independently pinned specification/config and own scalar block outputs, with analytic anchors rather than candidate-derived expected values. Shared authorship/parser/weights and512-token scope still limit independence. Actual CUDA-unavailable-to-CPU component execution does not establish preferred-Metal failure or full production custody/fallback routing. All raw numeric inputs and receipts remain private outside Git.

Public provider metadata and logs may diagnose a failed bootstrap without credentials. They remain untrusted diagnostic content: reading a sibling success or downloading a failure log cannot import a passed observation, alter the required graph or grant publication. A pre-parser helper launch failure establishes no source-test or native execution. Provider recovery must preserve failed history and bind the next real execution to the exact ordinary branch candidate. No authentication state, raw machine logs or model inputs belong in Git.

Numerical and semantic references have separate trust limits. The scalar Rust reference shares parsing/hash infrastructure and pinned weights with the candidate, and its author is the same agent; measured agreement does not imply separate human review or unsupported-mode correctness. Public bug findings, test-oracle resolution and same-agent counterfactual labels do not establish comprehensive readiness. Related lineage IDs must not inflate independent risk counts, nor may an absent review finding be silently converted into a ready label. These limits retain the unsatisfied readiness-calibration gate rather than changing policy or thresholds to fit the available evidence.
