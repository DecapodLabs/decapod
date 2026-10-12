//! Dependency-free CLI for the shared immutable release classifier.

use std::path::Path;

// The runtime also uses validation_change_base; this small CLI exposes only classification.
#[allow(dead_code)]
#[path = "../../src/decapod/core/release_metadata.rs"]
pub mod implementation;
pub use implementation::classify;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 9
        || args[1] != "--repo"
        || args[3] != "--event"
        || args[5] != "--base"
        || args[7] != "--head"
    {
        eprintln!(
            "usage: decapod-release-metadata --repo PATH --event EVENT --base SHA --head SHA"
        );
        std::process::exit(2);
    }
    let release_only = match classify(Path::new(&args[2]), &args[4], &args[6], &args[8]) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Release metadata could not be proven; normal CI is required: {error}");
            false
        }
    };
    println!("release_only={release_only}");
}
