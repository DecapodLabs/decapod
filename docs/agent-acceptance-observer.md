# Independent agent acceptance observer (#1757)

The existing Rust flight recorder can export a bounded, read-only presentation
of an externally captured agent run:

```sh
decapod trace flight-recorder acceptance --capture /private/run/capture.json
```

This command correlates observations. Its output always has
`publication_ready=false` and `merge_authorized=false`. Only the existing live
validator and publisher can authorize admission. Importing a capture, native
receipt or successful model statement cannot recreate that authority.

## Capture and trust boundary

`core::flight_recorder::acceptance::Capture` is the versioned
`decapod.agent-acceptance.v1` input. A private capture index identifies the owning
Decapod issue (1757 or 1758), original objective and revision, operator-approved
objective digest, pinned kernel/source/harness identities, evidence files and
ordered source pointers. Artifact paths must be sibling regular files; symlinks,
path traversal, altered digests, oversized input and nonmonotone order fail.

The capturing host supplies distinct sources: actual Pi host events, observed
kernel invocation results, independent Git snapshots and GitHub readback.
Agent declarations and scripted controls have separate labels. Source labels
are host assertions, not cryptographic producer authentication. The observer
cannot establish that an arbitrary caller has supplied genuine host events.
The experiment must capture the actual pinned Pi process outside the agent and
retain raw operational records privately. Uncaptured direct shell activity is
unknown; no universal host interception or OS sandbox is claimed.

Kernel envelopes retain the actual operation, exit status, measured executable
digest and repository identity. Essential kernel actions must join an actual
in-flight Pi bash tool call through host_tool_call_id; otherwise native admission
can be observed but cannot be attributed to autonomous agent operation. The
capturing host must observe the actual child invocation and causal tool interval,
not infer this join from a model's proposed command. Validation envelopes reference the kernel's
independently compiled expected plan, actual observations and native completion
receipt. The existing evidence verifier checks that historical suite rather
than a new proof interpreter. This correlation still does not authenticate a
caller-supplied plan or import a live completion. The trusted capturing host must
obtain it from the kernel, never from model narration or an agent-selected plan.

## Observable outcomes

An observed autonomous-admission arc requires the original objective in Pi's
input, real provider/model metadata, matched tool invocations/results, Decapod
context, task/workspace custody, an actually executed failing required check,
an agent-tool-bound changed candidate, fresh complete matching native evidence,
successful governed publication and separate matching GitHub readback. A source
change performed by the operator is not agent repair. Additional input and
observed bypasses prevent a zero-intervention autonomous result. Timeouts and
transport failures do not substitute for an executed engineering rejection.

Actual tool IDs must appear in observed provider tool requests. A repair must be
requested by a new model response and started after the failing tool's feedback
has returned. A prewritten repair in the same failing shell call cannot prove
response to governed feedback. Composite Codex tool IDs retain their exact join.

The exact published head, accepted target, work branch and repository must match
the receipt. A PR-created-but-not-admitted result cannot become success from a
PR URL. Later source/target movement clears current applicability while retaining
the historical admission and its rejection evidence. Fresh validation alone
cannot reinterpret an old publisher response as a new admission.

Exports contain stable digest-derived event IDs, execution order, elapsed time,
source class, evidence digests/pointers, approved public URIs and allowlisted
facts. Raw commands, model reasoning, outputs, environment, credentials and
local paths are not copied. Public objective text requires an exact operator
review digest and conservative secret checks. This is a redaction boundary,
not a guarantee that an arbitrary free-text secret can be detected automatically.

## Verification and actual-run status

### Retained capture coverage

A correlated admission is historical evidence. A complete autonomous-run result
also requires a retained capture boundary. The optional `coverage` input records
the run identity, observing host's witness custody, measured start/end in elapsed
milliseconds, observed channels, dropped-event count, retained-event count,
SHA-256 digests of the serialized event and artifact indexes, and the sequence
of the actual terminal process observation.
Index digests use compact UTF-8 JSON with sorted object keys and no whitespace.

