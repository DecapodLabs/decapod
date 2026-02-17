# KNOWLEDGE.md - KNOWLEDGE Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

KNOWLEDGE stores contextual memory with provenance pointers.
It is append-first context for decisions, lessons, and execution rationale.

## CLI Surface
- `decapod data knowledge add --id <id> --title <t> --text <body> --provenance <ptr> [--claim-id <id>] [--merge-key <k>] [--on-conflict merge|supersede|reject] [--ttl-policy ephemeral|decay|persistent]`
- `decapod data knowledge search --query <q> [--as-of <epochZ>] [--window-days <n>] [--rank recency_desc|recency_decay]`
- `decapod data knowledge retrieval-log --query <q> --returned-ids <csv> [--used-ids <csv>] --outcome helped|neutral|hurt|unknown`
- `decapod data knowledge decay [--policy default] [--as-of <epochZ>] [--dry-run]`
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
