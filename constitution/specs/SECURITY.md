# SECURITY.md - Security Architecture and Threat Model

**Authority:** binding (general security contract)
**Layer:** Constitution
**Binding:** Yes ⚠️
**Scope:** security philosophy, credential architecture, threat model, incident response
**Non-goals:** specific vulnerability disclosures, active CVE tracking

⚠️ **THIS IS A BINDING CONSTITUTIONAL CONTRACT. AGENTS MUST COMPLY.** ⚠️

---

## 1. The Security Philosophy

### 1.1 The Zero-Trust Imperative

Trust is a vulnerability. Every component, every user, every agent, and every pipeline must be verified at every access point. The perimeter is dead. The network is hostile. The supply chain is compromised by default until proven otherwise.

**Operational Principle:** Never trust any entity by default. Verify identity, verify authorization, verify integrity. Verify again.

### 1.2 Defense in Depth

No single control prevents compromise. Authentication alone fails. Encryption alone fails. A wall alone fails. Effective security requires layered controls where each layer can detect, delay, or deny attack progression.

**Operational Principle:** Design as if the attacker is already inside each layer. Assume layer N-1 is compromised. Can layer N still protect the asset?

### 1.3 The Convenience Paradox

Security inversely proportional to friction. Every control imposes a cost in cognitive load, latency, or workflow disruption. Controls that are too burdensome will be bypassed, documented in wikis that nobody reads, or defeated by "temporary" workarounds.

**Operational Principle:** Security controls must be frictionless by default. If a control is annoying, it will be circumvented. Design controls that make the secure path easier than the insecure path.

### 1.4 The Risk Management Reality

You cannot secure everything. Not every asset warrants every protection. Not every vulnerability requires remediation. The art of security is informed risk acceptance, not paranoid avoidance.

**Operational Principle:** Quantify risk in terms of impact and likelihood. Mitigate where the cost of mitigation is less than the expected loss. Accept what you cannot cost-effectively protect. Document every acceptance.

---

## 2. The Golden Rules

These are non-negotiable. Violate them only with explicit documented justification and compensating controls.

### 2.1 Least Privilege

Every entity—human, agent, service, or system—must have exactly the minimum access required to accomplish its function. Nothing more.

**Corollary:** Root is a deployment credential, not a daily-use credential. Service accounts should not have admin rights. Agents should not have keys that outlast their session.

### 2.2 Separation of Duties

No single entity should be able to complete a sensitive operation without another entity's involvement. This creates accountability and limits blast radius.

**Corollary:** The entity that writes code should not be the sole approver of that code's deployment. The agent that proposes a change should not be the sole approver of that change's merge.

### 2.3 Fail Secure

When a security control fails, the default behavior must be denial, not access. Errors should not default to "allow."

**Corollary:** Expired certificates block access, not allow insecure fallback. Missing permissions deny, not grant. Unverified signatures reject, not accept.

### 2.4 Complete Mediation

Every access to a protected resource must be checked. No shortcuts, no caches that bypass checks, no "trusted" internal calls that skip verification.

**Corollary:** Do not cache authorization decisions without TTL. Do not treat internal network as trusted without authentication.

---

## 3. Credential Architecture

Credentials are the primary attack surface. Poor credential hygiene is the leading cause of compromise.

### 3.1 Key Generation

**Requirements:**
- Minimum 256-bit entropy for symmetric keys
- RSA keys minimum 4096 bits, prefer Ed25519 or ECDH P-384
- Hardware-backed key generation when available (HSMs, TPMs, secure enclaves)
- Never generate keys on shared infrastructure

**The NSA Principle:** A key generated on a compromised machine is already compromised. The machine that generates your keys is a prime target.

### 3.2 Key Storage

**Requirements:**
- Keys never stored in plaintext
- Use dedicated secrets management: HSMs, key vaults, OS keychains
- Encryption at rest for all persistent key storage
- Memory cleared after use

**The Death Spiral:** Once a key is compromised, you must assume the attacker can access everything that key protects. The cost of key compromise is not the key itself—it is everything the key unlocks.

### 3.3 Key Rotation

**Requirements:**
- Automatic rotation for service accounts
- Time-based rotation schedules (shorter is safer, balance with operational risk)
- Event-triggered rotation: personnel changes, incident response, untrusted deployments

**The Rotation Imperative:** A key that has not rotated in a year is a ticking time bomb. Assume compromise with 100% certainty given enough time.

### 3.4 Key Revocation

**Requirements:**
- Documented revocation procedures for every credential type
- Fast-fail propagation: revocation must affect all systems within minutes
- Blocklist propagation: revoked keys must be rejected everywhere, immediately

**The Revocation Fantasy:** Revocation lists that take hours to propagate are revocation lists that fail when it matters. Design for minutes, not hours.

### 3.5 The Credential Lifecycle

Every credential must have a defined lifecycle:

```
Generate → Distribute → Use → Rotate → Revoke → Destroy
```

Missing any step creates gap vulnerability. Unknown credentials are unmanaged credentials. Unmanaged credentials are compromised credentials waiting to be found.

---

