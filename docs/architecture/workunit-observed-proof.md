# Observed workunit proof

A workunit declaration is intent and retained history. It cannot establish that
its required checks ran. `record-proof --status pass`, a file containing green
text, an epoch field, and a `VERIFIED` projection do not grant current readiness.

## Existing authority and the narrowed repair

`validation_runtime` selects the accepted verification contract, executes the
sealed native/provider suite, verifies exact material bindings, and evaluates
terminal rules. `validation_history::finish_archive` persists only its opaque
successful observation in the existing canonical verification journal. Workunit
journals retain execution custody and recovery generations. None of these are a
new datastore or another policy interpreter.

Previously, declared file hashes and resumable execution custody could satisfy
the workunit transition without establishing actual validation provenance. The
current transition obtains the matching canonical producer record, independently
remeasures its inputs, and derives the required result and epoch. File artifacts
retain their existing path and byte-integrity semantics. The optional
`validation_record` contains the candidate, record ID and record digest; it is a
selector, never authentication supplied by the agent.

The currently supported task obligation is one complete `validate_passes` or
`validated_commit` gate. Additional or unknown required gates remain unsupported
and block acceptance. The selected repository contract still determines every
required check. Repository-local scope cannot discharge hosted requirements, and
Decapod source PRs retain their stricter accepted profile.

## Exact applicability and recovery

The producer records the original workunit generation, manifest digest and
contract digest before execution, and compares them again before canonical
append. Current consumption compares task revision and custody, accepted intent,
repository, candidate, branch, accepted local target ref, Git material/history,
policy/workflow bytes, validation epoch and measured tool/input closure. Relevant
untracked files and modes participate through the existing Git material reader.
It does not use modification time as content identity.

A changed dependency rejects application of the old successful observation.
History remains unchanged. A missing tool or unavailable provenance is unproven.
The durable current workunit projection must match its journal and execution
binding. A missing projection can be recovered from that journal; divergent
valid contents are preserved for explicit reconciliation.

Read-only applicability checks do not execute repository commands or append
success. Tool/input references are deduplicated and bounded; source snapshots and
command outputs are not copied into the canonical record. Non-workunit history
omits the new references. Hosted test logs report serialized growth, actual local
execution and reconstruction time; measurements are pending until those jobs run.

An unfetched forge change cannot be discovered through a local ref alone. The
full publication runtime separately rechecks the actual accepted remote target
and integration result. Current workunit proof is a prerequisite within that
boundary; it does not itself grant publication, a remote merge exception, or
human merge authority.

## Compatibility and assurance

Historical JSON without `validation_record` retains its canonical encoding and
is readable. Existing declared results remain history. They cannot be promoted
into observed proof. Explicitly downgrade an unsupported old `VERIFIED` workunit
to `CLAIMED`, select the supported full obligation, obtain a current execution
checkpoint, run full validation, then request the verified transition. Failed or
unknown operations must be inspected before retry, using the existing journal
recovery interfaces.

Rust callers constructing `WorkUnitManifest` add `validation_record: None` for a
legacy declaration. Proof consumers pass the repository root to
`validate_verified_manifest(root, manifest)` so context-free status checks cannot
masquerade as current verification. JSON CLI names and existing file artifacts
are unchanged.

New VERIFIED append and identical-operation replay also recheck canonical task
checkpoint, current exclusive lease and additional task obligations on the same
SQLite transaction connection as the workunit append. Normal linked workspaces
using the canonical parent runtime retain that transaction domain. An explicitly
separate initialized workspace database returns
`WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED`; this implementation does not pretend
that two independent database transactions are atomic. Compatible migration or
convergence for that split runtime remains unresolved, with history preserved.

Historical non-workunit records omit the witness. Verifiers predating observed
workunit support reject the new optional record fields as unverifiable; rolling
back does not convert the observation into a legacy passing declaration. A safe
reader/writer migration for independently initialized workspace runtimes is still
required before claiming complete workunit convergence.

The local trust root is the installed kernel, accepted policy, measured tools,
and canonical local journals. Producer and agent sharing an unrestricted OS user
are not independent security domains. Digests establish integrity and binding;
they do not authenticate a privileged operator or prevent direct filesystem or
SQL mutation outside supported operations. Native agent hooks remain cooperative;
execution isolation and authenticated cloud/forge publication require their own
independent enforcement. This change introduces no signing key, daemon, cloud
fallback or hosted runner.

## Reproduction and evidence boundaries

`validation_history_workunit_tests` uses the real public runtime and sealed native
adapter to execute two source assertions, then traverses canonical archival and
workunit consumption. Its accepted-target transport reads a genuine local bare
Git repository. It exercises fabricated output, source/config/branch/base changes,
untracked input, selector substitution, interruption/recovery, changed workunit
at archival, and declared outcomes after genuine verification. Portable completion
tests reuse that actual observed fixture and retain artifact/custody checks.

`workunit_cli` and publication tests separately exercise actual binary legacy
admission and the supported kernel publication helper. The local transport fixture
is not evidence of full CLI forge integration or an independently authenticated
remote verifier. Hosted execution results and exact final publication evidence
must be attached before claiming the repair passed.
