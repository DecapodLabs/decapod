# Counter-Vibe Coding: Production Readiness Delivery Chain

This package translates rapid AI-assisted prototyping into production engineering work.

## Scope
- Repository: `decapod`
- Objective: ensure systems are operable, secure, diagnosable, and evolvable
- Method: dependency-linked one-shot tasks with explicit artifacts

## Task-to-artifact map
- #2 Infrastructure blueprint -> `01-infrastructure-blueprint.md`
- #3 Security boundary model -> `02-security-boundary-model.md`
- #4 Deployment and rollback strategy -> `03-deployment-rollback-strategy.md`
- #5 Error-handling contract -> `04-error-handling-contract.md`
- #6 Structured logging baseline -> `05-structured-logging-baseline.md`
- #7 Monitoring/SLI/SLO model -> `06-monitoring-sli-slo-model.md`
- #8 Alerting and incident runbooks -> `07-alerting-incident-runbooks.md`
- #9 Engineer-guided AI guardrails -> `08-engineer-guided-ai-guardrails.md`
- #10 Production debt register -> `09-production-debt-register.md`
- #11 Final readiness proof gate -> `10-final-readiness-proof-gate.md`

## Dependency chain
- Foundation: #2
- Security/deploy/error contracts: #3, #4, #5 depend on #2
- Observability: #6 depends on #4 and #5; #7 depends on #4 and #6; #8 depends on #7
- AI governance: #9 depends on #2, #3, #5
- Debt governance: #10 depends on #3, #4, #5, #6, #9
- Final gate: #11 depends on #8, #9, #10

## Exit criteria
- All artifacts exist and are internally consistent
- All 11 tasks are closed
- Readiness gate status in `10-final-readiness-proof-gate.md` is recorded
