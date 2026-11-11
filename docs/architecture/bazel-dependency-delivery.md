# Bazel archive delivery

This cache proposal originated at `15b69ce43fc56440d498d52d8726f89d499615fe`
and is reconciled against accepted v0.137.2 source
`53a55742dfbd100d2f0fd94a91d9b5bdd6c484ee` for PR #1612. It changes no dependency
version, lockfile, test graph, source executor, or custody verdict. The original
HTTP 504s prevented compilation and cascaded into missing bootstrap artifacts.

The observed failing check on historical head
`309791e87851364310966f6d754faa2e57494622` is Buildkite build 7355's
`governance-artifacts`: `PR_MERGE_CONFLICT` against current master. The cache
restore step completed with an ordinary miss before that failure. This evidence
does not identify a cache-runtime defect or prove hosted cache population and
reuse. Reconciliation must preserve the existing governance journal through
supported custody and checkpoint commands; source copying alone is not that
proof.

## Cache boundary

The existing source-CI preparation action restores only Linux Bazel 9.3.0's
`~/.cache/bazel/*/cache/repos/v1/content_addressable/sha256` subtree. Bazel's
`DownloadCache.findCacheValue` hashes each file against its content-addressed key
before use. The restored bytes do not constitute a test result or toolchain
receipt. Extracted repositories (`contents`), output trees, action caches,
executables and proof artifacts are outside the saved paths. Bazel still applies
the requested checksum and canonical download identity.

The stock `actions/cache` v4.3.0 implementation is pinned to
`0057852bfaa89a56745cba8c7296529d2fc39830`. Non-master events use its restore-only
entrypoint. Only a `push` to `refs/heads/master` invokes the root action with its
successful-job post-save. Provider repository/ref authorization remains required;
workflow conditions are not a substitute for server-side cache authorization.
Cache keys separate OS, architecture and the hash of `.bazelversion` plus
`MODULE.bazel.lock`. Unique run/job/attempt suffixes allow later successful jobs
to publish more complete entries without overwriting immutable cache objects.
There is no broad cross-lockfile restore prefix or cross-provider claim.

A miss uses the unchanged upstream URLs and integrity checks. Cache availability
is an optimization, never grounds to skip a required build. This does **not**
solve a cold start during a complete upstream outage. An accepted-master run must
first populate a usable provider-scoped cache when delivery is available.
GitHub and Buildkite caches need separate authentic producer observations; a
successful local fixture does not establish either provider's persistence.

## Acceptance sequence

1. Review this carrier change independently. Released controller v0.137.2 still
   embeds the accepted `.github/actions/source-ci-profile/action.yml` in `carrier_files()`.
   `accepted_executor_never_authorizes_changed_candidate_workflows_or_carriers`
   explicitly rejects changed carrier bytes with
   `CONTROLLER_CONTRACT_UNSUPPORTED`, even with a matching reviewed source digest.
2. Independently accept and release a controller containing the reviewed carrier.
   Updating a candidate's digest or using a candidate-built controller cannot
   grant this authority. The cache change cannot certify its own publication.
3. Reconcile the complete reviewed source inventory through the documented
   maintainer transition. This branch deliberately does not repin the inventory,
   accepted executor or configured requirements.
4. Exercise the cache on the accepted baseline with authentic native GitHub and
   Buildkite jobs. Observe population, restore, required builds/tests and custody
   evidence, including cache-miss behavior. The deployed importer must admit the
   pinned cache client; current documentation alone is not deployment proof.
5. Keep existing required checks and their original failure evidence. Merge
   conflict repair, carrier acceptance, and authentic hosted cache reuse are
   separate obligations; satisfying one does not certify the others.

## Regression evidence and limits

`python3 tests/bazel_repository_cache_test.py` uses the repository's exact Bazel
pin, isolated output roots and a loopback-only fixture origin. Bazelisk and that
Bazel version must already be available or may need an external bootstrap
download; the test does not claim an entirely offline cold toolchain. It warms an archive,
then changes the origin to HTTP 504 and requires reuse without another request.
It corrupts the cached bytes and requires failure rather than executing them.
A separate assertion checks read-only versus accepted-master cache entrypoints.
This checks Bazel archive semantics, not complete accepted-baseline execution,
hosted cache service authorization, archive extraction security, or publication.
Actors with arbitrary access to the runner account/provider administration remain
outside these workflow-level protections. Cache archives must come from the
existing trusted provider service; no alternate mirror or identity is introduced.

References:

- https://bazel.build/reference/command-line-reference
- https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/bazel/repository/cache/DownloadCache.java
- https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/bazel/repository/cache/RepositoryCache.java
- https://github.com/actions/cache/tree/0057852bfaa89a56745cba8c7296529d2fc39830
- https://buildkite.com/docs/pipelines/migration/run-github-actions-workflows/compatibility#cache-action

Historical local execution results at the original source head (not rerun or
reissued as results for this reconciliation):

- Real pinned-Bazel cache fixture: 2 tests passed, including the carrier boundary
  assertion. The fixture required host access because macOS sandbox restrictions
  prevent Java process inspection; no runner security or repository check changed.
- `cargo fmt --all -- --check`, `actionlint -shellcheck= .github/workflows/ci.yml`,
  Python compilation and `git diff --check`: passed.
- `cargo test --locked --test source_ci_profile_policy -- --test-threads=1`:
  52 passed, 3 failed. A clean Git snapshot of accepted `15b69ce4` reproduced all
  three failures with the same diagnostics. They invoke Linux-only runner checks
  on macOS; no affected test or executor code changes in this branch. These remain
  failures, not passing portable coverage.
- Hosted cache restore/save, full accepted-baseline CI and exact-candidate
  publication readiness: unavailable pending independent carrier acceptance and
  authentic provider execution. These historical results remain limited to their
  original source and environment.

Current failing-check evidence:

- https://github.com/DecapodLabs/decapod/pull/1612
- https://buildkite.com/decapodlabs/decapod/builds/7355#01a1c9d9-ef68-41be-8ec2-bffa8740ff7a

The current reconciliation has not been executed through the configured
validation. No controller-contract digest, accepted executor, required check,
or proof artifact is changed to turn the unavailable readiness claim into a pass.
