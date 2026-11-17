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
digest and repository identity. Validation envelopes reference the kernel's
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
