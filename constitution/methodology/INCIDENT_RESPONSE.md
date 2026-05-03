# INCIDENT_RESPONSE.md - Production Incident Handling

**Authority:** guidance (incident management procedures)
**Layer:** Methodology
**Binding:** No
**Scope:** Response procedures for production incidents

---

## 1. Incident Classification

### Severity Levels
- **SEV1:** Complete service outage
- **SEV2:** Major feature unavailable
- **SEV3:** Minor feature degraded
- **SEV4:** Non-critical issue

### Categories
- **Availability:** Service down or unresponsive
- **Data:** Data loss or corruption
- **Security:** Breach or vulnerability
- **Performance:** Severe latency or throughput degradation

---

## 2. Response Procedure

### Detection
- Automated alerts from observability systems
- User reports via designated channels

### Initial Response (0-15 minutes)
1. Acknowledge incident in #incident-response channel
2. Assess severity and category
3. Identify scope and impact
4. Assign incident commander

### Containment (15-60 minutes)
1. Implement temporary mitigation
2. Preserve evidence for post-mortem
3. Communicate status to stakeholders

### Resolution (60+ minutes)
1. Implement fix or rollback
2. Verify resolution
3. Document root cause

---

## 3. Agent Responsibilities

When assisting with incidents:
1. **Stop non-essential work** - Abandon tasks to focus on incident
2. **Use incident channel** - All comms in designated channel
3. **Preserve state** - Don't modify production without approval
4. **Document actions** - Log all changes made
5. **Request escalation** - Escalate if blocked or unclear

---

## 4. Post-Incident

### Post-Mortem Requirements
- Root cause analysis
- Timeline of events
- Impact assessment
- Corrective actions with owners

### Prevention
- Update validation gates to catch similar issues
- Add monitoring for early detection
- Update runbooks as needed

---

## 5. Escalation Contacts

Defined in `.decapod/config.yaml`:
```yaml
incident_response:
  channel: "#incidents"
  severity_matrix: "incidents/severity.yaml"
  on_call: "oncall.yaml"
```
