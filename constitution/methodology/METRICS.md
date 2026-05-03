# METRICS.md - Project and Agent Performance Metrics

**Authority:** guidance (performance measurement standards)
**Layer:** Methodology
**Binding:** No
**Scope:** Metrics collection, reporting, and analysis for agentic projects

---

## 1. Agent Performance Metrics

### Token Efficiency
- **Prompt tokens:** Context injected per task
- **Completion tokens:** Output generated per task
- **Token cost:** Estimated cost per 1K tokens
- **Context reuse:** % of context from session vs fresh

### Task Completion
- **Tasks completed:** Total tasks finished per session
- **Tasks abandoned:** Tasks started but not completed
- **Context switches:** Times intent was re-clarified
- **Proof artifacts:** % of tasks with generated proof

### Governance Adherence
- **Intent clarifications requested:** Times agent asked for clarification
- **Boundaries respected:** % of boundary checks passed
- **Proof verification:** % of completions with VERIFIED status

---

## 2. Project Health Metrics

### Code Quality
- **Validation pass rate:** % of `decapod validate` passes
- **Proof coverage:** % of tasks with proof artifacts
- **Test coverage:** Code coverage percentages

### Operational Metrics
- **Build success rate:** CI/CD pipeline pass rate
- **Deployment frequency:** Releases per time period
- **Mean time to recovery:** Incident recovery time

---

## 3. Governance Metrics

### Intent Clarity
- **Clarification rate:** Tasks requiring intent clarification
- **Intent drift:** Cases where final output != initial intent

### Context Efficiency
- **Context relevance:** % of injected context actually used
- **Context bloat:** Instances of full-repo context injection
- **Token budget adherence:** % of tasks within estimated budget

---

## 4. Reporting

Agents should report metrics in:
- `constitution/generated/metrics/session.json`
- `constitution/generated/metrics/validation.json`

Metrics are computed deterministically from stored state.
