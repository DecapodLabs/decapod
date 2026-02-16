# GIT.md - Git Etiquette and Workflow Contract

**Authority:** constitutional (BINDING)
**Layer:** Constitution (Guiding Principles)
**Binding:** Yes (for all agents and operators)
**Scope:** Git operations, branching strategy, commit conventions, push policies

This document defines the mandatory git workflow and etiquette that all agents and operators must follow when working in Decapod-managed repositories.

---

## 0. Purpose

Git is the canonical state layer for all project work. Poor git hygiene leads to:
- Lost work (destructive operations without recovery)
- Merge conflicts (uncoordinated changes)
- Broken history (force pushes to shared branches)
- Unclear attribution (malformed commits)
- Deployment failures (untagged releases)

**This contract prevents these failure modes.**

---

## 1. Branch Management

### 1.1. Branch Naming Convention

**Required format:** `<owner>/<purpose>`

Examples:
- `ahr/work` — General development work
- `ahr/feature-policy-engine` — Specific feature branch
- `claude/fix-validation-bug` — Agent-created bug fix
- `gemini/refactor-cli` — Agent-created refactoring

**Rationale:** Clear ownership and purpose. Prevents namespace collisions.

### 1.2. Protected Branches

**NEVER force-push to:**
- `master` (or `main`)
- `production`
- `stable`
- Any branch prefixed with `release/`

**Exception:** Only force-push to `master` when explicitly instructed by the operator.

**Violation:** Force-pushing to protected branches without authorization is a contract violation.

### 1.3. Working Branch Policy

**Default:** All agent work happens in designated working branches (e.g., `ahr/work`) unless explicitly instructed otherwise.

**Rationale:** Isolates experimental work from stable branches. Allows parallel exploration without conflicts.

**Enforcement:** Agents MUST check current branch before making commits. Use `git branch --show-current` to verify.

### 1.4. Branch Lifecycle

1. **Create:** Branch from `master` (or designated base branch)
2. **Work:** Make atomic commits with clear messages
3. **Sync:** Regularly pull/rebase from base branch
4. **Review:** Create PR when ready for integration
5. **Merge:** Merge via PR (never direct push to master)
6. **Cleanup:** Delete branch after merge (optional but recommended)

---

## 2. Commit Conventions

### 2.1. Commit Message Format

