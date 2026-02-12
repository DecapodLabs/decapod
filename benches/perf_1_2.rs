use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

/// Benchmark TODO operations
fn bench_todo_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("todo_operations");

    // Setup temp directory and database
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("todos.db");

    group.bench_function("create_todo", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            counter += 1;
            let todo_id = format!("todo_{}", counter);
            let title = format!("Test TODO {}", counter);

            // Simulate TODO creation
            let todo_data = format!(
                "{}|{}|pending|{}",
                todo_id,
                title,
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            );

            // Append to mock database
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&db_path)
                .unwrap()
                .write_all(format!("{}\n", todo_data).as_bytes())
                .unwrap();

            black_box(todo_data);
        });
    });

    group.bench_function("update_todo_status", |b| {
        // Pre-populate with todos
        for i in 0..100 {
            let todo_data = format!(
                "todo_{}|Task {}|pending|{}\n",
                i,
                i,
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            );
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&db_path)
                .unwrap()
                .write_all(todo_data.as_bytes())
                .unwrap();
        }

        let mut counter = 0usize;
        b.iter(|| {
            counter = (counter + 1) % 100;
            let todo_id = format!("todo_{}", counter);
            let new_status = "completed";

            // Simulate status update
            let update = format!("{}|{}|{}\n", todo_id, "status_update", new_status);
            black_box(update);
        });
    });

    group.bench_function("track_todo_list_1000", |b| {
        // Create 1000 TODOs
        let mut todos = Vec::new();
        for i in 0..1000 {
            todos.push(format!("todo_{}", i));
        }

        b.iter(|| {
            // Simulate listing/filtering todos
            let pending_count = todos
                .iter()
                .filter(|_| true) // Mock filter
                .count();
            black_box(pending_count);
        });
    });

    group.finish();
}

/// Benchmark health claim and proof operations
fn bench_health_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("health_operations");

    group.bench_function("record_health_claim", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            counter += 1;
            let claim = HealthClaim {
                id: format!("claim_{}", counter),
                subject: "test_subject".to_string(),
                kind: "test_kind".to_string(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                provenance: "test".to_string(),
            };

            // Simulate claim recording
            let serialized = format!("{}", claim.id);
            black_box(serialized);
        });
    });

    group.bench_function("record_proof_event", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            counter += 1;
            let proof = ProofEvent {
                claim_id: format!("claim_{}", counter),
                proof_type: "validation".to_string(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                result: true,
            };

            // Simulate proof recording
            black_box(proof);
        });
    });

    group.bench_function("health_check_aggregation", |b| {
        // Create 100 claims with proofs
        let mut claims: Vec<HealthClaim> = Vec::new();
        let mut proofs: Vec<ProofEvent> = Vec::new();

        for i in 0..100 {
            claims.push(HealthClaim {
                id: format!("claim_{}", i),
                subject: "test".to_string(),
                kind: "test".to_string(),
                timestamp: 0,
                provenance: "test".to_string(),
            });
            proofs.push(ProofEvent {
                claim_id: format!("claim_{}", i),
                proof_type: "check".to_string(),
                timestamp: 0,
                result: i % 2 == 0,
            });
        }

        b.iter(|| {
            // Aggregate health status
            let mut health_map: HashMap<String, bool> = HashMap::new();
            for proof in &proofs {
                health_map.insert(proof.claim_id.clone(), proof.result);
            }

            let health_score =
                health_map.values().filter(|&&v| v).count() as f64 / health_map.len() as f64;

            black_box(health_score);
        });
    });

    group.finish();
}

/// Benchmark knowledge base operations
fn bench_knowledge_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("knowledge_operations");

    group.bench_function("create_kb_entry", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            counter += 1;
            let entry = KnowledgeEntry {
                id: format!("kb_{}", counter),
                title: format!("Knowledge {}", counter),
                content: format!("Content for knowledge entry {}", counter),
                tags: vec!["test".to_string()],
                created_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            black_box(entry);
        });
    });

    group.bench_function("search_kb_1000_entries", |b| {
        // Create 1000 knowledge entries
        let mut entries: Vec<KnowledgeEntry> = Vec::new();
        for i in 0..1000 {
            entries.push(KnowledgeEntry {
                id: format!("kb_{}", i),
                title: format!("Entry {}", i),
                content: format!("This is the content for entry number {}", i),
                tags: vec![format!("tag_{}", i % 10)],
                created_at: 0,
            });
        }

        b.iter(|| {
            // Simulate search by tag
            let results: Vec<&KnowledgeEntry> = entries
                .iter()
                .filter(|e| e.tags.contains(&"tag_5".to_string()))
                .collect();
            black_box(results.len());
        });
    });

    group.bench_function("update_kb_entry", |b| {
        let mut entry = KnowledgeEntry {
            id: "kb_test".to_string(),
            title: "Test".to_string(),
            content: "Original content".to_string(),
            tags: vec!["test".to_string()],
            created_at: 0,
        };

        b.iter(|| {
            // Simulate content update
            entry.content = format!(
                "Updated content at {}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            );
            entry.tags.push("updated".to_string());
            black_box(&entry);
        });
    });

    group.finish();
}

