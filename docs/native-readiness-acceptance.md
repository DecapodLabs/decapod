# Released native CLI acceptance (#1474)

The shipped runtime regression `tests/consumer_readiness.rs` executes real native
commands, but replaces project prerequisites and accepted-target transport. It
is not evidence that released `decapod validate` succeeds through actual
repository governance. Do not close #1474 on that regression alone.

## Agent-operated acceptance

`tests/acceptance/native_readiness.py` is an opt-in acceptance observer around the
released CLI, not a new provider or a replacement publication gate. It does not
bootstrap a repository, approve plans, rewrite proof policy, manufacture a
receipt, or create a PR. The agent prepares two genuine governed work branches
using the released controller's documented lifecycle:

- Both have an explicit `validation_buildkite_preflights=false`, real reviewed
  proof obligations, an approved plan, material specs, claims, trajectory,
  checkpoint, and committed clean candidate. Neither has a legacy
  `sections.validation` preparation receipt. Do not invoke project-check merely
  to manufacture that receipt before this acceptance.
- The positive candidate satisfies its real obligations. Its accepted target and
  installed execution tools must be available through supported transports.
- The negative candidate is independently governed and has an intentional source
  regression which fails an actual required native check. A missing prerequisite,
  unavailable provider, or incomplete plan does not qualify this negative control.
- Choose an ordinary tracked source file on the positive branch for the stale
  control. Own these workspaces exclusively for the duration. The observer appends
  temporary invalid source, invokes released validation again, and restores only
  its own bytes. It refuses to overwrite a concurrent edit. This tests rejection
  of a dirty candidate after an earlier successful run, not an in-flight target
  race; existing runtime race tests retain their separate scope.

The agent verifies the executable's official release provenance/checksum before
running the observer. Pass its canonical external path as `--released-binary`,
the two worktrees as `--passing-project` and `--failing-project`, the relative
source path as `--stale-file`, and a new private external `--output` directory.
Run outside CI. Do not unset CI flags or substitute a fake provider to obtain a
full-readiness result. Python 3.11+ and POSIX process groups are required.

The observer calls actual `decapod validate --format json`, saves stdout/stderr
for each run and the controller's byte hash, and checks:

1. Successful actual prerequisite gates and `publication_ready=true`.
2. One external completion pair with the returned verdict, native receipt hash,
   governance/trajectory hashes, evidence digest, proof-plan binding, invocation
   identity, unchanged before/after/per-check material and accepted target.
3. Unchanged committed candidate and governance after positive validation.
4. A genuine failed native observation with no success completion bundle.
5. Rejection of the dirty candidate, without publishing another success bundle,
   followed by exact restoration of the original source/candidate.

Only after every assertion passes does it write `acceptance.json`. A partial
transcript, an exception, timeout, a missing fixture, or `--help` is not a pass.
The original successful bundle may remain as historical evidence after the stale
probe; it must never be imported as current publication authority. The agent
still runs full current-candidate validation before PR creation.

## Current evidence boundary

This observer has not yet completed the full acceptance. The available Linux
workspace has no Docker/Podman, and no alternate connected/saved executor was
listed. The Decapod source repository's accepted configuration selects Buildkite
with `decapod.pr-readiness.v1` and CI-attested policy. Changing that checkout to a
weaker repository-local profile, removing accepted proof, or using the stubbed
consumer fixture would not establish the requested native acceptance. Native
worker networking limitations remain separate from source-policy selection.

A real supported native-mode acceptance project and its governed failure control
are still required. Preserve unsuccessful transcripts and report the exact gate;
do not describe this observer's implementation as a successful released CLI run.
