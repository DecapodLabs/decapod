# Public review-source capture

## Current role after the 2026-10-20 acceptance revision

These captures, semantic/admission ledgers and failed studies are preserved
research history. The maintainer replaced the implementation-added statistical
publication prerequisite with functional integration acceptance. Zero admitted
research units no longer blocks that approved delivery by itself. It still means
domain-level readiness reliability is unestablished; the old criterion is not
passed and agent-authored labels are not independent human judgments.

The current native recipe uses fixed production-path functional cases from
`../vega-functional-regressions.json`, actual candidate contract checks, real
Metal/CPU model decisions and a required aggregate with unchanged numerical
comparisons. Positive acceptance, obligation rejection and non-ready uncertainty
are required; collection alone never grants publication. Historical descriptions
of admission as a required gate below record the earlier contract only.

## Exposed-study and admission ledgers

`exposed-v2-acceptance-audit.json` preserves all 29 accepted IDs from the
completed 9f017d31 experiment, both backends' unmodified typed outcomes, token
counts and exact serialized-input commitments. Reconstruction used the Rust
`readiness_evaluation::state` field order and unchanged frozen corpus; every
SHA256 matched the recorded state digest. Sorting JSON keys changes those bytes
and is not an equivalent hash reconstruction. Input-hash verification and
classification are forensic analysis, not new model execution, an independent
label or a controller acceptance receipt. It contains no private input text or
task-transfer material. The full-precision outcomes remain failed diagnostic
evidence; **all exposed v2 units are ineligible as an untouched final holdout**.

`final-admission-review.json` reuses the 24 already inspected public semantic
cases and their original immutable source bindings/limitations. Narrow static
witnesses, changed requirements, future-check cutoffs and unresolved semantics
remain distinct. No final readiness case is admitted or frozen by this review;
approval and agent-authored rationale do not supply independent complete labels.
The nine earlier unexecuted source-fact pilot scenarios also differ from the
production projection and cannot substitute for a final study.

The native recipe now binds the admission ledger and keeps required final
qualification non-ready while it admits zero complete production units. The
exposed-outcome ledger remains forensic history. New native protocol diagnostics
use the exact shared production projection, response interpretation and final
decision predicate; their constructed observations do not supply independent
labels or authorize publication. Risk/coverage conditions remain unchanged.
The crate's existing exclusion covers these research artifacts.
See [the product decision](../../../docs/vega-readiness-decision.md) for contract
findings and the original working-integration scope; the maintainer rejected the
inactive-stage proposal. No calibration or model was changed.

## Original capture

`source-records.json` is a frozen public input dataset, not a validation log,
controller receipt, calibration result or published model asset. Its SHA256 is
`50ce9421f775db9145c8bef8540a32e0b2f8581222627dd426de460d25cf33c2`.
It is excluded from the published Decapod crate. No authentication headers,
session credentials, machine paths or private repository records are included.

The capture contains 356 snapshots from 274 public repositories: 263 structured
approval candidates and 93 change-request candidates. Candidates came from the
first six pages of GitHub repository search, ordered by stars, for
`language:Rust stars:>500 archived:false fork:false`, excluding the seven
repositories in the prior benchmark corpus. The most recently updated 20
closed/merged PRs in each of 593 unique repositories were inspected. For each
repository and outcome, the first indexed eligible candidate was retained:
an external source OWNER/MEMBER/COLLABORATOR `User` review naming the snapshot
commit and a currently successful CI rollup. These are *discovery filters*;
they do not establish that CI preceded the review, historical branch policy,
complete semantic labels, independent samples or calibration.

The records were retrieved through GitHub's public GraphQL API on 2026-10-19.
Each includes repository identity and observed SPDX metadata, exact commit,
PR number, selected review ID/URL/time, requirements title/body, title-edit
events, review identities and the commit's check contexts with conclusions,
completion times and observed requiredness. Review bodies and inline comments
are deliberately omitted; selected verdicts and later review reasoning must
never enter model input. Public review URLs identify the original rationale
for subsequent case-by-case semantic audit. A repository's observed SPDX ID
describes the source project; it does not establish separate dataset authorship
or independent adjudication of this adaptation.

Source verification uses `repository(owner:,name:)`, `object(oid:)`,
`pullRequest(number:)`, `reviews(first:100)`, title-renaming timeline events and
`statusCheckRollup.contexts(first:100)`. All returned pagination/completeness
fields are retained. `CheckRun.isRequired(pullRequestNumber:)` and the equivalent
status-context field describe policy at retrieval, not authenticated policy at
the historical decision. Re-query using `gh api graphql --input <query.json>`
and compare the pinned snapshot and selected review/commit identities. Later
edits, dismissed reviews, re-run checks or policy changes can make live API
responses differ; they must not silently rewrite this frozen capture.

`public_review_sources.rs` audits identity, external reviewer binding, complete
review/check history, requirements edited after the decision, conflicting
same-commit verdicts and CI timestamps. Future or unfinished checks cannot be
backfilled into the pre-decision input. Repositories sharing a source owner use
the same prospective split, chosen by the first SHA256 byte of
`vega-public-review-split-v1:<lowercase-owner>` (below 128 is development).
Owners used by the previous corpus are excluded from prospective inference.
Owner grouping reduces direct leakage; it does not establish statistical
independence across projects, maintainers or organizations.

The existing numerical collection attempts this source audit independently and
exports its private report. `--audit-public-review-sources --proof-output
<new-private-file>` exposes the same data preparation only. Run it through a
reviewed Decapod native-worker recipe, with `decapod validate` as the entrypoint.
Audit errors do not suppress independent native model diagnostics. The existing
strict numerical and heldout acceptance conditions are unchanged.

No candidate inference, calibration fitting or new heldout evaluation has used
these records. Passing structural filters means only that a source record can
be audited further. Its source verdict remains an external review proxy, not
human-reviewed ground truth for all obligations in the bounded input. Semantic
requirements, evidence relevance, ambiguity, temporal completeness and related
case groups must be adjudicated before freezing a new evaluation corpus. The
audit explicitly reports `production_calibration = "unsatisfied"` and
`holdout_frozen_for_inference = false`. The prior failed corpus and its frozen
zero-false-ready, one-sided 95%/5% risk and 50% coverage criteria remain intact.