/// Benchmark policy evaluation operations
fn bench_policy_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("policy_operations");

    group.bench_function("simple_policy_eval", |b| {
        let policy = Policy {
            name: "test_policy".to_string(),
            rules: vec![Rule {
                condition: "risk < 5".to_string(),
                action: "allow".to_string(),
            }],
        };

        b.iter(|| {
            let risk = 3;
            let result = evaluate_policy(&policy, risk);
            black_box(result);
        });
    });

    group.bench_function("complex_nested_policy_eval", |b| {
        let policy = Policy {
            name: "complex_policy".to_string(),
            rules: vec![
                Rule {
                    condition: "risk < 3".to_string(),
                    action: "allow".to_string(),
                },
                Rule {
                    condition: "risk >= 3 && risk < 7".to_string(),
                    action: "review".to_string(),
                },
                Rule {
                    condition: "risk >= 7".to_string(),
                    action: "deny".to_string(),
                },
            ],
        };

        let mut counter = 0u8;
        b.iter(|| {
            counter = (counter + 1) % 10;
            let result = evaluate_policy(&policy, counter as i32);
            black_box(result);
        });
    });

    group.bench_function("policy_with_context", |b| {
        let policy = Policy {
            name: "context_policy".to_string(),
            rules: vec![
                Rule {
                    condition: "agent_trust > 0.8 && risk < 5".to_string(),
                    action: "auto_approve".to_string(),
                },
                Rule {
                    condition: "agent_trust > 0.5 && risk < 8".to_string(),
                    action: "approve".to_string(),
                },
                Rule {
                    condition: "risk >= 8".to_string(),
                    action: "deny".to_string(),
                },
            ],
        };

        b.iter(|| {
            let context = PolicyContext {
                agent_trust: 0.75,
                risk: 4,
            };
            let result = evaluate_policy_with_context(&policy, &context);
            black_box(result);
        });
    });

    group.finish();
}

/// Benchmark concurrent workflow operations
fn bench_concurrent_workflows(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_workflows");

    for concurrency in [10, 25, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_todo_creation", concurrency),
            concurrency,
            |b, &concurrency| {
                b.iter(|| {
                    let handles: Vec<_> = (0..concurrency)
                        .map(|i| {
                            std::thread::spawn(move || {
                                let todo_id = format!("todo_{}", i);
                                let todo_data = format!("{}|Task {}|pending", todo_id, i);
                                black_box(todo_data);
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

// Mock structures for benchmarking
#[derive(Debug, Clone)]
struct HealthClaim {
    id: String,
    subject: String,
    kind: String,
    timestamp: u64,
    provenance: String,
}

#[derive(Debug, Clone)]
struct ProofEvent {
    claim_id: String,
    proof_type: String,
    timestamp: u64,
    result: bool,
}

#[derive(Debug, Clone)]
struct KnowledgeEntry {
    id: String,
    title: String,
    content: String,
    tags: Vec<String>,
    created_at: u64,
}

#[derive(Debug)]
struct Policy {
    name: String,
    rules: Vec<Rule>,
}

#[derive(Debug)]
struct Rule {
    condition: String,
    action: String,
}

#[derive(Debug)]
struct PolicyContext {
    agent_trust: f64,
    risk: i32,
}

fn evaluate_policy(policy: &Policy, risk: i32) -> String {
    for rule in &policy.rules {
        if rule.condition.contains("< 3") && risk < 3 {
            return rule.action.clone();
        } else if rule.condition.contains("< 7") && risk >= 3 && risk < 7 {
            return rule.action.clone();
        } else if rule.condition.contains(">= 7") && risk >= 7 {
            return rule.action.clone();
        }
    }
    "deny".to_string()
}

fn evaluate_policy_with_context(_policy: &Policy, context: &PolicyContext) -> String {
    if context.agent_trust > 0.8 && context.risk < 5 {
        "auto_approve".to_string()
    } else if context.agent_trust > 0.5 && context.risk < 8 {
        "approve".to_string()
    } else {
        "deny".to_string()
    }
}

criterion_group!(
    benches,
    bench_todo_operations,
    bench_health_operations,
    bench_knowledge_operations,
    bench_policy_operations,
    bench_concurrent_workflows
);
criterion_main!(benches);
