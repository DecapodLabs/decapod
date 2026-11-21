// Operator-owned criterion: copied outside the agent workspace before launch.
// This fixture is deterministic verification, not empirical efficacy evidence.
#[path = "src/lib.rs"]
mod candidate;

fn main() {
    let greeting: fn() -> &'static str = candidate::greeting;
    assert_eq!(greeting(), "Hello, governed fixture!");
    assert_ne!(greeting(), "Hello, bypass!");
}
