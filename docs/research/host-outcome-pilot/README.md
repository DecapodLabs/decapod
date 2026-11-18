# Real-host activation pilot: incomplete acceptance

This opt-in pilot addresses #880 and #1095. It is evidence of attempted real host
activation, not completion of either issue. The protocol and four amendments were
written before their corresponding trials in
`tests/fixtures/activation_conformance/`. Raw captures remain private; their
hashes identify operator-retained bytes, not publicly replayable or authenticated
provider evidence. Compact observations, comparison inputs and the retained-attempt inventory are committed here.

| Trial | Actual outcome | Independent fixed checker | Wall seconds |
|---|---|---|---:|
| Codex 0.169.0 baseline, gpt-6.1-sol | Greeting changed | Passed; dirty candidate explicitly recorded | 30.185467 |
| Codex governed, unconfigured actor | AUTHORING_ACTOR_REQUIRED; unknown claim preserved | Failed; source unchanged | 83.383615 |
| Codex governed, explicit stable actor | Native workspace reached; READINESS_REPOSITORY_MISSING | Failed; source unchanged | 304.195050 |
| Antigravity 1.5.10, gemini-4-carbon-medium | Ten-minute print deadline, partial result | Failed; source unchanged | 604.781253 |
| Codex, explicitly accepted local target | Greeting committed; ten-minute timeout during governance/proof | Passed on clean commit `1215dd06`; full native proof unobserved | 600.016065 |
| Codex local-target fresh-session recovery | Historical validation passed; completion proof absent | Passed on unchanged clean commit `1215dd06` | 90.204600 |
| Claude Code 2.1.311 | authentication_failed before inference | Unobserved; no model work | 0.952685 |

The two Codex cold arms have matching initial task source, declared model alias,
checker and protocol. Executable hashes at launch and complete environment
identity were not captured: they are null, and this pair cannot establish a
controlled causal effect. The explicit-actor recovery is a separate incomplete
pair with a disclosed setup amendment. Vendor aliases do not pin model weights.
The baseline accepted source hash differs from its unchanged seed commit because
it remained dirty. The collector retains both identities instead of labeling
that source as committed proof.

The identified Codex run created a native worktree and committed living-spec
updates before the kernel refused full readiness for a local-only upstream.
No fake GitHub origin or offline authorization was inserted. The earlier unknown
claim was neither released nor transferred after automatic approval review
rejected that ownership change. Antigravity's account did not offer auto mode;
the CLI reported fallback to its default approval behavior. Its sandbox remained
enabled. Claude's client initialized but reported no authenticated session;
account sign-in is pending the operator. Zero usage in that authentication error
is a client observation, not evidence of a zero-cost model task.

An exploratory baseline also completed the greeting, but searched a parent that
could expose sibling entrypoints; amendment 1 excludes it from the isolated pair.
Two Codex launch/parser failures and an initial Claude argument-parser failure
remain retained private attempts. They are not omitted successes or engineering
repair evidence. No warm or executed engineering-failure/recovery stratum is
complete. The separate fresh-session recovery below concerns native proof handoff, not a demonstrated failing engineering test followed by repair. A baseline compiler-wrapper failure is infrastructure evidence, not an
executed rejection of incorrect task logic.

`observations.json` contains redacted structural observations, raw-capture hashes,
vendor-reported usage and the separately executed fixed-checker result. The
checker was pinned before inference and executed outside each host. It establishes
functional greeting behavior only. It does not supply native governance completion,
publication admission or authenticated human approval. Provider-request correlation
is unavailable, so context delivery to a model remains unknown, including when a
host trace shows a tool result. These initial real-host trials used the released kernel, not the newly authored cooperative adapter. Adapter unit/CLI tests below remain a separate deterministic evidence class. No existing #1757/#1758 capture or fixture was
copied or attributed to this pilot: their unmerged acceptance format is scoped
to those owners and their current Pi/provider observer, not these host captures.

Run the compact collector with:

