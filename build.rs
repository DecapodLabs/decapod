use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=DECAPOD_CONSTITUTION_DIR");
    println!("cargo:rerun-if-changed=constitution/embedded");

    let out_dir = env::var("OUT_DIR")?;
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")?;

    // Create output directory if it doesn't exist
    fs::create_dir_all(&out_dir)?;

    // Collect all constitution documents into HashMap
    let mut documents = HashMap::new();

    // Process core, specs, and plugins directories
    let dirs = ["core", "specs", "plugins"];
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
                let relative_path = format!(
                    "{}/{}",
                    dir_name,
                    path.file_name().unwrap().to_string_lossy()
                );
                let content = fs::read_to_string(&path)?;
                documents.insert(relative_path, content);
            }
        }
    }

    if documents.is_empty() {
        eprintln!("Warning: No constitution documents found to compile");
        // Create empty blob to prevent build failures
        let empty_blob: HashMap<String, String> = HashMap::new();
        let blob_data = bincode::serialize(&empty_blob)?;
        let blob_path = Path::new(&out_dir).join("constitution.blob");
        fs::write(&blob_path, blob_data)?;
        return Ok(());
    }

    // Create and serialize blob
    let blob_data = bincode::serialize(&documents)?;

    // Write compiled constitution blob
    let blob_path = Path::new(&out_dir).join("constitution.blob");
    fs::write(&blob_path, blob_data)?;

    // Verify blob was created successfully
    let blob_size = fs::metadata(&blob_path)?.len();
    eprintln!(
        "Constitution blob created: {} bytes, {} documents",
        blob_size,
        documents.len()
    );

    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}
