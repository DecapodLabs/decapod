# STATE_COMMIT v1 Protocol

## Overview

STATE_COMMIT is Decapod's cryptographic commitment protocol. It binds an agent's claimed work to the exact state of the repository at a specific commit, enabling offline verification and proof-based promotion gates.

## Core Invariant

> Decapod doesn't just gate actions; it cryptographically commits to the exact state an agent claims to have produced—then verifies it.

## Inputs

- `base_sha`: Commit SHA of the base revision (e.g., origin/main merge-base)
- `head_sha`: Commit SHA of the HEAD revision (the work being committed)
- `constitution_ref`: Commit SHA of the constitution (governance snapshot)
- `ignore_policy_hash`: SHA256 of the resolved ignore patterns from constitution
- Git object database: All blob contents at `head_sha`

## Canonical Encoding

### CBOR Schema (Integer Keys)

```
Map {
  1: algo_version      (tstr)  - "state_commit.v1"
  2: base_ref          (tstr)  - base commit SHA
  3: head_sha          (tstr)  - head commit SHA
  4: diff_mode         (uint)  - 1 = committed-only
  5: ignore_policy_hash (tstr) - SHA256 of ignore patterns
  6: entries           (array) - sorted list of entry tuples
}

Entry tuple: [path, kind, mode_exec, content_hash, size]
  - path:      repo-relative path (UTF-8)
  - kind:      0 = file, 1 = symlink
  - mode_exec: bool (true if mode == 100755)
  - content_hash: SHA256 of blob content
  - size:      uint (bytes)
```

### Canonicalization Rules

1. **Definite-length** maps and arrays only
2. **Map keys sorted** by numeric value (1, 2, 3, 4, 5, 6)
3. **No floats** in the encoding
4. **Shortest encoding** for integers
5. **UTF-8 strings** only
6. **Byte strings** for hashes

### Path Handling

- Paths are **repo-relative**
- Paths are sorted **lexicographically by raw bytes** (not locale-dependent)
- No normalization (git output bytes are canonical)

## Hash Computation

### Leaf Hash

```
leaf_bytes = CBOR([path, kind, mode_exec, content_hash])
leaf_hash = SHA256(leaf_bytes)
```

### Merkle Tree

Binary Merkle tree over sorted leaf hashes:
1. Sort entries by path (lexicographic byte order)
2. Compute leaf_hash for each entry
3. Build tree: hash pairs as SHA256(left || right)
4. If odd number of leaves, duplicate last leaf
5. Root is the single remaining hash

### Output Values

Two distinct hashes are produced:

1. **`scope_record_hash`**: SHA256 of the canonical CBOR scope_record bytes
   - Used for artifact integrity
   - Changes if any byte in scope_record changes

2. **`state_commit_root`**: Merkle root of entry leaf hashes
   - Used for state commitment verification
   - Changes if any file content, path, or metadata changes

## Verification Contract

A STATE_COMMIT proof is valid if and only if:

1. scope_record.cbor exists and is readable
2. scope_record_hash matches SHA256(scope_record_bytes)
3. head_sha in record matches current HEAD
4. constitution_ref matches current constitution snapshot
5. ignore_policy_hash matches current ignore policy
6. state_commit_root recomputed from git objects matches recorded root
7. All verifications performed **offline** (no network)
8. All content read from **git objects** (not filesystem)

## Failure Modes

| Failure | Cause | Remediation |
|---------|-------|-------------|
| Root mismatch | Files changed since proof | Regenerate with `decapod state-commit prove` |
| HEAD mismatch | Commit changed | Regenerate proof at new HEAD |
| Missing scope_record | Proof not captured | Run `decapod state-commit prove` |
| Constitution drift | Governance policy changed | Update constitution_ref in proof |

## Golden Vectors (v1)

Immutable test vectors for cross-platform verification:

```
tests/golden/state_commit/v1/
├── scope_record.cbor         (canonical bytes)
├── scope_record.cbor.hex     (hex representation)
├── scope_record_hash.txt     (sha256 of bytes)
└── state_commit_root.txt     (Merkle root)

Expected values:
- scope_record_hash: 41d7e3729b6f4512887fb3cb6f10140942b600041e0d88308b0177e06ebb4b93
- state_commit_root: 28591ac86e52ffac76d5fc3aceeceda5d8592708a8d7fcb75371567fdc481492
```

## Immutability

STATE_COMMIT v1 is a **protocol**, not just a feature. Any byte-level change to the encoding, hashing, or verification rules requires a **SPEC_VERSION bump to v2**.

Golden vectors in `v1/` must never change. If they do, it's a protocol violation.

## CLI Commands

```bash
# Generate proof
decapod state-commit prove --base <sha> --head <sha> --output scope_record.cbor

# Verify proof
decapod state-commit verify --scope-record scope_record.cbor --expected-root <hash>

# Explain proof contents
decapod state-commit explain --scope-record scope_record.cbor
```

## Promotion Integration

To require STATE_COMMIT for promotion:

1. Set proof_plan to `["validate_passes", "state_commit"]`
2. Capture scope_record.cbor as proof artifact
3. On promotion, Decapod will:
   - Recompute root from git objects at current HEAD
   - Verify against expected_root
   - Fail promotion if mismatch

## Security Properties

- **Deterministic**: Same inputs always produce same outputs
- **Offline-verifiable**: No network required for verification
- **Binding**: Proof is bound to specific HEAD + constitution
- **Tamper-evident**: Any file change invalidates the proof
- **Reproducible**: Anyone can recompute and verify

## Version History

- **v1** (Current): Initial protocol with CBOR encoding, Merkle trees, and offline verification

## References

- `src/core/state_commit.rs` - Core implementation
- `tests/golden/state_commit/v1/` - Golden vectors
- `src/core/validate.rs` - Validation gate
- `src/plugins/verify.rs` - Promotion verification