```sh
decapod trace flight-recorder compare --input docs/research/host-outcome-pilot/comparison-input.json
```

Every unavailable measurement has an explicit null and reason. Reviewed human
active minutes, semantic interruption classes, setup/validation allocation,
complete model costs, retained context bytes and delivery identity are unobserved.
No useful-work-per-human-minute ratio can be calculated. Codex and Antigravity
cache counters use different vendor semantics; do not sum or normalize them into
a cross-host treatment effect. The observations show extra governance effort
without verified task completion in these governed fixture attempts. They support
no efficacy, poisoning resistance, token saving or cost-saving claim.

The thin cooperative adapter is `decapod infer boundary --input <private.json>`.
Its typed prompt, initialization, context, task/claim/workspace, reflection,
tool-result declaration, completion and resume boundaries invoke existing native
owners. It selects only the running kernel, uses literal arguments and bounded
process/output limits, and rejects caller-selected executables. Every adapter
response denies publication/merge authority. Hosts must explicitly wire and deliver
each boundary; unrestricted shell, forge actions and provider/model delivery are
not intercepted or proven. Tool-result hashes remain host declarations. A timeout
may leave native work incomplete; inspect native custody before retrying mutation.

Completion blockers remain precise: authenticated Claude inference; a current native completion record for the governed engineering task; completed
warm and engineering-failure/recovery strata; independently correlated host/context
boundaries and reviewable human attention observations. Deterministic unit and
black-box adapter tests do not satisfy these real-host criteria.

Amendment 3 selects the already-supported canonical local-target policy on a
fresh accepted fixture seed. It repairs the earlier fixture configuration gap;
local proof was available in the accepted kernel. The real trial corrected the
greeting and recovered a governance-only intermediate commit, but timed out
during the required spec/proof loop. Its pinned external checker passes on clean
commit `1215dd0648b66686f50d5b93aa1dc9508e57e8a6`. This establishes functional
behavior, not completed governance. Final turn usage was not emitted before
termination and remains unknown. Amendment 4 declares a separate fresh-session
recovery with a twenty-minute deadline; the first timeout is not extended or
erased. Both use the released kernel, and still do not establish delivery of the
proposed selector/knowledge changes. The proposed source has separate native CI.

The amendment-4 recovery finished in 90.204600 seconds and reported 492,477
input tokens (449,152 cached), 1,144 output tokens and 15 issued tool commands.
Its independent fixed checker passes on the same clean candidate. Native
`qa verify commit` returns a passing **historical_validation** observation,
record `decapod-readiness-BbBEpc`, digest
`73c8eff7b56c2245eb45200f324f3515b519bb603448b9205ba2b99a60fffaff`.
That lookup explicitly has `publication_ready=false`. Completion lookup has no
record; the host's completion-proof update was refused because the candidate
was not accepted history. An independent actor's current validation was correctly
refused for stale authoring custody. No acceptance-history change or custody
transfer was made to manufacture a positive result. The timeout and its recovery
remain separate rows and both contribute to total observed work; tokens from the
timeout remain unknown. Host exit zero does not turn this into governed success.

The report preserves independent functional correctness separately from reported
verified useful work. A governed run qualifies for the latter only when completed
and accompanied by a declared native-verifier completion result; a baseline needs
the independent functional result. These remain unauthenticated input assertions,
not a new proof verifier. Harmful-context rate is a fraction requiring observed
delivery and independent or human review; absent delivery remains null rather
than a fabricated zero. Human attention remains unreviewed, so no attention ratio
or treatment advantage is reported.

The checker observation's `candidate_source_sha256` hashes the exact
`src/lib.rs` bytes copied into its independent compile snapshot; it is not a
whole-repository tree digest. `candidate_commit` binds committed repository
content, while `candidate_dirty` exposes the baseline's uncommitted change.
The initial `task_source_sha256` identifies the predeclared task-source fixture.
Missing whole-environment, policy and delivered-context identities remain null.
