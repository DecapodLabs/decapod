# Decapod activation conformance fixture

This fixture is the bounded, host-neutral task for Issue #880. A host must receive only the task intent below and operate in a clean checkout. The repository's generated entrypoint is the activation contract; the host must not be given a hand-translated Decapod command sequence.

Task intent:

Update the fixture greeting while preserving the repository governance contract.

Required evidence for a host run:

- the host discovered and followed its generated entrypoint;
- governed context was resolved before mutation;
- the task was claimed and work happened in a Decapod-managed workspace;
- proof prerequisites blocked premature completion;
- validation ran before completion;
- the final workunit reached VERIFIED only with native proof applicable to its exact task, candidate and accepted target; a declared prerequisite result is insufficient.

The Rust integration test `activation_conformance` exercises deterministic discovery, context and custody boundaries from a clean checkout. Its diagnostic gate-skip flags and absent full readiness profile deliberately test refusal: project prerequisites and a self-declared passing result cannot establish VERIFIED completion. Its failure report is a diagnostic, not a real-host result or publication receipt. External host runs must meet the complete contract using `result.schema.json`; keep operational captures private and publish compact redacted observations and receipt identities. The opt-in pilot protocol and its amendments retain failures and unavailable observations separately.

Host procedure:

1. Start from a clean checkout containing this fixture and the generated entrypoints.
2. Give the host only the task intent above.
3. Preserve the host transcript and the repository's Decapod state after the run.
4. Record one result object for each required observation; a correct final file is not sufficient evidence.
5. Repeat with equivalent clean checkouts for Claude Code, Codex, and Antigravity when those hosts are available.

This fixture does not add a daemon, hidden context injection, vendor adapter, remote service, or new trust authority.
