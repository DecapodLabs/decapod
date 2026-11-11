//! Synthetic-only benchmark. Run explicitly; no tracked or user datastore is opened.
#[path = "../assets/tools/storage_size_probe.rs"]
mod storage_size_probe;

#[test]
#[ignore = "explicit offline experiment, never part of routine CI"]
fn benchmark_storage_frontier() {
    storage_size_probe::run_probe().expect("all physical layouts must roundtrip identically");
}
