#![allow(dead_code, unused_variables, clippy::useless_vec)]

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

/// Benchmark project initialization performance
fn bench_project_initialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("project_initialization");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("init_empty_project", |b| {
        b.iter(|| {
            let temp_dir = TempDir::new().unwrap();
            let project_path = temp_dir.path();

            // Simulate project initialization
            let decapod_dir = project_path.join(".decapod");
            fs::create_dir_all(&decapod_dir).unwrap();

            // Create minimal structure
            fs::write(decapod_dir.join("store.db"), "").unwrap();
            fs::write(decapod_dir.join("constitution.md"), "# Constitution").unwrap();

            black_box(project_path);
        });
    });

    group.bench_function("init_with_structure", |b| {
        b.iter(|| {
            let temp_dir = TempDir::new().unwrap();
            let project_path = temp_dir.path();

            // Create full Decapod structure
            let decapod_dir = project_path.join(".decapod");
            fs::create_dir_all(&decapod_dir).unwrap();
            fs::create_dir_all(decapod_dir.join("constitutions")).unwrap();
            fs::create_dir_all(decapod_dir.join("archives")).unwrap();
            fs::create_dir_all(decapod_dir.join("sessions")).unwrap();

            // Create core files
            fs::write(decapod_dir.join("store.db"), "").unwrap();
            fs::write(decapod_dir.join("constitution.md"), "# Constitution").unwrap();
            fs::write(decapod_dir.join("AGENTS.md"), "# Agents").unwrap();

            black_box(project_path);
        });
    });

    group.finish();
}

/// Benchmark validation and compliance checking
fn bench_validation_compliance(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation_compliance");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("validate_structure", |b| {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();
        let decapod_dir = project_path.join(".decapod");
        fs::create_dir_all(&decapod_dir).unwrap();
        fs::write(decapod_dir.join("store.db"), "").unwrap();
        fs::write(decapod_dir.join("constitution.md"), "# Constitution").unwrap();

        b.iter(|| {
            // Simulate validation checks
            let store_exists = decapod_dir.join("store.db").exists();
            let constitution_exists = decapod_dir.join("constitution.md").exists();
            let _ = black_box((store_exists, constitution_exists));
        });
    });

    group.bench_function("compliance_deep_check", |b| {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();
        let decapod_dir = project_path.join(".decapod");
        fs::create_dir_all(&decapod_dir).unwrap();
        fs::create_dir_all(decapod_dir.join("constitutions")).unwrap();
        fs::write(decapod_dir.join("store.db"), "").unwrap();

        // Create nested constitution structure
        for i in 0..10 {
            fs::write(
                decapod_dir
                    .join("constitutions")
                    .join(format!("constitution_{}.md", i)),
                format!("# Constitution {}", i),
            )
            .unwrap();
        }

        b.iter(|| {
            let constitutions_dir = decapod_dir.join("constitutions");
            let entries: Vec<_> = fs::read_dir(&constitutions_dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .collect();
            black_box(entries.len());
        });
    });

    group.finish();
}

/// Benchmark archive and restore operations
fn bench_archive_restore(c: &mut Criterion) {
    let mut group = c.benchmark_group("archive_restore");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("archive_small_session", |b| {
        b.iter(|| {
            let temp_dir = TempDir::new().unwrap();
            let project_path = temp_dir.path();
            let decapod_dir = project_path.join(".decapod");
            let archive_dir = decapod_dir.join("archives");
            let session_dir = decapod_dir.join("sessions").join("test_session");

            fs::create_dir_all(&session_dir).unwrap();
            fs::create_dir_all(&archive_dir).unwrap();

            // Create session data
            fs::write(session_dir.join("data.txt"), "session data").unwrap();

            // Simulate archive operation
            let archive_path = archive_dir.join("test_session.tar");
            fs::copy(session_dir.join("data.txt"), &archive_path).unwrap();

            black_box(archive_path);
        });
    });

    group.bench_function("restore_session", |b| {
        b.iter(|| {
            let temp_dir = TempDir::new().unwrap();
            let project_path = temp_dir.path();
            let decapod_dir = project_path.join(".decapod");
            let archive_dir = decapod_dir.join("archives");
            let restore_dir = decapod_dir.join("restored");

            fs::create_dir_all(&archive_dir).unwrap();
            fs::create_dir_all(&restore_dir).unwrap();

            // Create archive
            let archive_path = archive_dir.join("session.tar");
            fs::write(&archive_path, "archived data").unwrap();

            // Simulate restore
            let restored_path = restore_dir.join("restored_data.txt");
            fs::copy(&archive_path, &restored_path).unwrap();

            black_box(restored_path);
        });
    });

    group.finish();
}

/// Benchmark health assessment operations
fn bench_health_assessment(c: &mut Criterion) {
    let mut group = c.benchmark_group("health_assessment");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("basic_health_check", |b| {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();
        let decapod_dir = project_path.join(".decapod");
        fs::create_dir_all(&decapod_dir).unwrap();
        fs::write(decapod_dir.join("store.db"), "").unwrap();

        b.iter(|| {
            // Simulate health checks
            let checks = vec![
                decapod_dir.exists(),
                decapod_dir.join("store.db").exists(),
                decapod_dir.join("constitution.md").exists(),
            ];
            let health_score = checks.iter().filter(|&&x| x).count() as f64 / checks.len() as f64;
            black_box(health_score);
        });
    });

    group.bench_function("risk_assessment", |b| {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();
        let decapod_dir = project_path.join(".decapod");
        fs::create_dir_all(&decapod_dir).unwrap();

        // Create some TODOs to assess
        let todo_count = 10;
        let mut todos = Vec::new();
        for i in 0..todo_count {
            todos.push(format!("TODO {}", i));
        }

        b.iter(|| {
            let mut risk_score = 0.0;

            // Check for missing files
            if !decapod_dir.join("constitution.md").exists() {
                risk_score += 0.3;
            }

            // Check for high TODO count
            if todos.len() > 5 {
                risk_score += (todos.len() - 5) as f64 * 0.05;
            }

            // Check for archive health
            let archive_dir = decapod_dir.join("archives");
            if archive_dir.exists() {
                let archives: Vec<_> = fs::read_dir(&archive_dir)
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .collect();
                if archives.is_empty() {
                    risk_score += 0.1;
                }
            }

            black_box(risk_score.min(1.0));
        });
    });

    group.finish();
}

/// Benchmark concurrent project operations
fn bench_concurrent_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_operations");
    group.measurement_time(Duration::from_secs(10));

    for concurrency in [5, 10, 25].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_init", concurrency),
            concurrency,
            |b, &concurrency| {
                b.iter(|| {
                    let handles: Vec<_> = (0..concurrency)
                        .map(|_| {
                            std::thread::spawn(|| {
                                let temp_dir = TempDir::new().unwrap();
                                let project_path = temp_dir.path();
                                let decapod_dir = project_path.join(".decapod");
                                fs::create_dir_all(&decapod_dir).unwrap();
                                fs::write(decapod_dir.join("store.db"), "").unwrap();
                                black_box(decapod_dir);
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

criterion_group!(
    benches,
    bench_project_initialization,
    bench_validation_compliance,
    bench_archive_restore,
    bench_health_assessment,
    bench_concurrent_operations
);
criterion_main!(benches);
