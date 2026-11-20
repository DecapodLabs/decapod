# Portable JSON interpretation profile 1.0.0

Status: normative for the **encoding plane only**. This is a focused artifact
for #870/#1733, not the complete governance-envelope profile.

## Scope and authority

The inputs are raw JSON parameter texts. The independent Python decoder and
Decapod's real RPC boundary must agree on `accepted` or `invalid_json`.
Acceptance means only that decoding succeeded. Typed operation schemas,
negotiation, producer trust, task custody, currentness, approval and publication
remain separate kernel decisions. Neither decoder grants authority.

The pinned vectors live in
`tests/fixtures/interop_json_vectors.v1.json`, described by
`assets/schemas/interop-json-vectors.schema.json`. Raw inputs are strings:
parsing the surrounding vector document must not parse, normalize or replace
the embedded text before either consumer sees it. Expected answers are test
oracles; neither consumer receives them.

## Interpretation rules

1. Decode complete UTF-8 JSON text with no trailing non-whitespace input.
   Reject malformed syntax, non-JSON numeric constants and non-finite numeric
   overflow. Whitespace allowed by JSON remains valid.
2. Reject duplicate **decoded** object keys at every depth, including keys made
   equal by escape decoding. Do not Unicode-normalize distinct scalar keys.
3. Strings and keys contain Unicode scalar values. Reject unpaired surrogate
   escapes; accept valid surrogate pairs, ordinary Unicode and escaped control
   characters. Reject literal unescaped control characters.
4. A document has at most 127 nested containers, counting its root object or
   array. An object value or array element that is another container increases
   depth by one. Scalar leaves do not increase container depth.
5. Native integers in the signed64/unsigned64 range remain integers. Larger
   integer tokens must fit a finite binary64 value, as in the native decoder.
   This plane specifies decoding/refusal, **not** canonical number spelling,
   arbitrary-precision preservation or a cross-language canonical hash.
   Bind evidence to original bytes; do not substitute reserialized JSON.

The reference decoder is `tests/fixtures/interop_strict_json.py`; it requires
only Python's standard library and has no Decapod import, database, receipt
producer or runtime dependency. Invoke it as
`python3 -I -S -B interop_strict_json.py '<raw JSON>'` or use `--stdin`.
Its output is a reason class, not an authorization.

## Execution and reporting

The existing unfiltered `interop_conformance` target runs every pinned input
through the independent decoder and the actual Decapod CLI. Invalid inputs
must be rejected before project/session startup; valid inputs execute the
existing read-only projection operation. Valid Unicode and boundary-sized
inputs are positive twins, so tightening the reference decoder cannot simply
reject everything. The versioned report records actual outputs, both executable
versions/hashes, vector/schema hashes and explicit coverage limits.

The report belongs to the existing conformance report directory. It is
diagnostic execution evidence, never a native validation receipt, proof of
independent-host execution, or evidence of imported human consent.

## Compatibility and remaining domains

A changed interpretation requires a new profile/vector version and preserved
old results. Additive vectors may be appended when they express these same
rules. Unknown mandatory encoding semantics require an explicit refusal;
optional transport annotations must not change the raw parameter text.

This focused profile does not establish replay protection, cross-tenant
authority attenuation, URL/path/shell interpretation, normalized governance
envelopes, authenticated executed-conformance advertisement, or independent
host trust. Those original #870/#871/#1733 obligations remain open. Existing
CLI/RPC negotiation and all governance checks remain unchanged.
