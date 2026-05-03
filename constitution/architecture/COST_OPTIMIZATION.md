# COST_OPTIMIZATION.md - Cloud and Resource Cost Management

**Authority:** guidance (cost management)
**Layer:** Architecture
**Binding:** No
**Scope:** Cloud costs, resource allocation, and token economics

---

## 1. Cloud Cost Management

### Resource Right-Sizing
- **Compute:** Match instance size to actual usage
- **Storage:** Use appropriate storage classes (hot/warm/cold)
- **Network:** Minimize data transfer costs

### Cost Visibility
- Tag all resources by: team, service, environment
- Daily cost alerts at thresholds
- Weekly cost reports

### Optimization Strategies
- **Reserved instances:** For steady-state workloads
- **Spot instances:** For fault-tolerant batch jobs
- **Serverless:** For variable/unpredictable loads

---

## 2. Token Economics

### Context Efficiency
- **Target:** < 50K tokens per task
- **Budget:** Track token usage per task type
- **Optimization:** Reuse context from session state

### Model Selection
- **Simple tasks:** Use smaller/faster models
- **Complex reasoning:** Reserve premium models
- **Batch processing:** Use batch-optimized models

### Cost Tracking
```json
{
  "tokens": {
    "prompt": 5000,
    "completion": 2000,
    "cached": 3000
  },
  "cost_usd": 0.15
}
```

---

## 3. Agent-Specific Costs

### Context Waste Prevention
- Inject only relevant files
- Use session context when possible
- Exclude unnecessary documentation

### Proof Generation
- Balance proof thoroughness vs cost
- Cache proof templates
- Use incremental proofs when possible

---

## 4. Governance

### Budget Alerts
- **Warning:** 80% of budget consumed
- **Critical:** 95% of budget consumed
- **Action Required:** 100% budget exceeded

### Cost Attribution
- Per-team cost tracking
- Per-service cost tracking
- Per-feature cost tracking

---

## 5. Agent Guidelines

When agents make resource decisions:
1. Consider cost as a non-functional requirement
2. Use conservative resource estimates
3. Implement auto-scaling where possible
4. Clean up unused resources
