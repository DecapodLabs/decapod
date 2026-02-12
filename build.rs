use std::env;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=DECAPOD_CONSTITUTION_DIR");
    println!("cargo:rerun-if-changed=constitution/embedded");
    println!("cargo:rerun-if-changed=constitution/templates");
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR")?;

    // Index all constitution documents and set up watch dependencies
    let dirs = ["core", "specs", "plugins"];
    let mut doc_count = 0;

    for dir_name in &dirs {
        let dir_path = Path::new(&manifest_dir)
            .join("constitution/embedded")
            .join(dir_name);

        if !dir_path.exists() {
            eprintln!("Warning: Directory {} does not exist", dir_path.display());
            continue;
        }

        for entry in fs::read_dir(&dir_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                // Set up individual file watch for fine-grained rebuilds
                println!("cargo:rerun-if-changed={}", path.display());
                doc_count += 1;
            }
        }
    }

    eprintln!(
        "Constitution index created: {} documents tracked",
        doc_count
    );

    Ok(())
}
