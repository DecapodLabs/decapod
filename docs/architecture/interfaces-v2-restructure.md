# Aggressive interfaces/* Contract Restructure

Status: approved design target for Issue #938  
Scope: interface contracts only  
Related: #938, #805, #632  
Separate concern: knowledge implementation and promotion policy remain in #657.

## Decision

The conservative additive uplift in #805 and PR #937 remains the compatibility-preserving baseline. This document records the explicitly approved aggressive alternative: a versioned, typed, envelope-first interface model with a deliberate breaking boundary.

The aggressive target is not a permanent second implementation. It is a migration to one canonical contract model:

1. Define one typed taxonomy for command, entity, event, artifact, and schema contracts.
2. Require every cross-boundary operation to identify its producer, consumer, contract version, lifecycle state, scope, outcome, and proof.
3. Replace operation-specific success/error shapes with one strict response envelope.
4. Make compatibility adapters temporary, observable, and removable.
5. Remove v1 adapters after the migration window; do not preserve branching behavior indefinitely.

This design is intentionally limited to interfaces/*. It does not implement the knowledge subsystem, alter knowledge.db, or turn #657 preference/context records into interface doctrine.

## Why a breaking model is justified

The current interface section is rich enough to describe contracts but does not give every contract one common machine boundary. The same concepts are represented in several incompatible forms:

- RPC responses have id, success, receipt, optional result/error, blockers, interlocks, advisories, and attestations.
- Internalization responses repeat schema_version, success, artifact identity, and operation-specific result fields.
- Constitution and context payloads use their own envelopes and references.
- Storage entities use database-shaped records rather than explicit producer/consumer contracts.
- Generated specs and validation consume interface identifiers and paths as if they were stable contracts, but their ownership and lifecycle are mostly convention.
- Errors are structurally typed in some paths and free-form validation strings in others.

Adding more optional fields would make the ambiguity durable. The aggressive alternative makes the contract boundary explicit and gives migration a finite end state.

## Current consumer inventory

The inventory below is grounded in the current repository and is the required migration ledger. The implementation follow-up must turn each row into a named owner and proof case.

| Contract family | Current producers | Current consumers | Current breaking surface |
|---|---|---|---|
| Constitution discovery | src/core/constitution_cli.rs, embedded assets/constitution.json, lookup and schema assets | constitution get/search, context resolution, constitution tests, golden vectors, generated context | Node IDs, section names, lookup terms, required fields, embedded JSON shape |
| RPC/control plane | src/core/rpc.rs, dispatch in src/lib.rs | CLI/RPC callers, tests/agent_rpc_suite.rs, golden request/response vectors, external agents | Request IDs, receipts, allowed-next operations, blockers, interlocks, error fields |
| Context capsules and bundles | src/core/context_capsule.rs, src/core/context_bundle.rs, RPC context operations | pre-inference routing, project context, capsule schema tests, release/projection gates | Fragment identity, authority/source paths, scope, hashes, deterministic serialization |
| Project specs | src/core/project_specs.rs, decapod rpc --op specs.refresh | .decapod/generated/specs/*, validation, agents, CI | Manifest version, spec filenames, content hashes, repo-signal fingerprint |
| Claims and evidence | proof/completion/validation modules and interfaces/CLAIMS | validation gates, canonical evidence tests, workunit publication, release checks | Claim kind, evidence refs, verification state, stale/blocked semantics |
| Internalization | src/plugins/internalize.rs | internalize CLI, internalization tests, generated artifact validation | Five result types, manifest version, lease lifecycle, risk/capability fields, replay metadata |
| Demands and plans | demand/scaffold/plan-governance paths | init/scaffold, plan execution, workunit gates | Required/must-not semantics, step state, checkpoint and rollback shape |
| Store model | src/core/store.rs, SQLite subsystems, RPC store operations | todo, knowledge, aptitude, federation, policy, health, context, tests | Store kind, state owner, scope, mutation authorization, entity envelopes |
| Testing and validation | test harnesses, src/core/validate.rs, interfaces/TESTING | CI, local agents, release checks | Command identity, evidence threshold, skipped/failed/blocker distinction |
| Risk and policy | interfaces/RISK_POLICY_GATE, policy and assurance code | mutation interlocks, approval gates, high-risk operations | Risk tier, approver, blocked operation, unblock evidence |
| Memory/context indexes | context and memory paths, interfaces/MEMORY_INDEX and interfaces/MEMORY_SCHEMA | agent retrieval and context binding | subject/claim/provenance/scope/status and relevance semantics |
| Knowledge interfaces | knowledge CLI/RPC and interfaces/KNOWLEDGE_SCHEMA / interfaces/KNOWLEDGE_STORE | #657 follow-up, knowledge search and promotion | Knowledge remains a separate subsystem; only its future contract boundary is inventoried here |

The complete current `interfaces/*` identifier set is the following migration
ledger. The five nested internalization schemas are included deliberately: they
are interface contracts, not implementation details that can be omitted from
the breaking-change review.

```text
interfaces/AGENT_CONTEXT_PACK
interfaces/ARCHITECTURE_FOUNDATIONS
interfaces/CLAIMS
interfaces/CONTROL_PLANE
interfaces/DEMANDS_SCHEMA
interfaces/DOC_RULES
interfaces/GLOSSARY
interfaces/INTERNALIZATION_SCHEMA
interfaces/KNOWLEDGE_SCHEMA
interfaces/KNOWLEDGE_STORE
interfaces/LCM
interfaces/MEMORY_INDEX
interfaces/MEMORY_SCHEMA
interfaces/PLAN_GOVERNED_EXECUTION
interfaces/PROCEDURAL_NORMS
interfaces/PROJECT_SPECS
interfaces/RISK_POLICY_GATE
interfaces/STORE_MODEL
interfaces/TESTING
interfaces/TODO_SCHEMA
interfaces/jsonschema/internalization/InternalizationAttachResult.schema
interfaces/jsonschema/internalization/InternalizationCreateResult.schema
interfaces/jsonschema/internalization/InternalizationDetachResult.schema
interfaces/jsonschema/internalization/InternalizationInspectResult.schema
interfaces/jsonschema/internalization/InternalizationManifest.schema
```

### Consumer classes

The migration must test four consumer classes separately:

- In-process Rust consumers: typed structs, module calls, database migrations, and validation functions.
- CLI consumers: command-line arguments, stdout/stderr, exit codes, and JSON output.
- RPC consumers: stdin/stdout structured calls, envelopes, operation sequencing, and golden vectors.
- Repository consumers: constitution IDs, generated specs, fixtures, issue/PR automation, and agent entrypoints.

A contract is not migrated when Rust compiles. It is migrated when all four consumer classes either consume v2 or are explicitly isolated behind the temporary adapter.

## Target contract taxonomy

Every v2 contract declares exactly one kind:

| Kind | Purpose | Required owner | Typical lifecycle |
|---|---|---|---|
| command | A user/agent-invoked operation | command producer and control-plane dispatcher | requested -> accepted -> succeeded/failed/blocked |
| entity | Durable state with identity and revision | state-owning subsystem | proposed -> active -> superseded/deprecated |
| event | Immutable observation of a state transition | event recorder | recorded -> verified/rejected |
| artifact | File-backed generated or derived output | artifact producer and validator | created -> inspected -> attached/revoked |
| schema | The contract that validates one of the above | schema owner | draft -> active -> deprecated |

Each contract has:

- a globally unique contract_id;
- a semantic contract_version;
- an explicit producer and consumer;
- a state owner;
- lifecycle state and transition evidence;
- a strict input/output shape;
- a failure model;
- a proof declaration;
- a migration predecessor or successor when replacing v1.

### Ownership rule

One subsystem owns the state; other modules may produce requests or read projections but may not silently become alternate owners.

For example:

- internalize owns artifact manifests and leases;
- validate owns validation evaluation, not the underlying artifact;
- rpc owns dispatch and receipt correlation, not every entity stored through RPC;
- knowledge owns knowledge entries, while aptitude/preferences remain a separate concern;
- constitution owns embedded doctrine, while generated context capsules own only their derived files.

## v2 envelope

The prototype is in interfaces-v2-contract.schema.json. The envelope is strict and common to requests, responses, and recorded events.

~~~json
{
  "contract_id": "decapod.internalization.inspect",
  "contract_version": "2.0.0",
  "kind": "command",
  "operation": {
    "name": "inspect",
    "phase": "response"
  },
  "request_id": "01K...",
  "correlation_id": "01K...",
  "producer": {
    "component": "decapod.plugins.internalize",
    "version": "0.73.0",
    "role": "state_owner"
  },
  "consumer": {
    "component": "agent",
    "version": "unknown",
    "role": "caller"
  },
  "scope": {
    "repo_id": "repo:sha256:...",
    "session_id": "01K...",
    "task_id": "bugs_...",
    "authority": "repo-local"
  },
  "lifecycle": {
    "state": "active",
    "transition": "inspect",
    "occurred_at": "2026-07-20T00:00:00Z"
  },
  "outcome": {
    "status": "succeeded",
    "result": {
      "artifact_id": "int_0123456789abcdef01234567",
      "integrity": {
        "source_hash_valid": true,
        "adapter_hash_valid": true,
        "manifest_consistent": true
      }
    },
    "error": null
  },
  "proof": {
    "schema_ref": "decapod.interfaces.internalization.inspect",
    "input_hash": "sha256:...",
    "output_hash": "sha256:...",
    "evidence": [
      {
        "kind": "file",
        "ref": ".decapod/generated/artifacts/internalizations/..."
      }
    ]
  }
}
~~~

The envelope deliberately separates identity (request/correlation IDs), authority (producer, consumer, scope, and lifecycle), behavior (operation and outcome), failure (structured error rather than a string convention), and proof (hashes and evidence references).

## Common failure model

All v2 failures use the same shape:

~~~json
{
  "code": "INTERFACE_CONTRACT_INVALID",
  "message": "The request does not satisfy the v2 contract.",
  "retryable": false,
  "category": "validation",
  "field_path": "payload.manifest.schema_version",
  "details": {
    "expected": "2.0.0",
    "actual": "1.2.0"
  },
  "next_operations": [
    "interfaces.migrate"
  ]
}
~~~

Error codes are stable machine values. Messages are explanatory and may evolve. A consumer must branch on code, retryable, and category, never on message text.

Required categories:

- validation: malformed or semantically invalid input;
- authorization: caller lacks the required authority;
- conflict: revision, lease, or ownership conflict;
- not_found: referenced state is absent;
- expired: TTL or lease is no longer valid;
- blocked: a proof, approval, or workspace gate is missing;
- internal: unexpected implementation failure;
- migration: v1/v2 translation cannot be made losslessly.

## Aggressive migration sequence

### Phase 0: Freeze and inventory

- Freeze current v1 schemas and golden vectors.
- Record every producer, consumer, fixture, lookup, generated artifact, and external example.
- Add a machine-readable inventory with owner, current version, target contract ID, migration status, and proof command.
- Fail CI when a new v1 consumer is added without an explicit exception.

### Phase 1: Implement the v2 substrate

- Add shared Rust types for envelope, participant, scope, lifecycle, outcome, error, proof, and contract metadata.
- Add strict JSON Schema for the common envelope and each migrated payload.
- Add deterministic canonical serialization and hash rules.
- Add an explicit version-negotiation operation; do not infer versions from missing fields.
- Add contract conformance tests that run without network or daemon state.

### Phase 2: Migrate the highest-risk families

Migrate in this order:

1. RPC/control plane, because every other contract may travel through it.
2. Internalization result family, because it currently has five parallel envelopes and a lease lifecycle.
3. Context capsule/bundle and project specs, because hashes and provenance must remain deterministic.
4. Claims/evidence and validation, because false proof is the highest-risk failure.
5. Store entities, plans, demands, testing, risk, and memory indexes.
6. Constitution IDs and section references, with an explicit v1-to-v2 map.

### Phase 3: Temporary adapter window

- Accept v1 only at a named adapter boundary.
- Translate v1 input to v2 immediately.
- Emit v2 only from the canonical implementation.
- Include adapted_from, original hash, and lossy-field warnings in the receipt.
- Count adapter use by contract ID and consumer.
- Reject unknown v1 fields and ambiguous omission semantics.
- Set a release deadline and removal issue for every adapter.

### Phase 4: Consumer cutover

- Update Rust consumers first.
- Update CLI JSON output and RPC golden vectors.
- Update generated specs, constitution references, fixtures, docs, and external examples.
- Require each consumer to assert the v2 contract ID/version and structured error behavior.
- Run old/new replay comparisons for deterministic operations.

### Phase 5: Remove v1

- Require adapter usage counters to be zero for one complete release window.
- Remove v1 schemas, translation code, compatibility flags, and old golden vectors.
- Keep a migration note and archived fixtures for forensic replay.
- Bump the major contract version and publish the final removal evidence.

## Breaking-change inventory

The implementation follow-up must explicitly handle these changes:

| Existing behavior | v2 behavior | Required evidence |
|---|---|---|
| Operation-specific response top-level fields | One envelope with typed outcome | JSON schema, CLI/RPC golden vectors |
| Optional/missing version fields | Required semantic contract version | Version negotiation and rejection tests |
| Free-form validation messages | Stable error code/category/retryability | Error matrix and consumer branch tests |
| success plus unrelated status fields | outcome.status owns terminal state | State-machine tests |
| Implicit repo/session/task scope | Required scope object where applicable | Authorization and workspace tests |
| Module-local producer assumptions | Explicit producer/consumer participants | Ownership and contract inventory |
| Repeated internalization result envelopes | Shared envelope plus typed payload | Five-result replay and schema tests |
| Constitution node IDs used as contracts | Versioned contract IDs with a mapping | Lookup, embedded output, and migration tests |
| Generated spec hashes updated as a side effect | Spec refresh is a declared contract operation | Manifest and release validation |
| Error messages used as behavior signals | Machine branching on stable error codes | Negative-path conformance tests |
| Durable entities without common revision semantics | Entity revision/lifecycle fields | Conflict, supersede, and rollback tests |

## Compatibility, deprecation, and rollback

### Compatibility

Compatibility is a migration property, not a permanent architecture:

- v1 and v2 may coexist only at the adapter edge;
- the adapter must never write a second canonical state store;
- v1-to-v2 translation must preserve hashes, identity, scope, and failure meaning;
- lossy translation must fail closed, not guess;
- a v2 response must identify whether an adapter was involved;
- compatibility behavior must be disabled by default after the announced cutover release.

### Deprecation

Each contract records:

- introduced_version;
- deprecated_after;
- removal_release;
- replacement_contract_id;
- adapter usage metrics;
- owner and migration issue.

### Rollback

Rollback happens at release boundaries, not through hidden runtime branching:

1. stop rollout if conformance or replay comparisons fail;
2. preserve the v2 state and migration receipt;
3. restore the previous binary/CLI release;
4. replay v2-to-v1 only through a tested reverse adapter where lossless;
5. if reverse translation is lossy, restore from the pre-migration snapshot and mark affected contracts blocked;
6. record the rollback event and evidence in the release proof.

No migration may delete v1 state before the rollback window closes.

## Proof plan

### Static proof

- Prototype schema parses as deterministic JSON.
- Every v2 contract has unique ID/version/kind/owner.
- All required envelope fields are present and additionalProperties is false.
- Contract inventory has no unowned producer/consumer.
- Constitution and generated-spec references resolve.
- No new v1 consumer bypasses the adapter.

### Runtime proof

- Each command family returns the common envelope on success, validation failure, authorization failure, not-found, conflict, expiry, blocked, and internal error.
- Replay of deterministic requests produces identical canonical output hashes.
- Scope and ownership checks reject cross-repository/session/task access.
- Internalization create/attach/detach/inspect preserve lease and manifest semantics.
- Adapter translations preserve identity, scope, errors, and proof references.
- Concurrent revision and lease conflicts fail deterministically.

### Release proof

- CI runs schema, contract-conformance, golden-vector, migration, replay, daemonless-lifecycle, and rollback tests.
- Generated specs and entrypoint fingerprints are refreshed after contract changes.
- Release notes list breaking contract IDs, adapter deadline, migration command, and rollback procedure.
- The removal release proves zero adapter use for the required window.
- Issue/PR evidence links the consumer inventory, schema hash, test output, and validation epoch.

## Follow-up implementation issue

This design issue is complete when the design artifact, prototype schema, consumer inventory, and proof plan are reviewed. The breaking implementation must land in a separate issue/PR that references this document and #938. That implementation issue must not be closed by a passing compile alone; it must satisfy the full proof plan above.
