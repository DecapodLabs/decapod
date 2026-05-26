# Intent

In Decapod, **Intent** is the primary driver of the development lifecycle. It is the human-originated "Why" that must be preserved and formalized before implementation begins.

## The Spec-First Mandate

Agents often dive into implementation before fully understanding the human operator's goal. Decapod prevents this by mandating a spec-first approach:

1.  **Capture:** Vague requests are converted into explicit tasks via `decapod todo add`.
2.  **Formalize:** Agents are required to update or verify the `specs/INTENT.md` document scaffolded in the repository.
3.  **Validate:** The final implementation is checked against this intent. If the outcome deviates from the recorded intent, validation fails.

## Intent Pressure

"Intent Pressure" occurs when an agent encounters ambiguity. Decapod's philosophy is that **uncertainty must be preserved, not compressed**. If an agent is 70% sure of a requirement, it must not guess the remaining 30%. Instead, it should record the ambiguity in the task's metadata or `INTENT.md` and request human clarification.

## Versioned Specifications

Because Decapod is repo-native, your specifications are versioned alongside your code. This creates a permanent, auditable link between a feature's requirements and its implementation, which is invaluable for long-term maintenance and multi-agent handover.
