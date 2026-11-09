# Capacity assumptions, not a hosted load result

Pricing checked against [Supabase pricing](https://supabase.com/pricing) on
2026-11-09: Free includes 500 MB database, 50K MAU and 5 GB uncached egress;
Pro starts at $25/month with 8 GB disk, 100K MAU and 250 GB uncached egress.
Published overages are $0.125/GB disk, $0.00325/MAU and $0.09/GB uncached egress.
Compute, other storage and operational overhead must be budgeted separately.

Use decimal bytes/GB below. Assumptions: 10% of registered users active daily,
20% monthly; each daily user performs 20 state transitions/day, one retained
event per transition, 30 days/month. Model PostgreSQL at **2,048 bytes/event
including a share of indexes/task rows**, pending actual native backend storage
measurements. This is an assumption, not a conversion of SQLite's measured bytes.
Actual checkpoint payloads can be much larger. No evidence deletion is assumed.

Response assumptions: 1 KiB per mutation plus four 64-row list pages/day at
600 bytes/row = 174,080 bytes/day/daily-active user. Real response envelopes,
TLS/header bytes, larger descriptions, polling, retries and additional reads
increase this. Request ingress is not included in the egress estimate.

| Registered | Daily active | MAU | Monthly events | DB growth/month | Response egress/month |
|---:|---:|---:|---:|---:|---:|
| 1,000 | 100 | 200 | 60,000 | 0.123 GB | 0.522 GB |
| 10,000 | 1,000 | 2,000 | 600,000 | 1.229 GB | 5.222 GB |
| 100,000 | 10,000 | 20,000 | 6,000,000 | 12.288 GB | 52.224 GB |
| 1,000,000 | 100,000 | 200,000 | 60,000,000 | 122.880 GB | 522.240 GB |

At 1K registered, 500 MB permits roughly four months of this event growth before
other application/Auth/index/system usage. At 10K registered, modeled database
growth exceeds Free in the first month and egress also crosses its allowance.
Even this light 10K **daily-active** scenario needs paid capacity: 12.288 GB/month
of additional retained application data. Registered users alone do not predict
load, and stored accounts/Auth overhead must be added even for inactive users.

At 1M registered the first month's modeled disk overage above 8 GB is about
$14.36 and uncached egress over 250 GB about $24.50. MAU overage is $325. These
plus the $25 plan give about **$388.86/month before additional compute, existing
retained history, Auth storage, backups, artifacts, monitoring and other costs**.
This is a resource-accounting floor under the stated assumptions, not a service
quote or a claim that the included compute can serve this workload. By month
12 the append-only modeled application history alone is about 1.47 TB.

Repository planning is more useful than a global user count. A repository with
eight agents each producing 100 transitions/day adds 24K events/month, modeled
at 49.15 MB/month. A 64-agent hot repository at one write/second/agent offers
64 writes/second to the repository lock. If a measured critical section were
20 ms, service capacity would be at most 50 writes/second and that load would
be unstable; if 5 ms, its serial service budget would be 200/second. These are
arithmetic examples, not measured transaction durations. Pool size cannot fix
an overloaded repository-local serialization point.

The global 1M registered scenario averages 23.15 transitions/second across all
repositories over a full day, with unspecified peak multiplier. A million users
each active daily would be a different scenario: ten times the modeled daily
activity, storage growth and egress. Benchmark active repositories, agents per
repository, request rate, lock duration, retry/error distribution and peak load
independently before selecting compute or claiming commercial capacity.
