# AGENTS.md - Decapod Entrypoint

**Canonical:** AGENTS.md
**Authority:** entrypoint
**Layer:** Guides
**Binding:** No

This repo is Decapod-managed.

## 1. Decapod Constitution (Embedded)
The Decapod methodology is built into the `decapod` binary.
- List available docs: `decapod docs list`
- Read the constitution: `decapod docs show core/DECAPOD.md`
- **Ingest for Agentic Memory:** `decapod docs ingest` (dumps all embedded docs)

## 2. Project Living Docs (Mutable)
Maintain and follow the project-specific intent and architecture here:
- `.decapod/constitutions/specs/INTENT.md`
- `.decapod/constitutions/specs/ARCHITECTURE.md`

## 3. Tooling
Use Decapod as the shared interface:
- `decapod todo ...`
- `decapod validate`

## 4. Overrides
Review `.decapod/README.md` for guidance on overriding embedded methodology.
Any time you run and want to trigger `decapod`, review the `.decapod/constitutions/` overrides.
