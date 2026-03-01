# INTERNALIZATION_SCHEMA.md - Internalized Context Artifact Contract

**Authority:** interface (machine-readable contract)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** schema, invariants, CLI lifecycle, and proof gates for internalized context artifacts
**Non-goals:** model training, hidden memory, background services

---

## 1. Purpose

Internalized context artifacts let agents reuse long-document context without re-sending the full document on every call.

An internalization is **not training** and **not hidden state**. It is a governed repo-local artifact produced on demand by a pluggable profile tool, bound to exact source bytes, and attachable only through an explicit lease-bearing mount step.

---

## 2. Capability Decision + Scope

### Added

One capability family: `internalize.*`

- `internalize.create` creates or reuses a content-addressed internalization artifact.
- `internalize.attach` creates a session-scoped mount lease with explicit expiry.
- `internalize.detach` revokes the mount explicitly before lease expiry.
- `internalize.inspect` proves exact bindings, integrity status, and determinism labeling.

### Not Added

- No background daemon or auto-mounting.
- No silent GPU dependency.
- No implicit session reuse across tools.
- No claim that best-effort profiles are replayable.
- No general-purpose ambient memory layer.

---

## 3. Artifact Layout

```text
.decapod/generated/artifacts/internalizations/<artifact_id>/
  manifest.json
  adapter.bin
```

Session-scoped active mount leases are stored at:

```text
.decapod/generated/sessions/<session_id>/internalize_mounts/
  mount_<artifact_id>.json
```

---

## 4. Manifest Contract

Schema version: `1.2.0`

Required fields include:

- `source_hash`
- `base_model_id`
- `internalizer_profile`
- `internalizer_version`
- `adapter_hash`
- `determinism_class`
- `binary_hash`
- `runtime_fingerprint`
- `replay_recipe`
- `capabilities_contract`

Determinism rules:

- `determinism_class` is `deterministic` or `best_effort`
- only deterministic profiles may claim `replay_recipe.mode=replayable`
- best-effort profiles must be `non_replayable`
- best-effort manifests must carry `binary_hash` and `runtime_fingerprint`

Capabilities rules:

- default scope is `qa`
- `allow_code_gen=false` by default
- attach must enforce `permitted_tools`

---

## 5. CLI Surface

### `decapod internalize create`

Creates or reuses a content-addressed artifact from:
- `--source`
- `--model`
- `--profile`
- `--ttl`
- `--scope`

### `decapod internalize attach`

Creates a session-scoped mount lease from:
- `--id`
- `--session`
- `--tool`
- `--lease-seconds`

### `decapod internalize detach`

Revokes the session-scoped mount lease:
- `--id`
- `--session`

### `decapod internalize inspect`

Proves artifact status:
- `valid`
- `best-effort`
- `expired`
- `integrity-failed`

---

## 6. Provable Acceptance Criteria

An internalization is provable only if:

1. `source_hash` binds to exact source bytes.
2. `base_model_id` is recorded.
3. `adapter_hash` matches the adapter payload.
4. replayability claims match determinism policy.
5. use requires a successful attach lease.
6. expired artifacts cannot be attached.
7. expired mount leases fail validation if left active.
8. the attach tool is allowed by `permitted_tools`.

---

## 7. Stable JSON Schemas

- `constitution/interfaces/jsonschema/internalization/InternalizationManifest.schema.json`
- `constitution/interfaces/jsonschema/internalization/InternalizationCreateResult.schema.json`
- `constitution/interfaces/jsonschema/internalization/InternalizationAttachResult.schema.json`
- `constitution/interfaces/jsonschema/internalization/InternalizationDetachResult.schema.json`
- `constitution/interfaces/jsonschema/internalization/InternalizationInspectResult.schema.json`
