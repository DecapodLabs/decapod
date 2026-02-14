# Security

Decapod takes security seriously. This document provides an overview of our security practices and guidance for agents operating within the Decapod environment.

## For Agents

**Read the constitutional security contract:** [`constitution/specs/SECURITY.md`](constitution/specs/SECURITY.md)

This document is binding. All agents must follow the security principles outlined therein, including:
- Credential handling (never log, never commit, always rotate)
- Git security (signed commits, verified remotes)
- CI/CD security (OIDC, short-lived tokens)
- Supply chain integrity (dependency audit, reproducible builds)

## Reporting Security Issues

For security vulnerabilities in Decapod itself:
- **Do not** open a public GitHub issue
- Contact the maintainers directly
- Provide detailed reproduction steps

## Security Principles Summary

| Principle | Description |
|-----------|-------------|
| **Zero Trust** | Never trust, always verify |
| **Defense in Depth** | Layered controls, assume breach |
| **Least Privilege** | Minimum access required |
| **Fail Secure** | Default deny, error toward safety |
| **Complete Mediation** | Every access checked |

## Credential Handling

When handling credentials as an agent:
1. **Never** log credentials to any output
2. **Never** commit credentials to source control
3. **Always** use environment variables or secrets management
4. **Always** rotate credentials between sessions
5. **Always** revoke credentials when work is complete

Violations of these principles are constitutional breaches requiring immediate remediation.
