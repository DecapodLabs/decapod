import sys
import os

with open('src/core/docs.rs', 'a') as f:
    f.write('''
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideChecksumStatus {
    MissingOverride,
    Cached,
    Updated,
    Unchanged,
}

pub fn sync_override_checksum(
    repo_root: &std::path::Path,
    force: bool,
) -> Result<OverrideChecksumStatus, crate::core::error::DecapodError> {
    let override_path = repo_root.join(".decapod").join("OVERRIDE.md");

    if !override_path.exists() {
        return Ok(OverrideChecksumStatus::MissingOverride);
    }

    let current_checksum = calculate_sha256(&override_path)?;
    if force {
        cache_checksum(repo_root, &current_checksum)?;
        return Ok(OverrideChecksumStatus::Cached);
    }

    match get_cached_checksum(repo_root) {
        Some(cached_checksum) if cached_checksum == current_checksum => {
            Ok(OverrideChecksumStatus::Unchanged)
        }
        Some(_) => {
            cache_checksum(repo_root, &current_checksum)?;
            Ok(OverrideChecksumStatus::Updated)
        }
        None => {
            cache_checksum(repo_root, &current_checksum)?;
            Ok(OverrideChecksumStatus::Cached)
        }
    }
}

fn calculate_sha256(path: &std::path::Path) -> Result<String, crate::core::error::DecapodError> {
    use sha2::Digest;
    let content = std::fs::read(path).map_err(crate::core::error::DecapodError::IoError)?;
    let hash = sha2::Sha256::digest(&content);
    Ok(format!("{:x}", hash))
}

fn get_cached_checksum(repo_root: &std::path::Path) -> Option<String> {
    let checksum_path = repo_root
        .join(".decapod")
        .join("generated")
        .join("override.checksum");
    std::fs::read_to_string(checksum_path).ok()
}

fn cache_checksum(repo_root: &std::path::Path, checksum: &str) -> Result<(), crate::core::error::DecapodError> {
    let checksum_path = repo_root
        .join(".decapod")
        .join("generated")
        .join("override.checksum");
    if let Some(parent) = checksum_path.parent() {
        std::fs::create_dir_all(parent).map_err(crate::core::error::DecapodError::IoError)?;
    }
    std::fs::write(checksum_path, checksum).map_err(crate::core::error::DecapodError::IoError)
}
''')

# Update lib.rs
with open('src/lib.rs', 'r') as f:
    content = f.read()

content = content.replace('docs_cli::sync_override_checksum', 'docs::sync_override_checksum')
content = content.replace('docs_cli::OverrideChecksumStatus', 'docs::OverrideChecksumStatus')
content = content.replace('    schemas.insert("docs", docs_cli::schema());\\n', '')

# Remove commas in imports if they are isolated
content = content.replace('docs_cli, ', '')
content = content.replace(', docs_cli', '')

with open('src/lib.rs', 'w') as f:
    f.write(content)

# Update core/mod.rs
with open('src/core/mod.rs', 'r') as f:
    lines = f.readlines()
with open('src/core/mod.rs', 'w') as f:
    for line in lines:
        if 'pub mod docs_cli;' not in line:
            f.write(line)

# Update constitution/core.rs
with open('src/constitution/core.rs', 'r') as f:
    lines = f.readlines()
with open('src/constitution/core.rs', 'w') as f:
    for line in lines:
        if 'pub use crate::core::docs_cli;' not in line:
            f.write(line)

# Update cli.rs
with open('src/cli.rs', 'r') as f:
    content = f.read()
content = content.replace('docs_cli, ', '')
content = content.replace(', docs_cli', '')
with open('src/cli.rs', 'w') as f:
    f.write(content)

print("Migration complete")
