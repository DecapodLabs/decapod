# INTERNALIZATION_SCHEMA.md - Internalized Context Artifact Contract

**Authority:** interface (machine-readable contract)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** schema, invariants, and lifecycle for internalized context artifacts
**Non-goals:** internalizer implementation details, model training

---

## 1. Purpose

Internalized context artifacts let agents convert long documents into mountable, verifiable context adapters. This eliminates redundant long-context ingestion across sessions while maintaining full auditability.

An internalization is **not training**. It is a governed artifact produced by a pluggable external tool (an "internalizer profile") and managed by Decapod's artifact lifecycle.

---

## 2. Artifact Layout

```text
.decapod/generated/artifacts/internalizations/<artifact_id>/
  manifest.json       # InternalizationManifest (see schema below)
  adapter.bin          # adapter payload (or pointer)
```

---

## 3. InternalizationManifest Schema (v1.0.0)

```json
{
  "schema_version": "1.0.0",
  "id": "<ULID>",
  "source_hash": "<SHA-256 of source document>",
  "source_path": "<original path or URI>",
  "extraction_method": "<profile name>",
  "chunking_params": {},
  "base_model_id": "<model identifier>",
  "internalizer_profile": "<profile name>",
  "internalizer_version": "<semver>",
  "adapter_format": "<format string>",
  "created_at": "<ISO 8601>",
  "ttl_seconds": 0,
  "expires_at": "<ISO 8601 | null>",
  "provenance": [
    {
      "op": "internalize.create",
      "timestamp": "<ISO 8601>",
      "actor": "<actor id>",
      "inputs_hash": "<SHA-256>"
    }
  ],
  "replay_recipe": {
    "command": "decapod",
    "args": ["internalize", "create", "--source", "..."],
    "env": {}
  },
  "adapter_hash": "<SHA-256 of adapter payload>",
  "adapter_path": "adapter.bin",
  "capabilities_contract": {
    "allowed_scopes": ["qa", "summarization"],
    "permitted_tools": ["*"],
    "allow_code_gen": false
  },
  "risk_tier": {
    "creation": "compute-risky",
    "attach": "behavior-changing",
    "inspect": "read-only"
  }
}
```

---

## 4. Result Schemas

### InternalizationCreateResult

```json
{
  "schema_version": "1.0.0",
  "success": true,
  "artifact_id": "<ULID>",
  "artifact_path": "<absolute path>",
  "manifest": { "...InternalizationManifest..." },
  "source_hash": "<SHA-256>",
  "adapter_hash": "<SHA-256>"
}
```

### InternalizationAttachResult

```json
{
  "schema_version": "1.0.0",
  "success": true,
  "artifact_id": "<ULID>",
  "session_id": "<session identifier>",
  "attached_at": "<ISO 8601>",
  "expires_at": "<ISO 8601 | null>",
  "capabilities_contract": { "...CapabilitiesContract..." },
  "risk_classification": "behavior-changing",
  "provenance_entry": { "...ProvenanceEntry..." }
}
```

### InternalizationInspectResult

```json
{
  "schema_version": "1.0.0",
  "artifact_id": "<ULID>",
  "manifest": { "...InternalizationManifest..." },
  "integrity": {
    "source_hash_valid": true,
    "adapter_hash_valid": true,
    "manifest_consistent": true,
    "expired": false
  },
  "status": "valid"
}
```

---

## 5. Invariants

1. **Source binding:** `source_hash` must be the SHA-256 of the document at creation time. No silent changes.
2. **Base model binding:** `base_model_id` must be recorded; adapters are model-specific.
3. **Reproducibility:** `internalizer_profile` + `internalizer_version` + `replay_recipe` must be sufficient to reproduce the artifact.
4. **Explicit attach:** Agents cannot reference an internalization without a logged `internalize.attach` operation.
5. **TTL enforcement:** If `expires_at` is set and in the past, `attach` MUST fail.
6. **Adapter integrity:** `adapter_hash` must match the SHA-256 of the payload file at attach time.
7. **Provenance logging:** Every `attach` operation appends a provenance entry to the session directory.

---

## 6. Risk Classification

| Operation | Risk Level | Rationale |
|-----------|-----------|-----------|
| `create` | compute-risky | Invokes external tool; no repo mutation beyond artifact dir |
| `attach` | behavior-changing | Affects inference behavior; logged as dependency |
| `inspect` | read-only | No side effects |

---

## 7. Internalizer Profiles

Profiles are pluggable external tools stored in `.decapod/profiles/internalizers/<name>.json`.

Profile schema:
```json
{
  "name": "<profile name>",
  "version": "<semver>",
  "executable": "<path or builtin:noop>",
  "default_params": {},
  "adapter_format": "<format string>"
}
```

The built-in `noop` profile produces an empty adapter for pipeline testing without GPU dependencies.

---

## Links

- `core/PLUGINS.md` - Subsystem registry
- `core/INTERFACES.md` - Interface contracts registry
