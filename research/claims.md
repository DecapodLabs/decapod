# Decapod Research Claims

This document is the falsifiable research spine requested by Issue #682. Each
claim names a failure mode, a baseline, a Decapod condition, and the evidence
that could disprove the claim. It is not product copy or a success narrative.

## 1. Intent is not durable without governed custody

**Claim:** Agent work drifts less when the original intent, derived intent,
boundaries, and unresolved assumptions are persisted as inspectable state and
reintroduced at decision boundaries.

**Baseline:** Run the same multi-step repository task with a vanilla agent and
record scope drift, contradicted assumptions, repeated clarification,
abandoned constraints, and unjustified task expansion.

**Decapod condition:** Run the task with a claimed todo, an isolated workspace,
an initialized trajectory, and context resolved through the Decapod control
plane.

**Failure mode:** The Decapod condition has no lower drift rate than baseline,
or its trajectory cannot show which intent and boundaries were active.

**Measurement:** Compare drift events per task, repeated clarification count,
constraint retention, and the fraction of actions with an inspectable intent
reference.

**Proof gate:** A replayable trajectory and custody record connect the human
intent to the executed scope and expose any boundary violation.

**Open questions:** How many tasks and agent/tool combinations are needed to
separate custody effects from model variance?

**Evidence status:** Instrumented in the repository; comparative benchmark
evidence is still required.

## 2. Governed context is more useful than undifferentiated context

**Claim:** Context selected from repository state and constitutional boundaries
produces fewer wrong-context failures than broad context dumps at comparable
task difficulty.

**Baseline:** Give an agent a large undifferentiated repository/context dump and
measure irrelevant file churn, token use, boundary violations, and task failure.

**Decapod condition:** Resolve a task-scoped context capsule with explicit
sources, scope, policy lineage, and task ownership before implementation.

**Failure mode:** Scoped context does not reduce irrelevant work or increases
wrong-context failures, or the capsule cannot explain why each source was
included.

**Measurement:** Compare token consumption, irrelevant edits, rediscovery
steps, boundary violations, and successful completion rate.

**Proof gate:** The context capsule validates its source lineage, policy binding,
task scope, and deterministic hash.

**Open questions:** Which task classes benefit most from narrowing, and when is
additional context worth its cost?

**Evidence status:** Capsule schemas and integrity checks exist; a baseline
study remains open.

## 3. Completion requires proof rather than an agent claim

**Claim:** Requiring bounded validation and durable proof artifacts lowers the
false-done rate compared with accepting an agent's natural completion claim.

**Baseline:** Accept completion claims without a required validation run and
measure missing tests, broken builds, uncommitted changes, and unsupported
claims discovered after completion.

**Decapod condition:** Require `decapod validate` to pass and provide the
tracked `.decapod/governance/trajectory.json` and
`.decapod/governance/validation.json` pair for every successful validation
completion and publication flow.

**Failure mode:** A completion is accepted without both artifacts, the receipt
does not bind to the trajectory hash, or post-completion review finds the same
false-done rate as the baseline.

**Measurement:** False-done rate, validation failures after claimed completion,
missing proof artifacts, unsupported claims, and time to detect a bad completion.

**Proof gate:** Validation exits successfully, the trajectory records the
validation check, the receipt hash is valid, and the receipt identifies the
current trajectory run and artifact hash.

**Open questions:** Which proof gates predict production-relevant correctness,
and how should warning-heavy but passing runs be classified?

**Evidence status:** The validation/publish path now enforces the artifact pair;
field evidence is still required.

## 4. Multi-agent work needs custody, not chat history

**Claim:** Claimed todos, isolated workspaces, explicit boundaries, and proof
artifacts reduce coordination collisions and handoff ambiguity in concurrent
agent work.

**Baseline:** Run concurrent agents on adjacent or overlapping work without
branch/worktree ownership or durable handoff records.

**Decapod condition:** Give each agent a claimed todo, a todo-scoped workspace,
an explicit trajectory, and a validation-gated publication path.

**Failure mode:** The Decapod condition does not reduce collisions or cannot
show which agent owned which scope and proof, or publication accepts stale
custody state.

**Measurement:** Collisions, duplicate edits, overwritten assumptions,
conflicting changes, handoff ambiguity, and unprovable ownership of the final
diff.

**Proof gate:** Todo ownership, worktree branch, trajectory task binding, and
validation receipt are all recoverable from repository-native state and Git
history.

**Open questions:** How does custody scale when tasks intentionally share an
interface or require coordinated changes across workspaces?

**Evidence status:** Isolation, todo claims, trajectory binding, and publish
gates are executable; concurrent benchmark evidence remains open.

## Future capability rule

Every new kernel capability must map to one of these claims or introduce a new
falsifiable claim before it is treated as a research-relevant capability. The
new claim must define a baseline, Decapod condition, measurable failure mode,
proof gate, open questions, and evidence status. A feature description alone is
not evidence.
