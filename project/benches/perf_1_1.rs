use criterion::{Criterion, criterion_group, criterion_main};

fn perf_1_1(c: &mut Criterion) {
    c.bench_function("perf_1_1_noop", |b| b.iter(|| 1 + 1));
}

criterion_group!(benches, perf_1_1);
criterion_main!(benches);
