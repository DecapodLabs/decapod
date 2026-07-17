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
- the final workunit reached VERIFIED only after a passing proof result.

The Rust integration test `activation_conformance` runs this same repository-native contract from a clean checkout and emits a JSON observation report on failure. External host runs should capture the equivalent observations using the schema in `result.schema.json` and attach the raw transcript or command receipts.

Host procedure:

1. Start from a clean checkout containing this fixture and the generated entrypoints.
2. Give the host only the task intent above.
3. Preserve the host transcript and the repository's Decapod state after the run.
4. Record one result object for each required observation; a correct final file is not sufficient evidence.
5. Repeat with equivalent clean checkouts for Claude Code, Codex, and Antigravity when those hosts are available.

This fixture does not add a daemon, hidden context injection, vendor adapter, remote service, or new trust authority.
