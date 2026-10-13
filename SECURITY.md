# Security

Report vulnerabilities privately, and keep credentials out of source code and
reports.

Decapod checks work performed through its governed workflow. Agents and tools
still run with the permissions granted by their host environment. See the
[security specification](.decapod/managed/specs/SECURITY.md) for the implemented
controls and their limits.

## For Agents

Agents follow the [repository contract](AGENTS.md) and the security guidance in
the [embedded constitution](assets/constitution.json), including
`architecture/SECURITY` and `architecture/SECRETS`.

Before changing authentication, permissions, or secret handling, identify the
affected trust boundary and how the change will be checked. Bring unresolved
security decisions to the human responsible for the work.

## Reporting Security Issues

Contact the maintainers directly through a private channel. Do not publish
vulnerability details in a GitHub issue or pull request.

Include:

- The affected version or commit
- Steps to reproduce the problem and its likely impact
- Relevant logs or a minimal example, with credentials and private data removed

## Security Principles Summary

- Validate untrusted input at the boundary where it enters the system
- Use the minimum permissions needed for the task
- Check authorization for each protected operation
- Keep independent safeguards where a single failure could expose data or access
- Leave work blocked when a required security check cannot be completed

## Credential Handling

- Never commit credentials or include them in logs, screenshots, issues, or pull requests
- Use supported environment-based configuration or secret storage; keep secrets out of tracked configuration files
- Follow the credential owner's expiry and rotation policy
- Revoke temporary access when it is no longer needed, and rotate or revoke exposed credentials promptly

If a credential is exposed, notify its owner and address the exposure before
continuing the affected work.