## 4. Agent-Specific Security

AI agents introduce new security dimensions. They act autonomously, they hold credentials, they access systems, and they create artifacts. They are not human, but they must be secured as if they were privileged users.

### 4.1 Agent Identity

Agents require verifiable identities. This identity must be:
- Unique per agent instance
- Verifiable at every action
- Revocable on compromise or termination

**Operational Principle:** Agent credentials are not eternal. They must have session-scoped tokens, heartbeat verification, and automatic expiration.

### 4.2 Session Lifecycle

**Requirements:**
- Time-to-live (TTL) on all agent sessions
- Heartbeat verification (agent must prove liveness)
- Automatic credential rotation within sessions
- Hard eviction after timeout (no zombie agents)
- Access binding MUST require `agent_id + ephemeral_password` per active session; stale-session cleanup MUST revoke assignments for expired agents (claim: `claim.session.agent_password_required`).

**The Zombie Problem:** An agent that runs forever with the same credentials is a sitting target. Every minute an agent runs without verification is a minute an attacker can hijack it.

### 4.3 Audit and Accountability

Every agent action must be logged with:
- Timestamp (synchronized)
- Identity (verifiable)
- Action (specific)
- Target (precise)
- Result (success/failure)
- Context (what triggered the action)

**Operational Principle:** If you cannot audit an agent's actions, you cannot trust the agent. Audit is not optional.

### 4.4 State Isolation

Agents must not bleed state. One agent's context must not leak to another. This applies to:
- Memory state
- Credentials
- Session tokens
- Artifact provenance

**The Contamination Problem:** If agent A's state can influence agent B's behavior, then compromise of A is compromise of B. Design for failure isolation.

### 4.5 Memory and Knowledge Redaction

Captured memory/knowledge artifacts must not persist raw secrets or credentials.

Minimum denylist targets:
- passwords and passphrases
- API keys and bearer tokens
- private keys and seed phrases
- authorization headers and session secrets

Operational rule:
- Persist pointers or redacted residues instead of raw secret-bearing blobs.
- Secret-pattern validation must fail loud when known credential patterns appear in persisted memory/retrieval logs.

---

## 5. Supply Chain Security

The supply chain is the attack surface. You do not just defend your code—you defend every dependency, every build artifact, every deployment pipeline.

### 5.1 Dependency Trust

Every dependency is an implicit trust decision. You are trusting:
- The maintainer's security practices
- The distribution channel's integrity
- The dependency's transitive dependencies

**The Dependency Lie:** Your application is only as secure as its most vulnerable dependency. The question is not if a dependency will be compromised—it is when.

**Operational Principle:**
- Audit dependencies regularly
- Pin versions (do not use floating versions in production)
- Use dependency lockfiles
- Scan for known vulnerabilities (automate this)
- Prefer maintained dependencies with active security response

### 5.2 Build Integrity

**Requirements:**
- Reproducible builds (verify what you build is what you deploy)
- Signed artifacts (verify provenance)
- Signed commits (verify authorship)
- No unsigned artifacts in deployment pipelines

**The Build Attack Surface:** If an attacker can modify your build process, they own your deployment. The build system is a prime target.

### 5.3 Deployment Pipeline Security

Every stage of the pipeline is a trust boundary:
- Source → Build: Verify authorship and integrity
- Build → Test: Verify test results, do not trust tests blindly
- Test → Staging: Verify environment parity
- Staging → Production: Verify approval and rollback capability

**Operational Principle:** The pipeline is a chain. It breaks at the weakest link. Secure every stage.

---

## 6. Incident Response Philosophy

Assume breach. Not because you are compromised—but because you might be and you need to be ready.

### 6.1 Detection

**Requirements:**
- Monitoring for anomalous behavior
- Alerting on credential use anomalies
- Log aggregation and correlation
- Anomaly detection for agent behavior

**The Detection Fantasy:** You cannot detect what you do not measure. You cannot respond to what you do not see. Visibility is prerequisite to response.

### 6.2 Containment

**Requirements:**
- Fast credential revocation (minutes, not hours)
- Network isolation of compromised components
- Preservation of evidence (do not delete logs)
- No premature cleanup (you might destroy evidence)

**The Cleanup Trap:** "Cleaning up" an incident before forensics destroys evidence. Contain first, investigate second, clean last.

### 6.3 Recovery

**Requirements:**
- Verified clean state (do not trust compromised systems)
- Credential re-rotation (every credential that touched the compromised system)
- Integrity verification (rebuild from known-good state)
- Lessons learned (document and improve)

### 6.4 Post-Incident

**Requirements:**
- Document timeline
- Identify root cause
- Identify attack vector
- Identify detection gaps
- Implement improvements
- Test improvements

**The Lesson Learned Theater:** Incidents without documented improvements are just stories. If you do not change your security posture after an incident, you will have another incident.

---

## 7. The Hard-Learned Truths

These are not theories. These are patterns observed across decades of security incidents.

### 7.1 Key Management Failures

