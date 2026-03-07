# CLOUD.md - Cloud Architecture

**Authority:** guidance (cloud infrastructure, deployment patterns, and operational excellence)
**Layer:** Guides
**Binding:** No
**Scope:** cloud platforms, infrastructure patterns, and DevOps practices
**Non-goals:** specific cloud provider tutorials, vendor-specific implementations

---

## 1. The Oracle's Verdict: Cloud as Utility, Not Magic

*The cloud is just someone else's computer, but it charges by the millisecond. If you do not engineer for cost and failure, the cloud will bankrupt you before it scales you.*

### 1.1 The CTO's Strategic View
- **Unit Economics over Vanity Metrics:** Cloud architecture must map directly to business unit economics. If serving one customer costs more than the revenue they generate, the architecture is fundamentally broken, regardless of how elegantly it scales.
- **Portability as Leverage:** Total vendor lock-in is a failure of negotiation. While using managed services (PaaS/Serverless) accelerates velocity, the core domain logic must remain portable enough to migrate if a vendor's pricing becomes predatory.

### 1.2 The SVP's Operational View
- **Infrastructure as Data:** Infrastructure as Code (IaC) is not enough; infrastructure must be versioned, tested, and promoted identically to application code. "Click-ops" (using the AWS/GCP console to change state) in production is a fireable offense.
- **Cost is an Engineering Metric:** Cloud cost is not a finance problem; it is an engineering problem. Every PR must have a predictable cost impact. If a developer cannot explain the cost of their architecture, it cannot be merged.

### 1.3 The Architect's Structural View
- **Embrace the Chaos (Design for Failure):** The network is not reliable, latency is not zero, and instances will die. Architecture must handle the loss of any single node, zone, or service gracefully. If an outage in a non-critical downstream service brings down the primary flow, the architecture has failed.
- **Stateless by Default:** Compute must be ephemeral and stateless. State belongs only in managed databases or object storage. If an auto-scaling group cannot kill an instance safely at any moment, the system is brittle.

### 1.4 The Principal's Execution View
- **The Limits of Serverless:** FaaS (Lambda/Cloud Functions) is phenomenal for event-driven glue, but terrible for consistent, high-throughput, low-latency APIs. Choose the compute model based on the workload's shape, not the hype cycle.
- **Least Privilege is Absolute:** IAM roles must be tightly scoped per service. A compromised container must not have access to the S3 buckets of another service. Wildcard (`*`) permissions in production IAM policies are a critical security violation.

---

## 2. Cloud Architecture Principles

### 1.1 Design for Failure
**Everything fails, all the time.**
- Hardware fails
- Networks partition
- Services degrade
- Regions go offline

**Resilience requires:**
- Redundancy at every layer
- Automated recovery
- Graceful degradation
- Circuit breakers and bulkheads

### 1.2 Elasticity
**Scale horizontally, not vertically.**
- Add/remove instances based on demand
- Stateless services enable elasticity
- Auto-scaling based on metrics
- Scale to zero for cost savings (serverless)

### 1.3 Infrastructure as Code (IaC)
**If it's not in code, it doesn't exist.**
- Version-controlled infrastructure
- Reproducible environments
- Peer review for changes
- Automated testing and deployment

### 1.4 Cost Awareness
**Cloud costs are architecture decisions.**
- Visibility into spending
- Reserved capacity for steady-state
- Spot instances for fault-tolerant workloads
- Right-sizing resources

---

## 2. Compute Options

### 2.1 Virtual Machines (IaaS)
**When to use:**
- Legacy applications
- Full control over OS
- Specific kernel requirements
- Long-running compute

**Examples:** EC2, GCE, Azure VMs

### 2.2 Containers (CaaS)
**When to use:**
- Microservices
- Consistent environments
- Rapid scaling
- Resource efficiency

**Orchestration:**
- Kubernetes: Industry standard, complex
- ECS/Fargate: AWS-native, simpler
- Cloud Run: Serverless containers

### 2.3 Serverless (FaaS)
**When to use:**
- Event-driven workloads
- Variable traffic
- Rapid development
- Cost optimization (pay per use)

**Examples:** Lambda, Cloud Functions, Azure Functions

**Limitations:**
- Cold start latency
- Execution time limits
- Vendor lock-in
- Limited local state

### 2.4 Platform as a Service (PaaS)
**When to use:**
- Focus on application, not infrastructure
- Rapid prototyping
- Standard web applications

**Examples:** Heroku, App Engine, Elastic Beanstalk

---

## 3. Deployment Patterns

### 3.1 Blue-Green Deployment
- Two identical environments
- Instant cutover (DNS or LB switch)
- Easy rollback
- Requires double capacity

### 3.2 Canary Deployment
- Deploy to small subset of users
- Monitor metrics
- Gradually increase traffic
- Automatic rollback on errors

### 3.3 Rolling Deployment
- Replace instances gradually
- No capacity overhead
- Slower rollback
- Version mix during deployment

### 3.4 Feature Flags
- Decouple deployment from release
- Gradual rollout by user segment
- A/B testing
- Instant rollback (toggle off)

---

## 4. High Availability

