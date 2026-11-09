# Increment 2: idempotent aptitude defaults

`initialize_aptitude_db` previously generated four new IDs on every call and
used `INSERT OR IGNORE` to seed `agent_prompts`. Only the ID is unique, so each
call inserted four additional copies. Repeated startup could also introduce an
active copy after a user disabled an earlier default.

Initialization now conditionally inserts each missing `(context, prompt_text)`
pair in a single SQLite statement, using the existing context index. SQLite's
writer serialization makes the existence check and insert atomic. There is no
separate read-before-write round trip, new table/index, migration or global
uniqueness rule on custom prompts. Custom insertion still permits repeated text.

Existing rows are never deleted, merged, updated or assigned new IDs. Historical
duplicates, priorities, disabled flags, usage counts and timestamps remain
unchanged. A missing default is restored once. A source change to default text
would introduce a distinct seed; this increment does not change that text.
Local/cloud capability boundaries and all proof encodings are unchanged.

## Measured evidence

The baseline ran the accepted v0.133.2 public initializer before the runtime
change. A preservation test failed against that baseline, then passed after
the fix. The same disposable fixture ran initialization once plus 128 repeated
calls in each variant, with SQLite 3.54.0 and the existing DELETE journal mode.
No runtime pragmas or broker audit requirements changed.

| Metric after 128 repeated calls | Before | After |
|---|---:|---:|
| Default prompt rows | 516 | 4 |
| Newly inserted duplicate rows | 512 | 0 |
| Prompt table plus its two indexes | 131,072 B | 12,288 B |
| Complete packed fixture database | 720,896 B | 602,112 B |
| p50 initializer latency | 78.02 ms | 75.77 ms |
| p95 initializer latency | 85.46 ms | 99.44 ms |
| p99 initializer latency | 94.57 ms | 128.42 ms |

This prevents 512 redundant inserts and 118,784 bytes of growth in the declared
workload: 90.63% less prompt-object storage, 25% less total growth above the
245,760-byte initial fixture, or 16.48% less final fixture storage. It does not
shrink an existing installation or remove required evidence. Both unvacuumed
and packed byte counts are retained in `aptitude-measurements.json`.

The single shared-host timing comparison has worse tail latency despite slightly
better p50. It does **not** establish a latency improvement or the broader
no-regression goal; repeated controlled latency qualification remains pending.
The four seed SQL statements still execute; their affected-row count becomes
zero when defaults already exist. Broker audit writes and other initializer
costs remain. CPU, allocations, WAL traffic, hosted operation and end-to-end CLI
cold start are not measured by this fixture.

`repeated_initialization_retains_ids_disabled_state_and_custom_history` checks
all persisted prompt fields, including a preexisting duplicate and a custom row.
`initialization_adds_only_a_missing_default` checks selective restoration.
`default_seeding_is_atomic_under_64_independent_sqlite_connections` races actual
production seed statements through 64 separate SQLite connections and verifies
one copy per default, while still allowing explicit duplicate custom prompts.
These tests do not qualify exclusive task claims or hosted concurrency.

```sh
cargo test --test aptitude_initialization
cargo test --lib plugins::aptitude::seed_tests
DECAPOD_APTITUDE_REPORT=/absolute/new/output.json \
  cargo test --test aptitude_initialization measure_repeated_initializer \
  -- --ignored --nocapture
```

The output file must not already exist. Fixtures are disposable; VACUUM is used
only to compare fixture packing and is not a live maintenance operation. This
change is independent of the event-index compaction PR. Full exact-candidate
repository validation remains required before publication.

The v0.134.0 port is based on accepted `1d865efd`. The accepted aptitude source is unchanged from v0.133.2, and the runtime/test patch is preserved from `9ae236fa`. The table above remains the original measurement, with current-candidate correctness and publication proof recorded separately.
