# Constitution provenance and evidence

Design provenance, implemented mechanisms and measured effectiveness are separate
claims. This page records what can be checked and what remains testimony. It does
not claim that an instruction-file study tested Decapod or that earlier design
proves a better outcome.

## Reproduce the repository chronology

From a full checkout, run the source-controlled Rust generator against an immutable
accepted commit:

```sh
cargo run --locked --features build-tools --bin decapod-constitution-history -- \
  --ref 6ab6de3134286a790d78d370ca3fb43c0d92f834 \
  --public-evidence docs/research/constitution-public-source-evidence.json
```

`assets/tools/constitution_history.rs` inspects history reachable from that
commit and explicitly captured public version-tag objects. It records source
predicates, full commit/blob identities, author and committer dates, source URLs
and the first containing published version. It
separately dates embedded Markdown, generated entrypoint templates, overrides,
section hashes, scoped selection, global JSON, compressed JSON embedding,
capsules, override provenance, workspace refusal and proof-gate implementation.
Missing evidence stays null. Its deterministic fixtures exclude an unaccepted
orphan branch and distinguish a version tag from a published release.

The checked-in [generated facts](https://github.com/DecapodLabs/decapod/blob/master/docs/research/constitution-public-history.json)
bind the [public source capture](https://github.com/DecapodLabs/decapod/blob/master/docs/research/constitution-public-source-evidence.json).
The capture records 594 public plain version tags and 592 nondraft, nonprerelease
GitHub releases observed on 2026-11-17. It was collected read-only with
`git ls-remote --tags origin` and `gh api --paginate repos/DecapodLabs/decapod/releases`.
The generator verifies local tag-object equality and hashes the explicit input;
it does not authenticate the capture's producer. Without this input, publication
remains unobserved. Older public tag lineages are included explicitly rather than
mistaking the earliest tag still ancestral to today's master for the first release.

The first matching commit in reverse topological source history is an observed
repository fact. Git author/committer/tag timestamps can be changed; they are not
independently recorded dates of first public availability. A containing tag is
not evidence of an uploaded GitHub release or of a functioning released binary.
Release publication uses the separate captured forge evidence. Literal source predicates
identify inspected bytes; tests and actual-binary execution establish behavior.

## Claim and evidence matrix

| Claim | Time asserted | Artifact | Commit/release | Class | Verification and limits | Provenance |
| --- | --- | --- | --- | --- | --- | --- |
| Intent-Driven Design statements began in August 2025 | Reported in the public research artifact; inspected 2026-11-17 | Author's account of LinkedIn posts | No independently verified original post supplied here | Public author testimony about earlier posts | Original post URL, publication date and contemporaneous archive still needed; no inception date inferred | [Research README](https://github.com/DecapodLabs/accountable-agentic-execution/blob/main/README.md) |
| Constitution-driven scaffolding existed around September 2025 | Same account | Reported earlier practice | No contemporaneous artifact verified here | Author testimony | A separate milestone from the November report; neither date silently replaces the other | [Research README](https://github.com/DecapodLabs/accountable-agentic-execution/blob/main/README.md) |
| Recurring context poisoning motivated a November 2025 design milestone | 2026-11-17 issue clarification | Author's report | No private artifact inspected or published | Author testimony | Private evidence remains private; permission and provenance are required before use | [#879](https://github.com/DecapodLabs/decapod/issues/879) |
| embedded markdown constitution | Commit metadata 2026-02-11T03:01:48-08:00; release 2026-02-11T12:51:11Z | `src/core/assets.rs`; literal predicates and blob in generated facts | `0b4378d01fd5cb7033856938db6a0b24cbbb26b9`; v0.0.1 | Public repository source + captured forge publication | Earliest observed matching source predicate; not design inception, execution or efficacy | [Source](https://github.com/DecapodLabs/decapod/blob/0b4378d01fd5cb7033856938db6a0b24cbbb26b9/src/core/assets.rs) |
| generated agent entrypoint template | Commit metadata 2026-02-11T03:01:48-08:00; release 2026-02-11T12:51:11Z | `src/core/assets.rs`; literal predicates and blob in generated facts | `0b4378d01fd5cb7033856938db6a0b24cbbb26b9`; v0.0.1 | Public repository source + captured forge publication | Earliest observed matching source predicate; not design inception, execution or efficacy | [Source](https://github.com/DecapodLabs/decapod/blob/0b4378d01fd5cb7033856938db6a0b24cbbb26b9/src/core/assets.rs) |
| repository override resolution | Commit metadata 2026-02-12T15:07:21-08:00; release 2026-02-12T23:20:02Z | `src/core/assets.rs`; literal predicates and blob in generated facts | `c164749cc5a3007dcae7afafcb04df9b71f4e314`; v0.1.4 | Public repository source + captured forge publication | Earliest observed matching source predicate; not design inception, execution or efficacy | [Source](https://github.com/DecapodLabs/decapod/blob/c164749cc5a3007dcae7afafcb04df9b71f4e314/src/core/assets.rs) |
| section fragment with source hash | Commit metadata 2026-02-17T15:41:09-08:00; release 2026-02-17T23:47:47Z | `src/core/docs.rs`; literal predicates and blob in generated facts | `b50afbb654bc5dcb854523a943a78cd075ae115b`; v0.26.0 | Public repository source + captured forge publication | Earliest observed matching source predicate; not design inception, execution or efficacy | [Source](https://github.com/DecapodLabs/decapod/blob/b50afbb654bc5dcb854523a943a78cd075ae115b/src/core/docs.rs) |
| task query scoped selection | Commit metadata 2026-02-23T04:28:38-08:00; release 2026-02-23T12:47:14Z | `src/core/docs.rs`; literal predicates and blob in generated facts | `329f5c234e73e6cc270a60e53bccb0c60b1389d6`; v0.38.0 | Public repository source + captured forge publication | Earliest observed matching source predicate; not design inception, execution or efficacy | [Source](https://github.com/DecapodLabs/decapod/blob/329f5c234e73e6cc270a60e53bccb0c60b1389d6/src/core/docs.rs) |
| global json node corpus | Commit metadata 2026-05-21T21:55:18-07:00; release 2026-05-22T04:59:06Z | `assets/constitution.json`; literal predicates and blob in generated facts | `0e87b0ebf9c3252482f2a37e1bb5b4188b044e12`; v0.49.0 | Public repository source + captured forge publication | Earliest observed matching source predicate; not design inception, execution or efficacy | [Source](https://github.com/DecapodLabs/decapod/blob/0e87b0ebf9c3252482f2a37e1bb5b4188b044e12/assets/constitution.json) |
| compressed json embedding | Commit metadata 2026-05-27T17:33:29-07:00; release 2026-05-28T00:36:19Z | `build/compress_constitution.rs`; literal predicates and blob in generated facts | `94f17b3a4a762ba0c7ad3421360ba0ac5f1b12bb`; v0.55.0 | Public repository source + captured forge publication | Earliest observed matching source predicate; not design inception, execution or efficacy | [Source](https://github.com/DecapodLabs/decapod/blob/94f17b3a4a762ba0c7ad3421360ba0ac5f1b12bb/build/compress_constitution.rs) |
| state bound context capsule | Commit metadata 2026-02-23T06:51:52-08:00; release 2026-02-23T15:18:52Z | `src/core/context_capsule.rs`; literal predicates and blob in generated facts | `b4ea35b07ebf4990ef47184d6c13a22483ab37df`; v0.38.5 | Public repository source + captured forge publication | Earliest observed matching source predicate; not design inception, execution or efficacy | [Source](https://github.com/DecapodLabs/decapod/blob/b4ea35b07ebf4990ef47184d6c13a22483ab37df/src/core/context_capsule.rs) |
| override source provenance | Commit metadata 2026-08-01T19:37:05-07:00; release 2026-08-02T02:46:17Z | `src/decapod/core/assets.rs`; literal predicates and blob in generated facts | `3590e46c46460b9e2fd42a8a1e447f017273e233`; v0.93.0 | Public repository source + captured forge publication | Earliest observed matching source predicate; not design inception, execution or efficacy | [Source](https://github.com/DecapodLabs/decapod/blob/3590e46c46460b9e2fd42a8a1e447f017273e233/src/decapod/core/assets.rs) |
| native workspace refusal | Commit metadata 2026-02-17T12:45:08-08:00; release 2026-02-17T20:51:28Z | `src/core/workspace.rs`; literal predicates and blob in generated facts | `9f071132dd996fe713159a27dca012b0b325cb26`; v0.25.1 | Public repository source + captured forge publication | Earliest observed matching source predicate; not design inception, execution or efficacy | [Source](https://github.com/DecapodLabs/decapod/blob/9f071132dd996fe713159a27dca012b0b325cb26/src/core/workspace.rs) |
| native proof gate | Commit metadata 2026-02-23T06:40:40-08:00; release 2026-02-23T14:57:40Z | `src/core/workunit.rs`; literal predicates and blob in generated facts | `0537bf48b9eac9c71a4fe5558f9cf2f43c4b602d`; v0.38.4 | Public repository source + captured forge publication | Earliest observed matching source predicate; not design inception, execution or efficacy | [Source](https://github.com/DecapodLabs/decapod/blob/0537bf48b9eac9c71a4fe5558f9cf2f43c4b602d/src/core/workunit.rs) |
| The AGENTS.md study first appeared in February, with later revisions | Publication history verified 2026-11-17 | arXiv history | v1: 2026-02-12 14:15:22 UTC; v2: 2026-06-23; v3: 2026-09-29 | Independent publication record | Studies repository instruction files; does not evaluate Decapod | [arXiv:2602.11988](https://arxiv.org/abs/2602.11988) |
| Accountable Agentic Execution's controlled studies remain prospective | README inspected 2026-11-17 | Explicit pre-results status and synthetic-fixture warning | README blob `168f0fa6e08e1cf65dd635fd77fabf33a918f99b` | Public research status | Mechanism implementation and synthetic records are not treatment-effect observations | [Research README](https://github.com/DecapodLabs/accountable-agentic-execution/blob/main/README.md) |

The generated facts supplement this matrix with separately identified later
mechanisms. A claim of independent development can be made only to the extent
its dated artifacts support it; this history alone does not establish November
2025 or general design priority. No private artifact is inferred from testimony.

## Security and empirical limits

The global Constitution is a binary-embedded source corpus. Repository overrides
specialize registered directives with explicit precedence; task context is a
bounded projection. Advisory research and contributed software-business guidance
do not acquire binding authority through repetition or retrieval rank. Source
admission checks structural promotion; required PR review remains a separate
authority boundary, and guidance is not automatically a machine-enforced gate.

Context poisoning includes rejected conversational premises resurfacing and
lower-trust text pretending to be accepted policy. Repository-file redundancy is
a different question studied by the AGENTS.md evaluation. Neither phenomenon
alone proves a Decapod advantage or a particular transformer cache mechanism.
Decapod cannot erase tokens already delivered to a third-party model, force an
uncooperative host to invoke it, or intercept arbitrary unrestricted shell/forge
actions. Source review and hashes cannot establish relevance, host consumption,
semantic truth or successful execution on their own.

Implementation and empirical acceptance remain with [#1772](https://github.com/DecapodLabs/decapod/issues/1772)
(selection), [#1774](https://github.com/DecapodLabs/decapod/issues/1774)
(contribution admission), [#657](https://github.com/DecapodLabs/decapod/issues/657)
(knowledge corrections), [#880](https://github.com/DecapodLabs/decapod/issues/880)
(observed host delivery) and [#1095](https://github.com/DecapodLabs/decapod/issues/1095)
(independent outcomes and total cost). Historical provenance never substitutes
for their tests or for controlled real-agent comparisons.
