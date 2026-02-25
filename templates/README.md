# .decapod - Decapod Project Metadata 🦀✨

Welcome to the control-plane directory for this repo.

## Quick Start

1. **Initialize**: Run `decapod init` to set up your project
2. **Configure overrides**: Edit `.decapod/OVERRIDE.md` to customize behavior
3. **Read docs**: Use `decapod docs show <path>` to read constitution docs

## Summary

The `.decapod/OVERRIDES.md` file is your project-local override layer for Decapod's embedded constitution.

The embedded constitution (shipped with Decapod) is read-only baseline policy.
`.decapod/OVERRIDE.md` is where you add project-specific behavior without forking Decapod.

Keep overrides in the correct section, minimal and explicit.

## How to Use Overrides

The embedded constitution (read-only, shipped with Decapod) provides the base methodology. The `.decapod/OVERRIDE.md` file lets you customize behavior without forking Decapod.

**To add an override:**

1. Find the component section in `OVERRIDE.md` (Core, Specs, Interfaces, Methodology, Architecture, or Plugins)
2. Scroll to the specific component you want to override (e.g., `### plugins/TODO.md`)
3. Write your override content under that heading
4. Use markdown formatting for your overrides
5. Commit this file to version control

**Example override:**

```markdown
### plugins/TODO.md

## Priority Levels (Project Override)

For this project, we use a 5-level priority system:
- **critical**: Production down, blocking release
- **high**: Sprint commitment, must complete this iteration
- **medium**: Backlog, next sprint candidate
- **low**: Nice-to-have, future consideration
- **idea**: Exploration, needs refinement before actionable
```

## Available Override Sections

- **Core**: DECAPOD.md, INTERFACES.md, METHODOLOGY.md, PLUGINS.md, GAPS.md, DEMANDS.md, DEPRECATION.md
- **Specs**: INTENT.md, SYSTEM.md, AMENDMENTS.md, SECURITY.md, GIT.md
- **Interfaces**: CLAIMS.md, CONTROL_PLANE.md, DOC_RULES.md, GLOSSARY.md, STORE_MODEL.md
- **Methodology**: ARCHITECTURE.md, SOUL.md, KNOWLEDGE.md, MEMORY.md
- **Architecture**: DATA.md, CACHING.md, MEMORY.md, WEB.md, CLOUD.md, FRONTEND.md, ALGORITHMS.md, SECURITY.md, OBSERVABILITY.md, CONCURRENCY.md
- **Plugins**: TODO.md, MANIFEST.md, EMERGENCY_PROTOCOL.md, DB_BROKER.md, CRON.md, REFLEX.md, HEALTH.md, POLICY.md, WATCHER.md, KNOWLEDGE.md, ARCHIVE.md, FEDERATION.md, FEEDBACK.md, TRUST.md, CONTEXT.md, HEARTBEAT.md, APTITUDE.md, VERIFY.md, DECIDE.md, AUTOUPDATE.md

## Canonical Layout

- `OVERRIDE.md`: Project-local override layer for embedded constitution.
- `data/`: Canonical control-plane state (SQLite + ledgers). Access through Decapod commands.
- `generated/specs/`: Living project specs scaffolded by `decapod init`.
- `generated/context/`: Deterministic context capsule artifacts.
- `generated/artifacts/provenance/`: Promotion manifests + convergence checklist.
- `generated/artifacts/inventory/`: Release inventory artifacts.
- `generated/artifacts/diagnostics/`: Opt-in diagnostics artifacts.
- `workspaces/`: Isolated todo-scoped git worktrees for implementation.

## Policy

- Treat `.decapod/` as the single home for Decapod-managed project state.
- Keep top-level project directories focused on product/source code, not control-plane artifacts.
- Access `.decapod` files via Decapod commands unless explicitly documented otherwise.
