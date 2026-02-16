# Security Boundary Model

## Trust zones
- Zone A: local repo content (source + docs)
- Zone B: `.decapod` state (tasks, federation, proofs, archives)
- Zone C: external systems (Git remotes, container registries, network)

## Identity and auth boundaries
- Agent identity is command-scoped and auditable via control plane events
- GitHub access requires explicit SSH key availability and repository authorization
- Docker daemon access is privileged and must remain explicit

## Secret handling
- No secrets in repo files or event logs
- Secrets only via environment or host secret stores
- Redact secret-like fields from logs and exported artifacts

## Required controls
- Enforce least-privilege execution for container and external actions
- Keep approval gates for high-risk irreversible operations
- Require provenance sources for knowledge graph claims
