# Architecture Overview (Canonical)

## 1. Storage Boundary

Decapod has one governed repo-native state root for project operations: `<repo>/.decapod`.

Rules:

- Promotion-relevant state MUST be repo-native.
- Agents MUST use Decapod CLI/RPC for state mutation.
- `.decapod` direct edits are forbidden.

## 2. Artifact Model

Core artifacts:

- Intent artifacts: `INTENT.md`, `SPEC.md`, ADRs.
- Claims artifacts: interface claims and proof obligations.
- Proof artifacts: validation reports, state-commit records, verification outputs.
- Provenance artifacts: artifact/proof manifests with hashes.

## 3. Validation and Promotion

Validation semantics:

- `decapod validate` is the repository health/proof gate.
- Failure means completion claims are invalid.

Promotion semantics:

- `decapod workspace publish` is the promote path.
- Publish MUST fail when required provenance manifests are missing.

## 4. Deterministic Execution Model

Determinism rules:

- Reducers and store updates are append-only/event-oriented.
- Envelopes are explicit, schemaed JSON.
- Golden vectors are used to detect protocol drift.
- Validation gates are executable and reproducible.