**The Truth:**
- Keys in source code get leaked (they always get leaked)
- Keys in environment variables get logged, logged, and logged again
- Keys with long lifetimes give attackers time to find them
- Keys without rotation give attackers persistent access
- Keys without revocation procedures ensure compromise is permanent

**The Lesson:** Key management is not an afterthought. It is a primary security control. Get it right.

### 7.2 Social Engineering

**The Truth:**
- Even sophisticated technical people get phished
- Even security-conscious people reuse passwords
- Even paranoid people click links from "trusted" sources
- Even experts make mistakes under pressure

**The Lesson:** Technical controls cannot prevent all social engineering. Build systems that assume humans will be tricked. Require verification for sensitive actions.

### 7.3 The Insider Threat

**The Truth:**
- Most breaches are internal (people with access)
- Not all insiders are malicious—many are compromised
- Privileged access is a target for compromise
- Departing employees take access with them if not revoked

**The Lesson:** Access controls must assume internal threat models. Verify authorization on every action. Audit privileged access. Revoke immediately on termination.

### 7.4 Physical Security

**The Truth:**
- Digital controls do not stop physical access
- Keys on machines can be extracted with physical access
- Networks can be tapped at the physical layer
- Backdoors can be implanted in hardware

**The Lesson:** If an attacker has physical access, they have your system. Design systems that degrade gracefully under physical compromise.

---

## 8. Tradeoffs We Live With

Security is not absolute. Every decision involves tradeoffs. The mature approach acknowledges these tradeoffs rather than pretending they do not exist.

### 8.1 Speed vs Security

Sometimes speed matters more than security. Rapid response to incidents, fast deployment of fixes, quick iteration on features—all require accepting security risk.

**The Balance:** Accept this tradeoff explicitly. Document the risk. Implement compensating controls. Do not pretend the tradeoff does not exist.

### 8.2 Transparency vs Security

Open source is more secure because more eyes find bugs—but it also exposes attack surfaces. Transparency enables collaboration but also enables attack.

**The Balance:** The open-source security model has proven superior despite exposure. Publish what you can. Protect what you must.

### 8.3 Compliance vs Reality

Compliance checklists do not equal security. Checking boxes does not prevent breaches. Over-reliance on compliance creates false confidence.

**The Balance:** Compliance is a minimum bar, not a target. Meet compliance requirements, but do not mistake compliance for security. Test your controls, not just your documentation.

### 8.4 Usability vs Security

The most secure system that nobody can use is useless. The most usable system with no security is a disaster.

**The Balance:** Security must be usable to be effective. Invest in user experience of security controls. Frictionless security is more secure than annoying security.

---

## 9. Agent-Specific Guidance

### 9.1 Credential Handling

When an agent requires credentials:
1. **Never** log credentials
2. **Never** commit credentials to source control
3. **Never** use credentials across sessions without rotation
4. **Always** use dedicated service accounts with minimal scope
5. **Always** revoke credentials when the agent's work is complete
6. **Always** use environment variables or secret management systems, never hardcoded values

### 9.2 Git Security

1. **Sign commits** with a verified key (SSH or GPG)
2. **Verify remote URLs** before pushing (prevent repository hijacking)
3. **Review diffs** before commit (prevent accidental credential inclusion)
4. **Use protected branches** with required review
5. **Rotate deploy keys** regularly

### 9.3 CI/CD Security

1. **Never use** long-lived credentials in pipelines
2. **Use OIDC** for cloud provider authentication
3. **Rotate secrets** between pipeline runs
4. **Sign artifacts** at build time
5. **Verify signatures** at deployment time

### 9.4 Secrets Detection

If you accidentally commit a secret:
1. **Do not delete the commit**—this creates a gap in history
2. **Revoke the credential immediately**—assume it is compromised
3. **Rotate all related credentials**—the attacker may have found more
4. **Force-push a clean branch** after rebase
5. **Document the incident**—learn from it

---

## 10. Enforcement

This document is binding. Agents must:
- Follow the Golden Rules in all operations
- Implement credential lifecycle management for all credentials they create
- Log all security-relevant actions
- Report security anomalies immediately
- Never bypass security controls without documented justification

**Violation of these principles is a constitutional breach requiring immediate remediation.**

---

*This document is inspired by decades of security failures, hard-won lessons, and the fundamental truth that security is a process, not a product. Trust nothing. Verify everything. Document decisions. Learn from failures.*

## Links

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` - System definition and authority doctrine
- `specs/GIT.md` - Git etiquette contract
- `specs/AMENDMENTS.md` - Change control

### Registry (Core Indices)
- `core/PLUGINS.md` - Subsystem registry
- `core/INTERFACES.md` - Interface contracts index
- `core/METHODOLOGY.md` - Methodology guides index

### Contracts (Interfaces Layer)
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/CLAIMS.md` - Promises ledger
- `interfaces/GLOSSARY.md` - Term definitions

### Practice (Methodology Layer)
- `methodology/SOUL.md` - Agent identity

### Operations (Plugins Layer)
- `plugins/EMERGENCY_PROTOCOL.md` - Emergency protocols

### Architecture Patterns
- `architecture/SECURITY.md` - Security architecture patterns
