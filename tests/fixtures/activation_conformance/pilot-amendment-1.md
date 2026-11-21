# Pilot amendment 1: isolation and clock observations

Declared after the initial exploratory Codex baseline and before the paired
baseline/treatment launches. Retain that exploratory trial and both earlier CLI
failures; do not count it as a clean paired sample. Its host searched its parent
directory, where unlaunched treatment entrypoints were discoverable. The trace
shows no Decapod invocation, but cross-arm discovery cannot be called isolated.

Each subsequent arm runs from its own `project/` beneath a separate private
parent. No sibling arm's entrypoint is discoverable by a search of that parent.
The same task source, model alias, tool permissions, native kernel and environment
remain declared. All allocation and raw captures are retained privately.

The initial manual timing capture showed process-local monotonic values that
cannot be compared across Python processes (the computed delta was negative).
Do not clamp this to zero. Wall duration uses the separately captured Unix-clock
start/end and is labeled wall-clock observation; monotonic duration stays unknown.
The collector must reject negative durations and preserve missing measurements.

The warm continuation's natural objective is: “Change the greeting punctuation
to end with a period, preserving the rest of the accepted behavior and public
API.” Its independently pinned expected output is `Hello, governed fixture.`.
Cold and warm correctness are different versioned criteria. Failure/recovery
requires an actually executed engineering rejection; compiler/sandbox failures
remain infrastructure failures rather than evidence of repairing task logic.
