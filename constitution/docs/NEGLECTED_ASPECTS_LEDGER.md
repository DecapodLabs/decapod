# Neglected Aspects Ledger

## Phase 0: Interface Surface Scan

Key surfaces:

- Product docs: `README.md`, `docs/`
- Control plane code: `src/lib.rs`, `src/core/rpc.rs`, `src/core/workspace.rs`
- Constitution contracts: `constitution/interfaces/*`, `constitution/core/*`
- Proof/tests: `tests/*`, golden vectors
- Templates now embedded in Rust via `template_agents()`, `template_named_agent()`

## Phase 1: Gap Map

| Area | Status Before | Status After |
|---|---|---|
| Product positioning | under-specified | hardened README + docs landing |
| Interop contract | partial | explicit API/stability policy + vectors |
| Security/provenance | partial | threat model + publish provenance gate |
| Release lifecycle | partial | release policy + `decapod release check` |
| Templates/ergonomics | sparse | session bootstrap + template set |
| Integration demos | missing | Rust-native CLI/RPC demo coverage + tests |

## Top 3 Risks If Left Weak

1. Integration failure: no stable shim contract for external agent frameworks.
2. Trust failure: claims without reproducible provenance chain.
3. Drift failure: release/process changes silently breaking operators.
