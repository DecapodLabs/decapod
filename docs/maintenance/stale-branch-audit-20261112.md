# Stale branch cleanup proof record — 2026-11-12

This records the cleanup's public evidence and remaining boundaries. The accepted
integration observed here is `c87e4f49cab8e2ccff5f9bdc5833d8602dfffdb8`
([#1635](https://github.com/DecapodLabs/decapod/pull/1635), merged at
2026-11-12T09:21:09Z). It is a dated audit, not a current-readiness receipt or
permission to execute a historical workflow.

## Merged billing recovery

[#1631](https://github.com/DecapodLabs/decapod/pull/1631) was merged by
`alexhraber` at 2026-11-12T01:22:37Z, after successful native publication.
Its proof has the following immutable binding:

| Role | Identity |
| --- | --- |
| Proven candidate | `b984a386fec57ff302f09013a143e63740639ced` |
| Historical accepted target | `c5de35a89b27e788169612ae71681e45c13bf71a` |
| Candidate tree | `cbb32bf666025718e35ba733ab9dbc02272b55d6` |
| Merge commit | `9b5f3de1d178e339a36ffd97804b382bd22f352b` |
| Native producer | `0261c2de10115957c20e1eef95d6bc94a626e199` |
| Native run / job | `43133190463` / `130902080880` |
| Artifact | `13906346897`, `billing-native-continuation` |

The [terminal native run](https://github.com/DecapodLabs/decapod/actions/runs/43133190463)
completed successfully. Its authenticated report records
`published_after_native_full_boundary`, full-boundary success, a clean source,
exported custody, native publisher exit zero, and verified remote/PR identity
without warnings. These statements describe that exact candidate and target.

The downloaded artifact is 2,323,894 bytes, SHA256
`6253526f77a83cc146154ab6207dad5591ed905ae52e5d05e006c8eb151f2cc9`.
The 30,839-byte public report has SHA256
`719cd8d4e57891e8cf52cbededddadc6b44d17b9b2cd7873a7af50291984893b`.

The merge has parents c5de35a8 and b984a386. It is an ancestor of d3358e1c,
which is a parent of c87e4f49. The replacement billing feature ref is absent.
Neither recreating that ref nor reopening #1631 is required.

## Six original redundant refs already retired

The [completed retirement run](https://github.com/DecapodLabs/decapod/actions/runs/43111223693)
succeeded at producer `3248e34c8372dc3849b5aab008c044a282735558`.
Artifact `13895634050`, `stale-six-retirement-result`, is 1,789 bytes,
SHA256 `5d67910ce4cc15925bd918f707a8788174466a40df82b625ce47573ed4c46195`.
Its authenticated report has SHA256
`4ef8b24c2d827c65cc04530f4e2f66d46ed321e4b9e0841cead6cb1ef6f0fdab`.

The report records deletion of these five original refs:

- `agent/chatgpt-ci-source-cache/cicd_01m74zx6jw1rx5jt-cache`
- `agent/codex-issue1610/code_01m750a4daxmnxmj-post-merge-proof`
- `agent/codex-issue1610/code_01m753k6cnsm40ma-post-merge-proof`
- `agent/codex-issue1610/code_01m7552xm7wp81wa-post-merge-proof`
- `ops/issue-1610-workspace-20261111`

The sixth, `ops/docs-rebase-v1381-20261111`, was already absent.
A later live ref inventory confirms all six remain absent.

Preserve the cache archive `archive/stale-20261111-cache-reconciled-49a33d35`
and the three `archive/stale-20261111-issue1610-{first-1385beba,second-36b976c4,third-1630ed15}`
tags at their report-bound original commits. The old operations histories were
preserved in a complete private Git bundle, not remote operations archive tags.
That earlier bundle is 44,108,402 bytes, SHA256
`67b025cff1a997731729cdde44da750b258580b42ce63e778c8d8d7d3f12404e`;
the report distinguishes operator persistence attestation from the runner's
independent identity/import checks.

## Remaining retirement

No remaining retirement is claimed complete. The two deletion candidates retain
these exact observed leases:

| Ref | Expected tip |
| --- | --- |
| `agent/codex-init-billing/code_01m7513af6jtzhp4-admin-checkout` | `26a499d9f0789a5ed00605d23cfc57f5392ae8fc` |
| `ops/stale-billing-publication-20261111` | `3f4a251e4a7b08eb011b9d376934fe1811f691a7` |

The billing archive tag `archive/stale-20261111-billing-26a499d9` points to
the exact 26a499d9 commit. A new complete private backup of operations parent
3f4a251e is 44,105,649 bytes, SHA256
`0f6b3c475c29dd9755af285e2a648495d8a0ff4b6df8cbcfbc7ae075474846c4`.
An independent empty-bare import verified 4,163 commits, tree
`a85eb5f4a8e75b908d621b024a71eeef6f73f2f0`, no prerequisites or alternates,
strict fsck exit zero, and successful producer 0261c2de as an ancestor.
This is a new attestation, not a rewrite of an older on-hold manifest.

Retirement still requires authenticated private recovery custody and matching
recovery key/certificate, exact source and operations recoverability, a reviewed
merged-state executable finalizer, fresh no-open-PR and competing-work checks,
and current leases. The obsolete publisher and open-PR finalizer must not run.
An eventual deletion must atomically name only the two intended refs, disable
follow-tags, and read back retained archives, drafts, accepted integration and
unrelated refs. A changed tip requires reassessment.

## Retained review surfaces

[#1612](https://github.com/DecapodLabs/decapod/pull/1612) retains the distinct
verified dependency-archive cache proposal at
`309791e87851364310966f6d754faa2e57494622`. It remains a conflicted draft.
The changed source-CI carrier has a recorded `CONTROLLER_CONTRACT_UNSUPPORTED`
blocker. An inventory declaration is not controller acceptance; current-target
reconciliation, supported importer/carrier execution, hosted cache population
and restore, and the real-Bazel fixture remain necessary.

[#1621](https://github.com/DecapodLabs/decapod/pull/1621) retains inactive
release-preparation research at
`3fad31096909dc7de91b615050ba1942bd24dbdf`. It remains a conflicted draft.
It does not complete #1399. Current-target reconciliation and exact proof are
still required; complete nonpublishing preparation, all four platform archives
and a bound execution/collection graph remain separate research obligations.

Neither draft is made ready by this cleanup audit.

## Billing findings after the accepted merge

The two findings were valid against d3358e1c. Accepted #1635 changed their
premises before the prepared follow-up was published:

- In `src/decapod/lib.rs`, `prepare_cloud_roster` and its init call were
  removed. The original repeat-roster-prompt path no longer exists.
- Managed cloud initialization now calls `bootstrap_cloud_session`, and both
  it and explicit session acquisition call the Dactyl connector in
  `src/decapod/core/cloud_client.rs`. Init is no longer purchase-only.
  Managed restart diagnostics name the supported
  `init --backend cloud --restart-cloud-signin` operation.
  Legacy `cloud_init_action` text remains in `core/propodus.rs`; this is not a
  claim that every legacy API diagnostic was removed or independently proved.

The prepared b984-follow-up candidate
`3638c7e86ef75239cb0f50058ddac6bbb9ab3102` had six focused regressions execute
successfully in [Buildkite 8258](https://buildkite.com/decapodlabs/decapod/builds/8258).
Its full native publication was interrupted after master advanced; it did not
produce a successful publication result. Its Rust/test delta is withdrawn from
this audit rather than restoring the removed purchase flow.

Every later readiness claim requires fresh native proof for its own exact
candidate and then-current accepted target. Private keys, native sessions,
plaintext custody, datastore backups and raw machine logs remain outside Git.