**Required:** [Conventional Commits](https://www.conventionalcommits.org/) format

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

**Allowed types:**
- `feat`: New feature
- `fix`: Bug fix
- `chore`: Maintenance (dependencies, cleanup)
- `docs`: Documentation only
- `style`: Formatting, whitespace (no code change)
- `refactor`: Code restructuring (no behavior change)
- `perf`: Performance improvement
- `test`: Adding or fixing tests
- `ci`: CI/CD pipeline changes

**Examples:**
```
feat(policy): add risk classification engine
fix(validate): handle missing .decapod directory
chore: bump dependency versions
docs(README): update installation instructions
refactor(cli): consolidate command groups
```

**Enforcement:** Use `decapod setup hook --pre-commit` to install validation hook.

### 2.2. Commit Atomicity

**Rule:** One logical change per commit.

**Good:**
```
feat(todo): add priority field to task schema
test(todo): add priority field tests
docs(TODO.md): document priority field usage
```

**Bad:**
```
feat: add priority field, fix validation bug, update README
```

**Rationale:** Atomic commits enable:
- Clean reverts (undo one change without affecting others)
- Clear history (understand what changed and why)
- Bisection (find bugs via git bisect)

### 2.3. Commit Co-Authorship

**User preference:** Do NOT add AI agents as co-authors unless explicitly requested.

**Rationale:** Some operators prefer attribution to remain human-only. Respect this preference.

**How to check:** Look for teammate preference entries like:
```bash
decapod data teammate get --pattern commit
```

---

## 3. Push Policies

### 3.1. Standard Push

**Safe operation:** `git push` or `git push -u origin <branch>`

**When to use:**
- Pushing new commits to your working branch
- Sharing work-in-progress
- Backing up local work to remote

**No authorization needed** for pushing to your own working branches.

### 3.2. Force Push

**Destructive operation:** `git push --force` or `git push --force-with-lease`

**NEVER force-push to:**
- `master` or `main`
- Any shared branch
- Any branch you don't own

**Only force-push to your own working branch when:**
- You've rebased and need to update the remote
- You've amended a commit that was already pushed
- You've cleaned up history before merging

**Prefer:** `git push --force-with-lease` (safer - checks remote hasn't changed)

**User authorization required** for force-pushing to `master`. Always ask first.

### 3.3. Push Verification

Before pushing, verify:

```bash
git status                    # Check working tree is clean
git log origin/master..HEAD   # See what you're about to push
git diff origin/master        # Review changes being pushed
```

---

## 4. Pull Request Requirements

### 4.1. When to Create a PR

Create a PR when:
- Work is complete and validated (`decapod validate` passes)
- Tests pass (if applicable)
- Documentation is updated
- Ready for human review

**Do NOT create PR for:**
- Work-in-progress (unless marked as draft)
- Broken/unvalidated changes
- Experimental branches (unless requesting feedback)

### 4.2. PR Description Format

```markdown
## Summary
<1-3 bullet points describing the change>

## Motivation
<Why this change is needed>

## Test Plan
<How to verify the change works>

## Checklist
- [ ] `decapod validate` passes
- [ ] Tests pass (if applicable)
- [ ] Documentation updated
- [ ] No force-push to master
```

### 4.3. PR Workflow

1. **Create:** `gh pr create --title "..." --body "..."`
2. **Review:** Wait for human approval
3. **Update:** Address feedback via new commits (don't force-push during review)
4. **Merge:** Operator merges when approved
5. **Cleanup:** Delete branch after merge

---

## 5. Merge Strategies

### 5.1. Allowed Merge Methods

**Prefer:** Merge commit (preserves full history)
```bash
git merge --no-ff feature-branch
```

**Alternative:** Rebase and merge (linear history)
```bash
git rebase master
git checkout master
git merge feature-branch
```

**Avoid:** Squash and merge (loses commit granularity) unless explicitly requested

### 5.2. Conflict Resolution

When conflicts occur:

1. **Understand:** Read both versions of conflicting changes
2. **Communicate:** Ask operator for guidance if unclear
3. **Resolve:** Manually edit files to resolve conflicts
4. **Test:** Verify merged code works (`decapod validate`)
5. **Commit:** Complete the merge with clear message

**NEVER:**
- Auto-resolve with `git checkout --ours` or `--theirs` without understanding
- Skip conflicts by deleting code
- Force-push to bypass conflicts

---

## 6. Tag and Release Conventions

### 6.1. Version Tags

**Format:** Semantic versioning `vMAJOR.MINOR.PATCH`

Examples:
- `v0.3.2` — Patch release
- `v1.0.0` — Major release
- `v1.2.0` — Minor release

**Create tag:**
```bash
git tag -a v0.3.2 -m "Release v0.3.2: CLI streamlining"
git push origin v0.3.2
```

**NEVER:**
- Delete tags without authorization
- Re-tag the same version (causes confusion)
- Push tags for unreleased code

### 6.2. Release Workflow

1. **Validate:** `decapod validate` passes
2. **Test:** `decapod qa verify` passes (if applicable)
3. **Version bump:** Update `Cargo.toml` version
4. **Commit:** `chore: bump version to vX.Y.Z`
5. **Tag:** Create annotated tag
6. **Push:** Push commit and tag together
7. **Build:** `cargo build --release`
8. **Publish:** `cargo publish` (if applicable)

---

## 7. Destructive Operations (Require Authorization)

The following operations are **destructive** and require **user authorization** before execution:

### 7.1. Force Push
```bash
git push --force
git push --force-with-lease
```
**When:** Only to your own working branch after rebase/amend
**NEVER:** To `master` or shared branches without explicit approval

### 7.2. Hard Reset
```bash
git reset --hard
git reset --hard origin/master
```
**When:** Discarding local changes you don't need
**Danger:** Loses uncommitted work - cannot be recovered

### 7.3. Branch Deletion
```bash
git branch -D <branch>
git push origin --delete <branch>
```
**When:** After PR is merged and branch is no longer needed
**Danger:** Loses unmerged work if branch wasn't backed up

### 7.4. Rebase Operations
```bash
git rebase -i HEAD~5
git rebase master
```
**When:** Cleaning up commit history before merge
**Danger:** Rewrites history - requires force-push

### 7.5. Cherry-Pick and Revert
```bash
git cherry-pick <commit>
git revert <commit>
```
**When:** Backporting fixes or undoing commits
**Caution:** Can cause conflicts and confusion

**Rule:** Always ask operator before performing destructive operations that affect:
- Shared branches
- Published commits
- Work that might be in use elsewhere

---

## 8. Git Hooks Integration

### 8.1. Available Hooks

Install via `decapod setup hook`:

**Commit-msg hook:**
- Validates conventional commit format
- Rejects malformed commit messages

**Pre-commit hook:**
- Runs `cargo fmt --all --check`
- Runs `cargo clippy --all-targets --all-features`
- Prevents committing unformatted or non-idiomatic code

### 8.2. Hook Enforcement

**NEVER bypass hooks** unless explicitly instructed:
```bash
git commit --no-verify   # DON'T DO THIS without authorization
```

**Rationale:** Hooks enforce code quality and conventions. Bypassing them introduces technical debt.

---

## 9. Safe Operations Checklist

Before any git operation, ask:

1. **Is this reversible?** (If no → ask operator first)
2. **Am I on the right branch?** (Check `git branch --show-current`)
3. **Is this a shared branch?** (If yes → be extra cautious)
4. **Have I validated my changes?** (Run `decapod validate`)
5. **Do I have a backup?** (Commit/push before destructive ops)

**When in doubt:** Ask the operator. The cost of asking is low; the cost of lost work is high.

---

## 10. Common Patterns

### 10.1. Starting Work

```bash
git checkout master
git pull origin master
git checkout -b ahr/work          # Or existing working branch
decapod todo list                  # See what to work on
```

### 10.2. During Work

```bash
# Make changes
git status                         # Check what changed
git add <files>                    # Stage specific files
git commit -m "feat: add feature"  # Commit with convention
git push -u origin ahr/work        # Push to remote
```

### 10.3. Preparing for PR

```bash
git checkout master
git pull origin master
git checkout ahr/work
git rebase master                  # Sync with master
# Resolve any conflicts
decapod validate                   # Ensure system is healthy
git push --force-with-lease        # Update remote after rebase
gh pr create                       # Create PR
```

### 10.4. After PR Merge

```bash
git checkout master
git pull origin master
git branch -d ahr/work             # Delete local branch (optional)
git push origin --delete ahr/work  # Delete remote branch (optional)
```

---

## 11. Troubleshooting

### 11.1. "Detached HEAD" State

**Problem:** `git checkout <commit-hash>` leaves you in detached HEAD

**Fix:**
```bash
git checkout master                # Return to branch
git checkout -b temp/recovery      # Or create branch if you made commits
```

### 11.2. Accidental Commit to Wrong Branch

**Fix:**
```bash
git log                            # Find commit hash
git checkout correct-branch
git cherry-pick <commit-hash>
git checkout wrong-branch
git reset --hard HEAD~1            # Remove from wrong branch
```

### 11.3. Lost Commits After Reset

**Recovery:**
```bash
git reflog                         # Find lost commit hash
git cherry-pick <commit-hash>      # Recover the commit
```

### 11.4. Merge Conflict Hell

**Abort and restart:**
```bash
git merge --abort                  # Cancel the merge
# Ask operator for guidance
```

---

## 12. Enforcement

This contract is enforced through:

1. **Git hooks** — Automated validation of commit format and code quality
2. **Agent contracts** — All agent templates mandate this document
3. **Code review** — Operators review PRs for compliance
4. **Validation gates** — `decapod validate` checks repository health

**Violations** of this contract (especially destructive operations without authorization) result in:
- Work rejection
- Branch restoration from backup
- Reduced agent autonomy (more oversight required)

---

## 13. See Also

- `specs/SYSTEM.md` — Authority and proof doctrine
- `specs/INTENT.md` — Methodology contract
- `core/DECAPOD.md` — Router (agent entry point)
- `plugins/AUTOUPDATE.md` — Session start protocol

---

**This contract is binding. Git operations MUST follow these rules.**

## Links

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` - System definition and authority doctrine
- `specs/SECURITY.md` - Security contract
- `specs/AMENDMENTS.md` - Change control

### Registry (Core Indices)
- `core/PLUGINS.md` - Subsystem registry
- `core/INTERFACES.md` - Interface contracts index

### Contracts (Interfaces Layer)
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/DOC_RULES.md` - Doc compilation rules

### Operations (Plugins Layer)
- `plugins/TODO.md` - Work tracking
- `plugins/VERIFY.md` - Validation subsystem
