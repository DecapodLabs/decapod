# SECURITY.md - Security Architecture

**Authority:** guidance (security patterns, threat modeling, and defense in depth)
**Layer:** Guides
**Binding:** No
**Scope:** security principles, threat modeling, and defensive patterns
**Non-goals:** specific security tools, compliance checklists

---

## 1. Security Principles

### 1.1 Defense in Depth
**No single point of failure.**
- Multiple layers of security
- If one layer fails, others protect
- No "silver bullet" security measure
- Assume breach will happen

**Layers:**
1. **Perimeter:** Firewalls, WAF, DDoS protection
2. **Network:** Segmentation, VPCs, encryption
3. **Application:** Input validation, auth, authorization
4. **Data:** Encryption, access controls, masking
5. **Physical:** Data center security (cloud handles this)

### 1.2 Principle of Least Privilege
**Give minimum access necessary.**
- Users: Only permissions needed for role
- Services: Only API calls needed to function
- Applications: Only file/database access required
- Regular access reviews

### 1.3 Zero Trust
**Never trust, always verify.**
- No implicit trust based on network location
- Verify every request, every time
- Assume network is compromised
- Strong authentication everywhere

### 1.4 Security by Design
**Security is not a feature; it's a property.**
- Consider security from design phase
- Threat model before implementation
- Security requirements are functional requirements
- Security reviews for architectural changes

---

## 2. Threat Modeling

### 2.1 STRIDE Methodology
**Threat categories:**
- **S**poofing: Pretending to be someone else
- **T**ampering: Modifying data/code
- **R**epudiation: Denying actions
- **I**nformation Disclosure: Leaking data
- **D**enial of Service: Making system unavailable
- **E**levation of Privilege: Gaining unauthorized access

### 2.2 Attack Surface Analysis
**Identify entry points:**
- APIs and endpoints
- Authentication mechanisms
- File uploads/downloads
- Admin interfaces
- Third-party integrations
- Logging and monitoring

### 2.3 Threat Modeling Process
1. **Diagram:** Create data flow diagram
2. **Identify:** Entry points and trust boundaries
3. **STRIDE:** Apply threat categories
4. **Rate:** Risk severity (likelihood × impact)
5. **Mitigate:** Design countermeasures
6. **Validate:** Review and test

---

## 3. Authentication

### 3.1 Passwords
**Requirements:**
- Minimum length: 12+ characters
- Complexity: Mix of character types
- No common passwords (check against breach databases)
- Rate limiting on login attempts
- Account lockout after failures
- Secure storage (bcrypt, Argon2, scrypt)

**Patterns:**
- Password reset via email with token
- Multi-factor authentication (MFA)
- Password managers encouraged

### 3.2 Multi-Factor Authentication (MFA)
**Factors:**
- **Something you know:** Password, PIN
- **Something you have:** Phone, hardware key
- **Something you are:** Fingerprint, face

**Implementation:**
- TOTP (Time-based One-Time Password)
- Push notifications
- Hardware security keys (FIDO2/WebAuthn)
- SMS (least secure, but better than nothing)

### 3.3 Session Management
- **Token-based:** JWT, opaque tokens
- **Session IDs:** Server-side sessions
- **Secure flags:** HttpOnly, Secure, SameSite
- **Expiry:** Short-lived access tokens
- **Refresh tokens:** Long-lived, rotate on use
- **Logout:** Invalidate tokens server-side

### 3.4 OAuth 2.0 / OpenID Connect
**Use for:**
- Third-party authentication ("Login with Google")
- Delegated authorization
- API access on user's behalf

**Security considerations:**
- Use PKCE for mobile/SPA
- Validate state parameter
- Verify ID token signatures
- Use HTTPS redirect URIs only

---

## 4. Authorization

### 4.1 RBAC (Role-Based Access Control)
- **Roles:** Group permissions (admin, user, guest)
- **Users:** Assigned to roles
- **Permissions:** Actions on resources

**When to use:** Hierarchical organizations, clear roles

### 4.2 ABAC (Attribute-Based Access Control)
- **Attributes:** User, resource, environment properties
- **Policies:** Rules combining attributes
- **Dynamic:** Context-aware decisions

**When to use:** Complex authorization, fine-grained control

### 4.3 ACL (Access Control Lists)
- **Resources:** Have list of who can access
- **Permissions:** Read, write, execute
- **Direct:** User-resource mapping

**When to use:** File systems, simple resource ownership

### 4.4 Authorization Best Practices
- **Deny by default:** Whitelist, not blacklist
- **Fail closed:** Deny if authorization check fails
- **Validate server-side:** Don't trust client
- **Least privilege:** Grant minimum necessary
- **Regular reviews:** Audit permissions

---

## 5. Data Protection

### 5.1 Encryption at Rest
- **Database:** Transparent Data Encryption (TDE)
- **Files:** Encrypt before storage
- **Backups:** Encrypted backup storage
- **Keys:** Managed by KMS, not in code

### 5.2 Encryption in Transit
- **TLS 1.2+:** Minimum version
- **Certificate pinning:** Mobile apps
- **HSTS:** Enforce HTTPS
- **mTLS:** Service-to-service authentication

### 5.3 Key Management
- **Never hardcode:** Use secret managers
- **Rotation:** Regular key rotation
- **Separation:** Different keys for different purposes
- **Access logging:** Audit key access
- **HSM:** Hardware Security Modules for high security

