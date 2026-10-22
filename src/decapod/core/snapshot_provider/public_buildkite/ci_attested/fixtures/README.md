# CI-attested metadata regression fixtures

These are **minimized and command-redacted captures**, not raw HTTP responses,
execution archives, or evidence that this candidate is publication-ready. The
regressions run the production collector against an exact three-GET fake reader.
Any artifact, log, redirect, duplicate request, or other URL is rejected.

`legacy-activation.toml` preserves the optional provider-metadata policy from
accepted v0.121.0 source `d05c43746830c4829f54f23c339ebcdce40a68fb`. It contains
only the historical proof setting and five exact workflow policies, without
session, machine or authentication state. Its regression still checks the entire
independently frozen catalog. The repository now selects the existing strict
preparation/step/artifact collector; a separate current-configuration regression
prevents accidentally reactivating metadata-only proof. Neither fixture proves
hosted execution or current-candidate readiness.

## Provider sources

The original inputs came from these public Buildkite metadata routes, without
artifact or log downloads:

- https://buildkite.com/decapodlabs/decapod/builds/1840.json
- https://buildkite.com/decapodlabs/decapod/builds/1840/data/steps
- https://buildkite.com/decapodlabs/decapod/builds/1840/data/jobs?include_retried_jobs=true&paginate=false
- https://buildkite.com/decapodlabs/decapod/builds/1870.json
- https://buildkite.com/decapodlabs/decapod/builds/1870/data/steps
- https://buildkite.com/decapodlabs/decapod/builds/1870/data/jobs?include_retried_jobs=true&paginate=false

Build 1840:

- Run UUID: `01a14183-310b-4426-8bac-b391137102d4`
- Source: `c444ff9b03ae569a268f348c898f0dccfa08b5e4`
- Branch: `agent/laya-readiness/cicd_01m50ns7bafppa7k`
- State: passed, 39 native jobs

Build 1870:

- Run UUID: `01a14205-d31a-4ae0-9fb2-c92b61248f00`
- Source: `bcb5842ff5d1dba6a8697b1cffe5deb1d135f701`
- Branch: `agent/laya-readiness/code_01m50zemd9zw1swe`
- State: failed, 39 native jobs; release-policies integration shard and
  integration-status both actually failed with exit status 1

Both bind pipeline `01a0bcf1-5e16-44ae-8457-04495fae8ff5`, repository
`DecapodLabs/decapod`, workflow `.github/workflows/ci.yml`, and event `push`.
Each has one keyless, null-command root bootstrap, two branch-dependent
concurrency gates, and 36 imported workflow jobs. Native step state is
`finished`; successful step outcome is `passed`. Display labels that include
`(skipped)` are preserved; provider-native job success does not independently
prove that every source-level check executed. Source applicability is handled
by the separate required catalog, under the explicitly accepted executor trust.

SHA-256 of the original input bodies before minimization:

| Input | SHA-256 |
| --- | --- |
| 1840 envelope | `105906a0c48c682750f24fbf4ead3c41d126a41b1f125c48951cbdbef3d9adce` |
| 1840 steps | `2f02f455e74014d7bf5bf8626a9fb258a2e1fa328147417a0bbfee4df99e7fca` |
| 1840 jobs | `49ba5894ff036d81efc9b543a9ec48e9d5f8be1c637410201b213055c62229fd` |
| 1870 envelope | `ff47d8fa5aa216a65ecca296fe7215b2248501940b89b4d25d740794fa4a1e34` |
| 1870 steps | `2a39ea83be5af78eb1f09eda84a8477f25342d720cdd0d2c6574e96b221763c0` |
| 1870 jobs | `18faa4e76b5b155a2002ce0fd26811d2279416d04b3abce2516f37d787274645` |

## Minimization

All 39 rows and their actual build, run, step and job identifiers remain in each
capture. Counts, pagination flags, source bindings, root/child relationships,
labels, outcome and retry fields, concurrency values, and dimensions are
unchanged. Fields unrelated to collection or the finite bootstrap/concurrency
contract are removed, including agent details, permissions, timings and UI data.

`command` and `command_script` retain JSON null and the exact literal `true`.
Every other decoded command string is replaced with
`fixture-redacted-sha256:<SHA-256 of its UTF-8 bytes>`. Original matching
job/step commands were checked before transformation; equal commands receive
equal tokens. Commands are never run. This removes large imported scripts
without manufacturing command equality or rewriting null bootstrap commands.
JSON object key order and whitespace were normalized. Consequently, tests hash
these fixture bytes, **not** the original bodies listed above.

## Independently selected required mappings

`accepted-workflow-key-mapping.json` is a separate frozen controller input, not
a projection of successful provider rows. It contains all 41 finite job/matrix
instances across five workflows, including CI's 36 jobs. It was derived from:

- Accepted source `d0f23e01601db5b4ce206355723a10b430f58d72`
- `assets/tools/source_ci_profile.rs` inventory, without applicability filtering
- Exact matrix conversion in `src/decapod/core/readiness_profile.rs`
- Authored workflow jobs beyond the source-check catalog: CI `propodus-live`,
  `test-master`, `release-build`, and Docs `deploy`
- Reviewed buildkite-gha v0.102.7 native key derivation: workflow path SHA-256
  prefix plus job and deterministic matrix hash

The source inventory plan used for derivation has SHA-256
`7c147ad121e70d6d5d54f878ae2b7554c026e809c1437cbb2503f6e2158305ae`.
It is inventory input, not a successful validation result. The complete frozen
mapping has SHA-256
`4b35e8ad4eb96475db63c65e375445c7a751fa60c6396ee308c98f1b3c6b7217`.
The accepted CI workflow bytes have SHA-256
`95625705ed672de81ee3ea337f928dd945d96508549e40995ac4b3a0c3d538a8`.

Tests require the checked-in activation policy to match all frozen job keys and
all compiled source-check IDs across all five workflows, including inactive
checks and exact subcheck/matrix mappings. The separate candidate-build check
must map to CI setup. Bootstrap and concurrency identities are not inferred
from this mapping or captured success rows: they come from the explicit finite
Pipeline Trigger policy and independent repository/workflow/branch inputs.
