# Optional Laya assessment

Laya assesses declared intent and bounded validation evidence after deterministic
proof succeeds. It is disabled by default. A required assessment must affirm the
current bound candidate before an already-authorized PR step can proceed.
Rejection, abstention, stale evidence and runtime failure require repair and
revalidation. Assessment never grants merge or release authority.

Enable it explicitly with `decapod init --laya-decisions`, or use the reviewed
`[laya]` configuration. Initialization preserves existing Laya policy and private
installation references. Unrecognized user extension tables remain opaque data;
only `[laya]` controls model assessment. Initialization does not download models.

`decapod setup laya` installs or verifies the pinned external model cache. It
accepts `--cache-directory`, `--offline`, and `--check-updates`. Register a native
worker with `--runtime <path>` and an optional independently pinned CPU companion
with `--cpu-runtime <path>`. These explicit operations do not establish numerical
qualification or model accuracy.

Private registration uses `decapod.laya-local-runtime.v2`. Older or unknown
registrations fail with `laya_local_binding_version_unsupported` and remain
untouched. Re-register through `decapod setup laya --runtime <path>` using the
reviewed executable; supply the CPU companion again when configured. Model
manifest identity, runtime bytes and historical proof bodies are not rewritten.

The worker protocol and `validation-results.v3` projection retain their existing
versions. The full input must fit its declared bounds; obligations are never
silently truncated. Material, accepted policy, governance and proof bindings are
checked around assessment. Replay uses the same projector but supplies no live
authorization or custody. Four captured input hashes protect byte compatibility.

The current JSON validation report exposes `laya_readiness` as its sole model
field. Removed evaluator configuration, report fields and Rust types are no
longer compatibility interfaces. Historical raw reports and canonical proof
artifacts remain in Git or their existing evidence stores. Generic historical
replay normalization remains unchanged.
