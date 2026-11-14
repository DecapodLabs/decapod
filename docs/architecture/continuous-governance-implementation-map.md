# Continuous governance implementation map

The accepted baseline is `f5da0e6183bf31cfbef3d5a53c52553ee8b6fbb5`
(Decapod 0.145.5). This map describes that code, identifies pending changes
separately, and cites executed observations by their actual source identities.
A merged PR, documented rule or agent declaration cannot supply missing proof.
The complete autonomous-engineering mission remains unresolved.

## Existing kernel and enforcement

“Already enforced” means implemented at the named supported boundary with
executable regression coverage. It does not assert an independent security
boundary against an unrestricted producer sharing the verifier's OS account.

| Classification | Mechanism and executable coverage | Assurance boundary |
| --- | --- | --- |
| Already enforced | `validation_runtime.rs`, `native_proof.rs` and `repository_profile.rs` bind measured Git material, target, policy and required executions. `consumer_readiness` covers real local commands and source/policy invalidation. | A reviewed local contract can establish `repository_local_readiness`. Declared tool/input closure does not infer arbitrary semantic or transitive completeness. It cannot discharge Decapod's stricter source-PR provider contract. |
| Already enforced | `repository_profile::preserve_accepted` and runtime loading preserve accepted local commands, execution declarations, coverage and the reviewed source-definition identity. The real consumer fixture attempts check replacement and candidate digest repinning. | A candidate cannot authorize its replacement policy. A separately accepted policy transition is required; first-adoption review remains a cooperative local trust input. |
| Already enforced | `workunit.rs` consumes canonical observed validation and remeasures current inputs before VERIFIED, replay, resume, completion evidence and publication consumption. `validation_history` fences task custody and append on the same connection. PR #1681 is merged. | A declared pass, artifact hash, changed projection or copied envelope is not producer authentication. Local observation remains in the host's trust domain. |
| Already enforced | `validation_runtime::require_full_readiness_environment` and CLI/RPC/publication/completion callers reject diagnostic overrides; broker scope is request-local. PR #1677 is merged. | Partial project/CI diagnostics remain available with publication unproven. Removing a flag does not refresh an old receipt or resolve an unknown write outcome. |
| Already enforced | `governance_document/checkpoint_history.rs::inspect_checkpoint_history` reconstructs immutable commit checkpoints; `require_proven` rejects findings and absent proof. Publication checks committed material and current governance. | A current checkpoint cannot manufacture an older missing checkpoint. Standalone history inspection leaves publication unproven; an authenticated offline exception issuer is missing (#1569). |
| Already enforced | `validation_history.rs`, `plugins/verify.rs` and `todo.rs` verify canonical historical proof and integration for `todo done --validated --commit <candidate>`. Wrong-task/candidate/integration and custody cases are covered. | Closed #1610 concerns post-merge chronology, not a new publication waiver. Historical permission cannot authorize a different candidate or substitute for fresh publication checks. |
| Already enforced | Task journal, exclusive leases, authoring bindings, resource reservations and broker request deduplication govern supported transitions. SQLite maintenance is default-deny under canonical opt-in and task/workspace custody. | PR #1662's local engine and #1679's required SQLite coverage are merged. Cloud owner authority and unrestricted filesystem prevention are separate; local opt-in never grants them. |
| Partially enforced | Workunit journals retain generations and uncertainty, but a separately initialized workspace DB cannot atomically accept parent task proof. | `WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED` remains fail-closed. Compatible explicit convergence/migration is still missing (#1660); copying history must not grant new authority. |
| Partially enforced | Full readiness freezes source and independently checks accepted target/integration through its supported provider. Workunit consumption remeasures local applicability. | An unfetched ref cannot reveal remote movement. Local workunit validity is a prerequisite, not a replacement for publication's fresh target observation. |
| Partially enforced | Accepted migration code preserves inert zero-byte TODO placeholders and blocks uncertain nonempty sources. Draft #1697 adds explicit bounded retirement of empty inputs or exact canonical copies. | #1697 is not in this accepted baseline. General legacy-row reconciliation and changed prepared-input supersession remain missing (#1657); physical archival never proves task completion. |
| Partially enforced | Merged #1684 preserves owning-test/fixture selection in the finite source-CI recipes. | Core/CLI partitioning and the measured five-minute requirement are unresolved (#1672). An accepted controller must support the new graph before activation; skipped selection is not timing proof. |
| Advisory | `AGENTS.md`, orientation, capsule retrieval and hooks discover/invoke existing kernel decisions. | An unrestricted shell can bypass hooks and directly mutate files or call Git/forge APIs. No arbitrary OS mutation prevention is asserted. |
| Missing | Prompt-independent native runtime reference integration, the complete ten-worker convergence harness and final A-H binary-level acceptance proof. | Existing CLI/RPC interfaces, actual child-process fixtures and concurrent deterministic workers are groundwork, not these complete demonstrations or ten real LLM agents. |
| Missing | Independently authenticated exception authority and complete Decapod repository read/write/owner-repair consumption (#1569, #1590). | Propodus server groundwork and Dactyl #123 do not prove deployed authority or released Decapod client behavior. #1667 now requests SDK 0.17.0; the accepted baseline pins published 0.16.1. Never substitute local labels after cloud denial. |

## Success-producing paths and remaining bypasses

1. **Readiness:** `validation_runtime::validate` reconstructs the expected plan
   and material, selects the supported executor and judges observed evidence.
   Missing, stale, corrupt or incomplete observations cannot be promoted by an
   agent-written success report. `ci_local`, prerequisite diagnostics and a
   complete native workbranch receipt have different scopes.
2. **Workunit VERIFIED:** declarations remain diagnostic until current canonical
   observed proof satisfies the full task contract and custody transaction.
   Additional task obligations cannot be replaced with weaker workunit gates.
3. **Todo completion:** validated current/historical completion consumes the
   canonical verification history for the exact task and applicable integration.
   A passing command, workunit status or another task's evidence is insufficient.
4. **Portable records:** exported bytes preserve custody. The receiver reconstructs
   applicability; missing/changed binding or artifacts cannot be repaired by a
   sender's claimed success. An envelope is not an authorization token.
5. **Governed publication:** `workspace::publish_workspace_with_readiness` checks
   custody, immutable checkpoints, current governance, merge conflicts and complete
   readiness, verifies the destination/PR, and rechecks movement after forge calls.
   Eligibility does not grant human merge authority.
6. **Exceptions and direct execution:** cooperative `validate --bypass-validation`
   keeps proof/publication false. A spec decision reference records provenance;
   it is not independently authenticated permission to waive unrelated checks.
   Direct Git pushes and forge PR creation remain possible outside Decapod for an
   unrestricted actor. An independently controlled service/forge restriction is
   required to enforce a remote merge gate.

A review draft opened through that external surface does not establish native
publication eligibility, accepted task completion or an exception to proof policy.
Current workbranch pushes are iteration; accepted completion remains separately
judged. No new daemon or parallel policy interpreter is needed for these limits.

## Real executed observations and their limits

For `d0b24f6f926583047be0fbe3f51f5686247d1de2` against the accepted baseline,
[GitHub push execution 43649604115](https://github.com/DecapodLabs/decapod/actions/runs/43649604115)
and [PR execution 43649753627](https://github.com/DecapodLabs/decapod/actions/runs/43649753627)
passed their applicable native checks. These are observations for draft #1697,
not receipts for this document's candidate, draft #1698 or an arbitrary later tree.

- [CLI job 132615175223](https://github.com/DecapodLabs/decapod/actions/runs/43649604115/job/132615175223)
  executed the three-target accepted recipe, including `cli_contracts` with the
  actual-binary SQLite/recovery module. Source-CI v3 reported complete execution
  for that exact head/base/tree in 949,797 ms including build. Target durations
  were 174.6 s, 221.7 s and 349.2 s. The recovery tests verified unchanged canonical
  bytes, an archive preserving bytes/inode, a receipt smaller than 4 KiB, exact
  replay, interrupted preparation and rejection of stale/unsupported inputs.
- [Core job 132615175154](https://github.com/DecapodLabs/decapod/actions/runs/43649604115/job/132615175154)
  passed the two exact broker regressions and complete eleven-target invocation
  in 2,063,432 ms including build. The full broker and interop targets took
  817.8 s and 639.2 s. These are actual observations, not five-minute proof or a
  controlled estimate of this feature's incremental cost.
- [Contract job 132615175300](https://github.com/DecapodLabs/decapod/actions/runs/43649604115/job/132615175300)
  actually ran `contract_conformance`. Its already-accepted index alignment
  regression supports closing the bounded documentation issue #1164; it does not
  establish an unrelated runtime, cloud or publication guarantee.

For draft #1698, source `6040f0853d028c538bb6f44e4e4aa0af7997f64e` against
the same accepted base, [release-policy job 132624601734](https://github.com/DecapodLabs/decapod/actions/runs/43652192736/job/132624601734)
actually passed `nix_gates_recognize_squash_linear_and_ordinary_release_commits`.
The actual workflow scripts exercised real Git histories and a local shallow
fetch, not Nix/Cachix publication. [Consumer job 132623199635](https://github.com/DecapodLabs/decapod/actions/runs/43652192736/job/132623199635)
passed 1,759 library tests with eight explicitly ignored after the reviewed
workflow pin was reconciled. Neither observation puts that pending workflow
contract into the accepted baseline or supplies full native publication proof.

The source-CI records explicitly report `authenticated=false` and
`complete_pr_profile=false`. They must not become an authenticated full native
receipt. Buildkite evidence is not collected for this delivery; its accepted
requirement remains enabled. Existing automatically triggered provider runs are
separate observations. Historical complete collection for
`b586dac3ec7d7048f296c214818c5f735138c89a` against
`422e0b56879abc1439032528a6d3e4514932e807` is not current-candidate proof.
Provider retention may expire diagnostic artifacts; missing evidence remains
unverifiable rather than surviving as an authoritative agent summary.

## Local proof, runtime integration and independent authority

`tests/consumer_readiness.rs` exercises real configured local checks through the
production adapter with a fixture transport to a genuine bare-Git target.
`tests/support/fresh_docs_readiness.rs` combines actual CLI prerequisite admission
with that runtime, executes first proof and rejects premature completion. Its
child CLI admission is explicitly `ci_local` and publication false. The combined
fixture is useful evidence for the local mechanism; it is not a normal-forge,
full plain-CLI A-H demonstration or a native LLM-host integration.

`workspace::resolve_readiness_repository` currently discovers a known network
upstream and the reviewed GitHub adapter. A standalone bare/local upstream is
not admitted by that full CLI discovery path. This is a concrete remaining
implementation boundary for an offline representative-repository demonstration;
a fixture transport must not be substituted for that supported acceptance path.

A supported runtime can call existing context, scope/admission, execution-state,
validation and publication interfaces. It must actually invoke them. The kernel
owns the resulting decisions, but it cannot intercept arbitrary conversation or
unrestricted tool execution merely because an instruction file exists. Source
hashes establish integrity/applicability, not protected key custody or independent
producer identity. Isolation and authenticated acceptance require a real boundary.

Current Decapod retains Dactyl's native SQLite runtime and canonical SQLite
semantics (`core/dactyl.rs`, `core/db.rs`). The old no-SQLite-engine condition in
#1220 is not met by a facade or registry upgrade. This convergence work does not
silently replace that accepted foundation with another engine. Cloud authority,
revocation, server deployment and owner repair require their own verified
contracts; Propodus login is not required for local solo work. Complete offline-local
upstream readiness admission remains the separate gap described above. Billing, dashboards and hosted runners remain outside this effort.

## Remaining end-to-end delivery

The full requested A-H demonstration must still integrate intent/scope, separate
workspaces, supported reference runtime calls, local proof, unauthorized mutation,
source staleness, target movement, forged completion, concurrent conflict,
interruption/resumption and the local/external assurance contrast. Disjoint broker
load is not evidence for all those transitions or final combined integration.

Workspace-local store convergence, authenticated exception/owner authority and
native integration remain explicit work. Reconstruction/invalidation microbenchmarks,
full storage growth and unchanged-input reuse measurements are incomplete; the
aggregate execution times and bounded receipt assertion above do not fill them.
Older finite controllers cannot learn changed workflow support from a candidate
review digest alone. Draft #1698's reviewed Nix pin and future shard activation
require independent accepted-controller support before full native readiness.

[Discussion #1556](https://github.com/DecapodLabs/decapod/discussions/1556)
requires runtime-native governance rather than more prompt text.
[Issue #1610](https://github.com/DecapodLabs/decapod/issues/1610) preserves
post-merge chronology. Neither is treated as executable evidence by this map.
