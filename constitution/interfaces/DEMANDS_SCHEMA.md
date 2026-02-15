# DEMANDS_SCHEMA.md - User Demand Interface Contract

**Authority:** interface (machine-readable demand schema and precedence rules)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** demand declaration model, key typing, precedence, and validation gates
**Non-goals:** natural-language preference coaching

---

## 1. Purpose

User demands are explicit runtime constraints that override default agent behavior.

---

## 2. Record Model

Each demand record MUST include:
- `key` (stable snake_case)
- `value` (typed)
- `type` (`bool` | `int` | `string` | `enum`)
- `scope` (`global` | `repo` | `agent:<id>`)
- `source` (`human` | `policy`)
- `updated_ts`

Optional:
- `reason`
- `expires_ts`

---

## 3. Standard Keys

- `require_manual_approval_for_commits` (bool)
- `always_squash_commits` (bool)
- `avoid_nodejs` (bool)
- `prefer_static_binaries` (bool)
- `limit_cpu_usage_to_percent` (int, 1..100)
- `limit_memory_usage_to_mb` (int, >0)
- `prefer_python_version` (string)
- `prefer_go_version` (string)
- `adhere_to_pep8` (bool)
- `adhere_to_google_style` (bool)
- `verbose_logging` (bool)
- `summarize_changes` (bool)
- `notify_on_blocking_tasks` (bool)
- `avoid_cleartext_credentials` (bool)

Implementations MAY add keys, but custom keys MUST include type metadata.

---

## 4. Precedence

Resolution order (highest wins):
1. `agent:<id>` scope
2. `repo` scope
3. `global` scope

If two records conflict at same scope, latest `updated_ts` wins.

---

## 5. Invariants

1. Unknown keys MUST be treated as non-binding unless explicitly registered.
2. Type mismatch is validation failure.
3. Expired demands MUST not be enforced.
4. Dangerous keys (commit/push/credential-related) SHOULD be visible in command planning output.

---

## 6. Proof Surface

Primary gate: `decapod validate`.

Required checks:
- key/type conformance
- precedence determinism
- expiration handling
- schema serialization stability

---

## Links

- `core/INTERFACES.md` - Interface contracts registry
- `core/DEMANDS.md` - Demand routing and usage
- `specs/SECURITY.md` - Security constraints
- `specs/GIT.md` - Git constraints
