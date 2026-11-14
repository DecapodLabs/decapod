# Independent public proof collection

The public reader and an authenticated diagnostic client have different authority
and quota. A successful `gh` request is not evidence that Decapod's independent
reader can read an artifact, and GitHub Actions success cannot discharge required
Buildkite obligations. `snapshot_provider::PublicGithubReads` owns the anonymous
GitHub route; `public_buildkite` owns the configured execution collection.

`github_quota::GithubQuota` observes only successful response headers. It accepts
one bounded decimal remaining/reset value; duplicate, malformed, absent or
overflowing hints remain unknown. Discovery polls reserve half the observed
allowance for collection and fresh identity checks. Direct Buildkite polling
without new GitHub discovery requests retains its ordinary interval. The shared
`Budget` carries quota hints into custody/archive sub-budgets without replacing
the original deadline or cancellation token. No response body, token, cookie or
credential is persisted as quota state.

A denied request returns its actual failure immediately. The pacing mechanism
does not retry a403, switch identities, infer provider authority from a header or
reuse stale green records. An exhausted successful response waits only within
the remaining invocation budget and requires an unambiguous reset. A bounded
wait that cannot complete remains a blocker. Unknown completion and missing
workflow production remain unproven.

The executable quota regressions cover discovery headroom, undelayed direct
Buildkite polls, exhausted/unknown resets, duplicate/malformed/overflowing hints,
and original deadline/cancellation preservation. Public-reader adapter tests
retain only typed status/numeric retry hints on failure. These mechanism tests
do not independently prove a real provider execution.

A real complete observation followed the merged repair: official Decapod0.144.3
validated candidate `b586dac3ec7d7048f296c214818c5f735138c89a` against target
`422e0b56879abc1439032528a6d3e4514932e807`, returning `status=pass`,
`scope=source_pr_readiness`, `proof_ready=true`, `publication_ready=true`, and no
blockers. [Buildkite9477](https://buildkite.com/decapodlabs/decapod/builds/9477)
supplied the source suite, with separate constitution9478, validation9479,
docs9480 and docs-sync9481 execution. The native evidence reference is
`decapod-readiness-gtf7FO`. The first attempt remained blocked on missing release
tags; fetching the missing official tags repaired that distinct history issue.
No alternate reader identity or provider-proof substitution was used, and local
project checks were `not_run_provider_owned`. Delivery merged as1675.

This demonstrates a complete supported collection, not immunity from future
quota exhaustion or unavailable providers. The original failures remain in1573.
Current work must still collect complete exact-candidate proof and recheck the
accepted target; cached history is never new publication authority. The SDK,
cloud authorization and workunit proof issues retain their separate obligations.
