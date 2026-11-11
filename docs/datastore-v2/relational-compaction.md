# Compact relational storage: verified first cut

Status: incremental, candidate source only; full governance validation, published binaries,
and hosted Propodus/Supabase proof are **not** implied by this document.

## Design boundary

Decapod owns the canonical **logical** model: task/custody transitions, immutable
events, authoring/verification/obligation bindings, knowledge and operator input.
Local SQLite is one repository-scoped file, so it needs no repeated tenant ID.
Propodus is a shared PostgreSQL service, so its physical layout must bind every
authoritative row to a compact, server-derived repository key. Dactyl owns
physical execution, never authority or schema policy.

Physical DDL cannot be byte-identical across a single-repo SQLite file and a
multi-tenant Supabase database without adding waste to local mode. Parity means
identical domain data, constraints, transition semantics, and proof, with
explicitly classified cloud-only identity/billing/entitlement tables.

## Increment: five redundant SQLite indexes

The following historical nonunique btrees duplicate an existing SQLite UNIQUE
btree's leftmost search key:

| Remove | Authoritative replacement |
| --- | --- |
| `idx_categories_name(name)` | `categories UNIQUE(name)` |
| `idx_risk_zones_name(zone_name)` | `risk_zones UNIQUE(zone_name)` |
| `idx_task_owners_task(task_id)` | `idx_task_owners_task_agent UNIQUE(task_id,agent_id)` |
| `idx_task_dependencies_task(task_id)` | `task_dependencies UNIQUE(task_id,depends_on_task_id)` |
| `idx_preferences_category(category)` | `preferences UNIQUE(category,key)` |

A new migration removes each obsolete btree only after checking that its exact
definition is nonunique, nonpartial, ascending/BINARY and matches the intended
key, and that an ascending/BINARY full UNIQUE replacement exists in the same
table. The entire operation runs under `BEGIN IMMEDIATE`, rolls back on
unrecognized definitions, and never modifies any row. New initialization paths
do not recreate the obsolete trees. Reverse-lookup indexes and
`idx_preferences_key` remain: their access paths are not supplied by the
leftmost prefix.

DROP frees pages for reuse. It does not guarantee a smaller existing SQLite
file until a separately governed maintenance operation rebuilds it. No VACUUM,
WAL, synchronous, journal, row format, proof hash, or compaction schedule change
is part of this increment. Savings and runtime speedups remain unmeasured until
the dedicated benchmark is executed.

## Next physical normalization: Propodus, separately gated

Propodus currently carries two different schema contracts: the native
`internal/backend/schema/001..006` migration ledger and an older Decapod
v0.98.1 catalog under `pkg/dactyl`. The native schema covers only a fraction of
Decapod's present local runtime; treating the old broad compatibility DDL as
native feature parity is unsafe.

The next versioned cloud migration should establish a compact integer
`repository_key` in the server-owned repository directory, reference it from
operational tables, and resolve organization membership and grants at the
authorization boundary. A repository's organization and long canonical name
should be stored **once** in the directory, not repeated on every task/event.
Do not copy auth sessions, entitlements, tenant metadata, or repeated
organization strings into local SQLite. Preserve typed external IDs and all
existing domain fields in the logical interface.

Do not rewrite cloud physical keys until every RPC statement, RLS policy,
event stream (including non-task subjects), receipt, legacy adapter and
cross-tenant negative test has a versioned compatible replacement.
Use add/backfill/validate/switch/retire migrations; preserve applied migration
checksums. Test old/new client overlap and rollback/unknown outcomes.

Acceptance for the complete program is a generated catalog diff with no
unclassified differences, cross-backend operation golden vectors for all
runtime entities, no proof or custody regression, and measured p50/p95 latency,
storage bytes per task/event, write amplification, and tenant isolation
across many repositories. One indexed table is never an adequate proxy for
governance correctness.
