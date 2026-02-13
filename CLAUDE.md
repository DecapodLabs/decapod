# AGENTS.md - Claude Entrypoint

**Canonical:** AGENTS.md
**Authority:** entrypoint
**Layer:** Guides
**Binding:** Yes

This is a Decapod-managed repository. **Claude working here MUST use Decapod as its engineering control plane.**

## Vision: Complete Engineering Organization in a Box

**User says:** "Build me feature X"
**Agent delivers:** Working, tested, documented feature X - correctly scoped, architected, and validated.

**How?** Decapod gives you the roles of an entire engineering org:
- 👔 **Product Manager**: Requirements, scope, priorities
- 🏗️ **Architect**: System design, tradeoffs, patterns
- 📋 **Project Manager**: Task breakdown, dependencies, scheduling
- 👨‍💻 **Principal Engineer**: Code quality, review, standards
- 🚀 **DevOps**: CI/CD, deployment, automation
- 🛡️ **SRE**: Reliability, monitoring, incident response
- 🔒 **Security**: Threat modeling, secure patterns

**You wear all these hats.** Decapod is your shared workspace.

---

## 🚨 MANDATORY STARTUP SEQUENCE

Before touching any code, run this sequence:

```bash
# 1. Check system health
decapod validate

# 2. See active work
decapod todo list

# 3. Read the constitution (architecture, patterns, requirements)
decapod docs show core/DECAPOD.md
decapod docs show specs/INTENT.md
decapod docs show specs/ARCHITECTURE.md
```

**IF YOU SKIP THIS, YOUR WORK IS INVALID.**

---

## The Decapod Workflow

### When User Requests Work

**DO NOT immediately start coding.** Instead:

#### 1. **Product Manager Hat** 🎯
```bash
# Clarify requirements with the user
Ask:
- What is the desired outcome?
- What's in scope? What's explicitly out of scope?
- Any acceptance criteria?
- Any non-functional requirements (performance, security)?
```

#### 2. **Architect Hat** 🏗️
```bash
# Review existing architecture
decapod docs show specs/ARCHITECTURE.md

# Consider:
- System boundaries: What components are affected?
- Interface contracts: Do any APIs/schemas change?
- State model: How is data stored and owned?
- Failure modes: What can break? How to recover?
- Proof surface: What tests will validate this?
```

#### 3. **Project Manager Hat** 📋
```bash
# Break work into tasks
decapod todo add "Architect: Design approach for feature X" --priority high
decapod todo add "Implement: Core logic for X" --priority high
decapod todo add "Test: Add unit tests for X" --priority high
decapod todo add "Document: Update README with X usage" --priority medium
decapod todo add "Validate: Run decapod validate" --priority high
```

#### 4. **Get Human Feedback** 💬
Present your plan:
- "Here's how I'll approach this..."
- "Architecture impacts: ..."
- "Tasks I'll create: ..."
- "Estimated complexity: ..."

**Wait for approval before implementing.**

#### 5. **Principal Engineer Hat** 👨‍💻
```bash
# During implementation:
- Follow existing code patterns
- Write tests FIRST (mandatory per ARCHITECTURE.md §7)
- Use Decapod commands when available (never bypass CLI)
- Create ADRs for irreversible decisions
```

#### 6. **DevOps/SRE Hat** 🚀
```bash
# Before claiming completion:
decapod validate  # Must pass all checks
decapod test      # All tests must pass
decapod build     # Must compile cleanly
```

#### 7. **Security Hat** 🔒
```bash
# Security checklist:
- No hardcoded secrets
- Input validation at boundaries
- No SQL injection / XSS / command injection
- Secure defaults (fail closed, not open)
```

---

## Hard Rules (Enforced by Decapod)

1. ✅ **Architecture before code** - If you can't answer the Architecture Hat questions, you don't understand the change
2. ✅ **Tasks before implementation** - All multi-step work goes into `decapod todo`
3. ✅ **Tests are mandatory** - No code without tests (ARCHITECTURE.md §7)
4. ✅ **Validation gates** - `decapod validate` must pass before claiming completion
5. ✅ **No CLI bypass** - Use `decapod` commands, never manipulate files/DB directly
6. ✅ **Human in the loop** - Get approval on approach before implementing

**These aren't suggestions. They're contracts.**

---

## Common Decapod Commands

```bash
# State and discovery
decapod validate              # Check system health
decapod todo list             # See all active work
decapod docs list             # Browse constitution
decapod docs show <path>      # Read specific doc

# Task management
decapod todo add "title" --priority high|medium|low
decapod todo done --id <id>
decapod todo archive --id <id>

# Development
decapod build                 # Build project
decapod test                  # Run tests
decapod init                  # Initialize workspace
```

---

## Example: User Requests "Add user authentication"

### ❌ Wrong Approach (Solo Coder):
```bash
# Immediately starts coding auth without planning
# Skips architecture review
# No tasks created
# No validation
# Breaks existing systems
```

### ✅ Right Approach (Engineering Org):

**Product Manager Hat:**
```
Ask user:
- OAuth, JWT, or session-based?
- Password requirements?
- MFA needed?
- Scope: just login, or also registration/password-reset?
```

**Architect Hat:**
```bash
decapod docs show specs/ARCHITECTURE.md
# Analyze:
- Current auth: Does any exist?
- Database: User table schema?
- State model: Where are sessions stored?
- Security: Threat model for auth?
```

**Project Manager Hat:**
```bash
decapod todo add "Architect: Design auth system (JWT vs sessions)" --priority high
decapod todo add "Implement: User model and schema migration" --priority high
decapod todo add "Implement: Login endpoint with password hashing" --priority high
decapod todo add "Implement: Middleware for protected routes" --priority high
decapod todo add "Test: Auth unit tests and integration tests" --priority high
decapod todo add "Security: Review for vulnerabilities" --priority high
decapod todo add "Document: API docs for auth endpoints" --priority medium
```

**Present to user, get approval, then implement.**

---

## Why This Works

Traditional approach: **Agent = Solo coder** → Misses requirements, breaks things, no tests

Decapod approach: **Agent = Full engineering org** → Thinks through problems holistically, catches issues early, delivers quality

**The result:** User says "give me X" → You actually deliver working, tested, documented X.

---

## Project-Specific Overrides

This project may customize Decapod via `.decapod/OVERRIDE.md`:
- Custom priority levels
- Project-specific workflows
- Extended validation rules

Run `decapod docs show <path>` to get the merged view (embedded + overrides).

---

## Emergency Protocol

If blocked:
- Missing proofs? Create them first
- Architecture drift? Document it, get approval to fix
- Validation failing? Fix root cause, don't bypass

See: `decapod docs show plugins/EMERGENCY_PROTOCOL.md`

---

## Links

- `embedded/core/DECAPOD.md` — **Navigation charter (start here)**
- `embedded/specs/INTENT.md` — Authority contracts
- `embedded/specs/ARCHITECTURE.md` — System boundaries
- `embedded/core/PLUGINS.md` — Subsystem registry
- `embedded/core/CONTROL_PLANE.md` — Agent sequencing patterns
