use criterion::{Criterion, criterion_group, criterion_main};

fn perf_1_3(c: &mut Criterion) {
    c.bench_function("perf_1_3_noop", |b| b.iter(|| 3 + 3));
}

criterion_group!(benches, perf_1_3);
criterion_main!(benches);