### 4.1 Multi-AZ (Availability Zone)
- Deploy across 3 AZs minimum
- AZs are independent data centers
- Automatic failover
- No additional latency

### 4.2 Multi-Region
- Deploy to multiple regions
- Active-active or active-passive
- Geographic redundancy
- DR for region failure
- Data residency compliance

### 4.3 Load Balancing
- **Layer 4 (TCP):** Fast, simple
- **Layer 7 (HTTP):** Content-based routing
- **Health checks:** Route around failures
- **Sticky sessions:** Minimize (breaks elasticity)

### 4.4 Health Checks
- **Liveness:** Is the process running?
- **Readiness:** Is it ready to serve traffic?
- **Startup:** Is initialization complete?
- Separate probes for different concerns

---

## 5. Storage in Cloud

### 5.1 Object Storage (S3, GCS, Blob)
- **Use for:** Files, images, backups, static assets
- **Benefits:** Infinite scale, high durability, cheap
- **Limitations:** No partial updates, eventual consistency
- **Performance:** CloudFront/CloudFlare for edge caching

### 5.2 Block Storage (EBS, Persistent Disks)
- **Use for:** VM disks, databases
- **Types:** SSD (performance), HDD (capacity)
- **Snapshots:** Point-in-time backups
- **Multi-attach:** Some volumes to multiple instances

### 5.3 File Storage (EFS, Filestore)
- **Use for:** Shared filesystems
- **Benefits:** NFS-compatible, auto-scaling
- **Latency:** Higher than block storage

---

## 6. Networking

### 6.1 Virtual Private Cloud (VPC)
- Isolated network environment
- Subnets (public/private)
- Route tables control traffic flow
- Network ACLs and security groups

### 6.2 Security Groups vs NACLs
**Security Groups (Stateful):**
- Instance-level
- Allow rules only
- Stateful (return traffic automatic)
- Default deny

**NACLs (Stateless):**
- Subnet-level
- Allow and deny rules
- Stateless (explicit return rules)
- Ordered rules

### 6.3 API Gateway
- **Purpose:** Entry point for APIs
- **Features:** Rate limiting, auth, caching, monitoring
- **Benefits:** Decouple clients from services
- **Patterns:** BFF, aggregation, protocol translation

### 6.4 Service Mesh
- **Purpose:** Service-to-service communication
- **Features:** mTLS, traffic management, observability
- **Examples:** Istio, Linkerd, AWS App Mesh
- **Trade-off:** Complexity vs capabilities

---

## 7. Operational Excellence

### 7.1 Monitoring
- **Metrics:** CloudWatch, Datadog, Prometheus
- **Logs:** Centralized (ELK, Splunk, CloudWatch)
- **Traces:** Distributed tracing (Jaeger, Zipkin)
- **Alerts:** Paging for symptoms, not causes

### 7.2 CI/CD
- **Pipeline:** Build → Test → Deploy
- **Automation:** Reduce manual steps
- **Testing:** Unit, integration, security, performance
- **GitOps:** Git as source of truth for deployments

### 7.3 Disaster Recovery
- **RPO (Recovery Point Objective):** Max acceptable data loss
- **RTO (Recovery Time Objective):** Max acceptable downtime
- **Backup strategies:** Automated, tested, offsite
- **Runbooks:** Documented procedures

### 7.4 Cost Optimization
- **Right-sizing:** Match resources to workload
- **Reserved instances:** Predictable workloads
- **Spot instances:** Fault-tolerant batch jobs
- **Auto-scaling:** Scale down when not needed
- **Tagging:** Attribute costs to teams/projects

---

## 8. Security in Cloud

### 8.1 Identity and Access Management (IAM)
- **Principle:** Least privilege
- **Roles:** Service accounts, user roles
- **Policies:** Resource-level permissions
- **Rotation:** Regular key rotation

### 8.2 Secrets Management
- **Never hardcode:** Use secret managers
- **Rotation:** Automated secret rotation
- **Audit:** Who accessed what secret when
- **Examples:** AWS Secrets Manager, HashiCorp Vault

### 8.3 Encryption
- **At rest:** Database, storage encryption
- **In transit:** TLS everywhere
- **Key management:** KMS, HSM for high security
- **BYOK:** Bring your own key (compliance)

### 8.4 Network Security
- **Private subnets:** No direct internet
- **Bastion hosts:** Controlled access
- **VPN/Direct Connect:** Secure on-prem connectivity
- **WAF:** Web application firewall

---

## 9. Anti-Patterns

- **Lift and shift:** Not leveraging cloud benefits
- **Giant VMs:** Vertical scaling instead of horizontal
- **No automation:** Manual deployments and changes
- **Hardcoded credentials:** Security nightmare
- **Public everything:** Default public access
- **No monitoring:** Flying blind
- **Single region:** No DR capability
- **Over-provisioning:** Wasting money
- **No IaC:** Click-ops infrastructure
- **Ignoring costs:** Surprise bills

---

## Links

- `methodology/ARCHITECTURE.md` - binding architecture doctrine
- `architecture/SECURITY.md` - Security architecture
- `architecture/OBSERVABILITY.md` - Monitoring and observability
- `architecture/CONCURRENCY.md` - Distributed systems patterns
