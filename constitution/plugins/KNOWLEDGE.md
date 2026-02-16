# KNOWLEDGE.md - KNOWLEDGE Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

KNOWLEDGE stores contextual memory with provenance pointers.
It is append-first context for decisions, lessons, and execution rationale.

## CLI Surface
- `decapod data knowledge add --id <id> --title <t> --text <body> --provenance <ptr> [--claim-id <id>]`
- `decapod data knowledge search --query <q>`
- `decapod data schema --subsystem knowledge`

## Contracts
- Provenance is required and must use supported schemes (`file:`, `url:`, `cmd:`, `commit:`, `event:`).
- Knowledge writes are brokered (`knowledge.add`) and auditable.
- Knowledge must not directly mutate health state.
- Lessons from autonomy loops are recorded through knowledge and mirrored into federation where configured.

## Proof Surfaces
- Storage: `<store-root>/knowledge.db`
- Audit: `<store-root>/broker.events.jsonl` with `knowledge.*` ops
- Validation gates:
  - Knowledge Integrity Gate
  - Control Plane Contract Gate
