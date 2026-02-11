# DEMANDS.md - User Demands & Agent Constraints


This document allows the human user to explicitly declare demands, preferences, and constraints for all AI agents operating within the Decapod Intent-Driven Engineering System. These demands supersede default agent behaviors and system configurations. Agents MUST consult and adhere to these declarations before taking action.

---

## 1. Operational Constraints

-   `[ ] require_manual_approval_for_commits`: Agents MUST NOT commit changes to version control without explicit human approval. (True/False)
-   `[ ] always_squash_commits`: Agents MUST squash all their changes into a single commit before pushing. (True/False)
-   `[ ] avoid_nodejs`: Agents MUST NOT introduce Node.js or npm dependencies into new projects or modules. (True/False)
-   `[ ] prefer_static_binaries`: Agents SHOULD prioritize building self-contained, statically linked binaries where possible. (True/False)
-   `[ ] limit_cpu_usage_to_percent`: Limit agent CPU usage to [PERCENTAGE]% when operating in background mode. (e.g., `50%`)
-   `[ ] limit_memory_usage_to_mb`: Limit agent memory usage to [MB]MB. (e.g., `2048MB`)

---

## 2. Code Generation & Style

-   `[ ] prefer_python_version`: Prefer Python version `[VERSION]`. (e.g., `3.10`)
-   `[ ] prefer_go_version`: Prefer Go version `[VERSION]`. (e.g., `1.21`)
-   `[ ] adhere_to_pep8`: Agents MUST adhere to PEP8 style guide for Python. (True/False)
-   `[ ] adhere_to_google_style`: Agents MUST adhere to Google style guides for relevant languages. (True/False)

---

## 3. Interaction & Reporting

-   `[ ] verbose_logging`: Agents SHOULD provide verbose logging of their thought process and actions. (True/False)
-   `[ ] summarize_changes`: Agents MUST summarize all proposed changes concisely before presenting them. (True/False)
-   `[ ] notify_on_blocking_tasks`: Agents MUST notify the user when a blocking TODO task is created or updated. (True/False)

---

## 4. Security & Privacy

-   `[ ] avoid_cleartext_credentials`: Agents MUST NEVER handle or log credentials in clear text. (True/False)

---

## 5. Agent-Specific Overrides

-   **[Agent Name]:**
    -   `[ ] custom_directive_for_agent_X`: [Specific demand for Agent X].

---

## Usage Notes:

-   To activate a demand, change `[ ]` to `[x]`.
-   For values, replace `[PLACEHOLDER]` with the desired value.
-   Add new demands or clarify existing ones as needed.
-   Agents will periodically scan this file for updates.

## Links

- `docs/templates/DEMANDS.md`
