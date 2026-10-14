## Repository-local readiness

Consumer repositories may select `proof.required_profile.id = "repo.pr-readiness.v1"`.
This supported profile proves the repository's reviewed local build/test commands,
not execution of GitHub Actions, Buildkite, release packaging, deployment, or an
unavailable platform matrix. Decapod's `decapod.pr-readiness.v1` profile retains its
complete finite source-CI contract. A candidate cannot replace an accepted Decapod
strict profile with repository-local proof. Unknown profile IDs still fail closed.

The repository profile requires `validation_buildkite_preflights = false`, a
nonempty set of unique `proof.commands` with `required = true`, and an explicit
`[proof.repository_contract]`:

- `reviewed_commands_digest`: review pin over commands, coverage and execution declarations
- `source_coverage`: one `{ source, commands }` row for every discovered workflow,
  with a nonempty list of configured required command names; no duplicate,
  unknown, omitted, or excluded workflow mappings
- `executions`: a table keyed by each required command name, containing `program`
  (absolute canonical installed executable), `toolchain_files` (nonempty map of
  labels to absolute canonical tool/runtime files), `environment` (explicit map),
  `directory` (repository-relative, default `.`), and `timeout_seconds` (1–7200)

`proof.required_profile.reviewed_sources_digest` separately pins the full
workflow/action/helper/build inventory. `init` and `rpc --op agent.init` report
`inventory_digest` and `command_contract_digest` as read-only review aids. Inspect
all inputs and the intended coverage before explicitly entering either digest.
Neither initialization nor a matching digest executes proof or approves itself.
A changed workflow, command, arguments, requiredness, tool path, environment or
coverage mapping needs renewed review. Init reruns preserve both declarations.

`proof.commands[].command` names the declared executable (or uses its absolute
path); `args` remain separate literal arguments. Runtime resolves no YAML, shell
text, auto-installed tools, or ambient environment. The installed executable and
all declared toolchain files are byte-measured before and after execution. Their
transitive completeness and semantic coverage are repository-authored review
assertions, not facts inferred by Decapod. Use pinned installations and declare the
complete runtime closure. An explicit PATH must contain only declared executables;
prefer direct executable paths. The default HOME and temporary files use owned
scratch, with no caller credentials inherited. Direct shells, privilege/container
wrappers, recursive validators, daemon/network/external-write operations and
publication/deployment are unsupported. This adapter is not an OS sandbox.

Real Python, Node and Make executables may run reviewed repository code through
this local profile. Source is copied to a detached, immutable execution snapshot;
write generated output to the provided temporary scratch instead of the source
checkout. Any required command failure, timeout, output overflow, source mutation,
tool change, stale accepted target or incomplete evidence blocks readiness. Native
proof reports `scope = "repository_local_readiness"`; it makes no hosted-CI or
release assurance claim. A provider job that itself invokes `decapod validate`
still runs CI-local prerequisites, never recursively proves full publication.

For Nix projects, installing this profile is not evidence that daemon-backed
builds, remote builders or deployment ran. A supported local contract can use
preinstalled pinned formatters/test tools directly; unavailable build/deployment
proof remains a limitation to reconcile, not a successful or skipped check.

Repository proof uses a sealed, argument-aware authorization path minted only
for the validated local contract and exact measured invocation. It does not add
Python, Make or other programs to the ordinary `proof_exec` default allowlist.
An authored `.decapod/EXTERNAL_ACTIONS.json` remains authoritative: an explicit
`proof_exec` or `repository_proof_exec` deny blocks the command, malformed policy
fails closed, and a present policy must explicitly allow proof execution. The
kernel never rewrites that policy to make a check pass.