The event index must retain contiguous sequence numbers. The observed channels
must include host, kernel, Git, and GitHub. The host must retain an actual
`agent_end` event followed by a `process_exit` observation with
`agent_ended=true` and `exit_code=0`. Independent Git/forge readback can follow
that terminal observation. Further model/tool activity cannot. The coverage
window must contain every retained event, and the reported dropped-event count
must be zero.

`capture_coverage.state` is `complete`, `incomplete`, or `unknown`.
Missing coverage in an older capture yields `unknown`. A missing tail, changed
index, incomplete channels, shared/unknown witness custody, or inconsistent
terminal observation yields `incomplete`. Both retain `admission_observed`
and the recorded history while clearing `observed_autonomous_admission` and
`current_admission_applicable`. The export always grants no publication or
merge authority.

The capturing operator retains the closing counts and index digests before
replaying or distributing a capture. Recomputing a checkpoint from an already
truncated bundle cannot establish its original completeness. Operator-controlled
custody remains a trusted-host assertion. An unrestricted OS user with access
to that host can forge it. Complete declared-channel coverage does not establish
visibility into unrelated host activity.

Offline adversarial controls live in
`tests/unit/core/acceptance_observer_tests.rs`, included by the core library and
therefore the existing mandatory full-library CI owner. They use deterministic
source fixtures; passing them is not real inference or real GitHub acceptance.
Run the repository's configured `decapod validate` for the exact checkpointed
source candidate. Required core tests stay independent of network access,
inference, GitHub mutations, website availability and media rendering.

The separate real Pi run and every disposable-fixture action are owned by
Decapod #1757/#1758. Original archive
`ae15eeffd6812002fbda2a2adaca23b006de0ba6` and historical PR #1 remain immutable.
Real-agent acceptance, public scene selection and film publication require the
actual observed run; this implementation document makes no claim that it has
already passed. Website issue #9 owns all rendering, assets and presentation.

## Actual headless Pi acceptance (2026-11-17)

Pi0.73.1 with the existing authorized openai-codex gpt-6.1-sol provider received
one ordinary objective: implement the README repository-scoped read authorization
and publish a verified PR. The disclosed seeded baseline required a real failed
check before repair. The host observed42 tool calls in458.276 seconds, with no
post-objective input or controller-written repair. Kernel0.148.8 remained
byte-pinned while the separate implementation workspace updated to0.148.9.

At bb1563c98fa8fb8845a24d76d9c722e9e4078659 the accepted authorization
command actually executed and failed with exit1: a member without a repository
grant was incorrectly allowed access. New model-requested tools inspected the
returned failure and repaired src/access.cjs. Fresh execution passed all ten
fixed assertions plus the independent source guard. Governed publication and
independent GitHub head/live-target readback correlated
[real PR2](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-06/pull/2),
head f3a7a160946b42fed7eefa3898362f0dbb122fd0, accepted base
4b7f3b05637120a2ac6d638c46a13288fa6d74ce.

The actual CLI export passed its opt-in acceptance test. The final167-event
export retains observed_autonomous_admission=true while current applicability
is false after an independently observed target advance. Separately labelled
operator controls created real PR3, provoked PR_CREATED_NOT_ADMITTED with
WORKUNIT_PROOF_STALE, independently confirmed draft quarantine, and verified
source and repeated STALE_ACCEPTED_BASE rejection. Neither probe repairs nor
controller activity is attributed to Pi. PR2 is now draft with unchanged head.

Source labels remain trusted-host assertions and the model ID remains a provider
alias; no universal OS interception, immutable model weights, hosted CI proof,
Cloud activation or merge authorization is inferred. The fixture evidence and
portable reproduction carriers are owned by#1757/#1758; the website#9 owns the
actual evidence-backed film and its safe public presentation. Required Decapod
source validation remains separate from fixture admission and observer export.
