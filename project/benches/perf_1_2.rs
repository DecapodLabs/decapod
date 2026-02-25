use criterion::{Criterion, criterion_group, criterion_main};

fn perf_1_2(c: &mut Criterion) {
    c.bench_function("perf_1_2_noop", |b| b.iter(|| 2 + 2));
}

criterion_group!(benches, perf_1_2);
criterion_main!(benches);
