//! CLI wrapper for the shared exact release-projection verifier.

use decapod::{core, plugins};
use std::path::Path;

// Compile the same implementation here so this established policy target
// continues to execute its complete private-helper regression suite.
#[allow(dead_code)]
#[path = "../../src/decapod/core/release_projection_drift.rs"]
mod implementation;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 9
        || args[1] != "--repo"
        || args[3] != "--event"
        || args[5] != "--base"
        || args[7] != "--head"
    {
        eprintln!(
            "usage: decapod-release-projection-drift --repo PATH --event EVENT --base SHA --head SHA"
        );
        std::process::exit(2);
    }
    if let Err(error) = implementation::verify(Path::new(&args[2]), &args[4], &args[6], &args[8]) {
        eprintln!("Release projection drift rejected: {error}");
        std::process::exit(1);
    }
    println!("Exact release-only projections verified after validation");
}
