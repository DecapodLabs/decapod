# `@decapodlabs/decapod-client`

Library-safe TypeScript binding for the **Decapod** governance kernel (`#1277`).

> **Architecture note:** This package is a typed binding and transport client over the Rust `decapod` core—not a TypeScript reimplementation of governance policy. All authority, safety evaluation, session custody, and proof verification remain owned by the Rust kernel.

## Supported Platforms & Runtime

| Target | Status | Transport |
| --- | --- | --- |
| macOS (`arm64`, `x64`) | Supported | Verified CLI/RPC process transport (`cli_process` fallback reported explicitly until prebuilt N-API artifacts are published) |
| Linux glibc (`x64`, `arm64`) | Supported | Verified CLI/RPC process transport |
| Node.js | `>= 22.6.0` | Native ESM + TypeScript strip-types (`node --test`) |

## Guarantees

- Zero `process.exit` calls and zero mutable global session state.
- Explicit per-client `cwd`, `agentId`, and `sessionPassword` isolation across concurrent invocations.
- Per-call `AbortSignal` cancellation and bounded timeout enforcement.
- Explicit `transportInfo()` visibility (`native_napi` vs `cli_process`)—never silently downgrading transport state.
- No raw SQL, arbitrary filesystem, or secret serialization surfaces.

## Verification

```bash
npm run typecheck
npm test
DECAPOD_TEST_BIN=/path/to/decapod npm test
```