### 5.4 Data Classification
- **Public:** No restrictions
- **Internal:** Company use only
- **Confidential:** Restricted access
- **Restricted:** Compliance requirements (PII, PHI)

**Protection by classification:**
- Encryption requirements
- Access controls
- Logging and monitoring
- Retention policies

---

## 6. Input Validation

### 6.1 Validation Principles
- **Whitelist:** Allow known good, reject everything else
- **Sanitize:** Remove or escape dangerous content
- **Validate early:** At application boundary
- **Fail securely:** Reject invalid input

### 6.2 SQL Injection Prevention
- **Parameterized queries:** Never concatenate SQL
- **ORMs:** Use built-in query builders
- **Stored procedures:** Limit direct table access
- **Least privilege:** Database user permissions

### 6.3 XSS (Cross-Site Scripting) Prevention
- **Output encoding:** Escape based on context (HTML, JS, CSS, URL)
- **Content Security Policy (CSP):** Restrict script sources
- **HttpOnly cookies:** Prevent JavaScript access
- **Validate input:** Reject suspicious patterns

### 6.4 CSRF (Cross-Site Request Forgery) Prevention
- **CSRF tokens:** Unique per session
- **SameSite cookies:** Lax or Strict
- **Referrer checking:** Validate request source
- **Double-submit cookie:** Token in cookie and header

### 6.5 Command Injection Prevention
- **Avoid shell execution:** Use library functions
- **Input validation:** Strict whitelist
- **Escape arguments:** If shell execution required
- **Least privilege:** Limited execution permissions

---

## 7. Secure Development

### 7.1 Secure Coding Practices
- **Input validation:** All untrusted input
- **Output encoding:** Context-appropriate encoding
- **Authentication:** Verify identity
- **Authorization:** Check permissions
- **Error handling:** Don't leak sensitive info
- **Logging:** Security events, no sensitive data
- **Dependencies:** Regular updates, vulnerability scanning

### 7.2 Secrets Management
**Never commit secrets to code:**
- API keys
- Database passwords
- Private keys
- Encryption keys

**Use:**
- Environment variables
- Secret managers (Vault, AWS Secrets Manager)
- Encrypted configuration
- Runtime injection

### 7.3 Dependency Security
- **Inventory:** Know what you're using
- **Scanning:** Automated vulnerability detection
- **Updates:** Regular dependency updates
- **Pinning:** Lock versions for reproducibility
- **Minimal:** Only necessary dependencies

### 7.4 Security Testing
- **SAST:** Static Application Security Testing
- **DAST:** Dynamic Application Security Testing
- **Dependency scanning:** Known vulnerabilities
- **Penetration testing:** External security assessment
- **Fuzzing:** Automated input testing

---

## 8. Infrastructure Security

### 8.1 Network Security
- **VPCs:** Isolate resources
- **Subnets:** Public/private separation
- **Security groups:** Instance-level firewalls
- **NACLs:** Subnet-level rules
- **WAF:** Web Application Firewall
- **DDoS protection:** AWS Shield, Cloudflare

### 8.2 Container Security
- **Minimal images:** Reduce attack surface
- **No root:** Run as non-root user
- **Read-only filesystem:** Prevent modifications
- **Secrets:** Don't bake into images
- **Scanning:** Image vulnerability scanning
- **Runtime protection:** Detect anomalous behavior

### 8.3 Cloud Security
- **IAM:** Least privilege access
- **Encryption:** At rest and in transit
- **Logging:** CloudTrail, audit logs
- **Monitoring:** Security dashboards
- **Compliance:** Automated compliance checks

---

## 9. Incident Response

### 9.1 Preparation
- **Playbooks:** Documented response procedures
- **Tools:** Forensics, log analysis
- **Contacts:** Security team, legal, PR
- **Training:** Regular drills

### 9.2 Detection
- **Monitoring:** SIEM, anomaly detection
- **Alerting:** Paging for security events
- **Logging:** Centralized, tamper-proof
- **Honeypots:** Detect attackers early

### 9.3 Response
1. **Contain:** Stop the attack
2. **Eradicate:** Remove threat
3. **Recover:** Restore services
4. **Learn:** Post-incident review

### 9.4 Post-Incident
- **Root cause analysis:** What happened, why
- **Timeline:** When did it start, how discovered
- **Impact assessment:** What was affected
- **Remediation:** Prevent recurrence
- **Communication:** Notify affected parties

---

## 10. Anti-Patterns

- **Security through obscurity:** Assuming secrecy = security
- **Hardcoded credentials:** In code, configs, logs
- **No input validation:** Trusting all input
- **Verbose error messages:** Leaking implementation details
- **No rate limiting:** Brute force vulnerability
- **Weak cryptography:** MD5, SHA1, DES
- **No logging:** Can't detect or investigate breaches
- **Overly permissive CORS:** Allowing any origin
- **No HTTPS:** Transmitting secrets in plaintext
- **Ignoring security updates:** Running vulnerable dependencies

---

## Links

- `embedded/specs/SECURITY.md` - Security doctrine (binding)
- `embedded/methodology/ARCHITECTURE.md` - CTO-level architecture
- `embedded/architecture/WEB.md` - Web security
- `embedded/architecture/CLOUD.md` - Cloud security
