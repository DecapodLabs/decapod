# Engineer-Guided AI Guardrails

## Core principle
AI accelerates implementation. Engineers own boundaries, invariants, and long-term operability.

## Mandatory constraints for AI-generated changes
- Must name impacted invariants and proof gates
- Must define rollback and failure behavior for risky changes
- Must include test/validation updates where behavior changes
- Must avoid speculative abstraction without a concrete current use

## Review checklist
- Is architecture simpler after the change?
- Can ownership boundaries be explained in one pass?
- Are runtime and operational costs explicit?
- Is technical debt explicitly accepted or remediated?

## Rejection criteria
- Plausible but unverifiable design
- Increased coupling with no measurable benefit
- Hidden external dependencies or secret handling ambiguity
