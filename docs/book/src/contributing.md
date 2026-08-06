# Contributing to Decapod

Decapod is a governed agent control plane. Contributions are accepted when they increase enforcement value with minimal surface area.

## Non-Negotiable PR Rules

Every PR MUST include:
- **Intent**: What invariant or behavior is being changed.
- **Invariants affected**: An explicit list of impacted invariants.
- **Proof added**: A test, gate, or command that enforces the change.

## Local Dev & Tooling

Decapod uses Bazel (coordinated via Bazelisk) as its primary build and test system.

```bash
# Build the decapod binary
bazelisk build //:decapod

# Run all tests
bazelisk test //:core_tests

# Initialize and validate decapod locally
bazel run //:decapod -- init --proof
bazel run //:decapod -- validate
```

### Nix Development Shell

If you are using Nix, you can enter a fully reproducible development shell containing all the required tooling by running:

```bash
nix develop
```

You can also build Decapod using Nix:

```bash
nix build
```

---

## Before You Open a PR

Before submitting your PR, ensure that Decapod's governance gates and CI expectations are satisfied:
- Work in an isolated worktree via `decapod workspace ensure` after claiming a todo. Do not work directly on `master`.
- Run `decapod validate` and ensure it passes successfully.

### GitHub Workflow Permissions

> [!IMPORTANT]
> **GitHub Workflow Permissions (`workflow` scope)**
> 
> If you are contributing changes that trigger GitHub Actions workflows or edit files under `.github/workflows/`, your Personal Access Token (PAT) must have the `workflow` scope.
> 
> To verify if your token has the appropriate permission level, run the following API call:
> ```bash
> curl -I -H "Authorization: Bearer YOUR_GITHUB_TOKEN" https://api.github.com/user
> ```
> Inspect the **`X-OAuth-Scopes`** header in the response. It must contain the `workflow` scope (for example: `X-OAuth-Scopes: repo, workflow`).
