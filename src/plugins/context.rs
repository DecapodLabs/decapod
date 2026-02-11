use crate::archive;
use crate::core::error;
use crate::core::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tiktoken_rs::cl100k_base;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContextProfile {
    pub budget_tokens: usize,
    pub required_files: Vec<String>,
    pub optional_files: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContextConfig {
    pub profiles: HashMap<String, ContextProfile>,
}

pub struct ContextManager {
    root: PathBuf,
    config: ContextConfig,
}

impl ContextManager {
    pub fn new(root: &Path) -> Result<Self, error::DecapodError> {
        let config_path = root.join("CONTEXT.json");
        let config = if config_path.exists() {
            let content = fs::read_to_string(config_path).map_err(error::DecapodError::IoError)?;
            serde_json::from_str(&content)
                .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?
        } else {
            Self::default_config()
        };

        Ok(Self {
            root: root.to_path_buf(),
            config,
        })
    }

    fn default_config() -> ContextConfig {
        let mut profiles = HashMap::new();
        profiles.insert(
            "main".to_string(),
            ContextProfile {
                budget_tokens: 32000,
                required_files: vec!["OPERATOR.md".to_string(), "SYSTEM.md".to_string()],
                optional_files: vec!["INTEGRATIONS.md".to_string(), "LEDGER.md".to_string()],
            },
        );
        profiles.insert(
            "recovery".to_string(),
            ContextProfile {
                budget_tokens: 64000,
                required_files: vec!["SYSTEM.md".to_string()],
                optional_files: vec![],
            },
        );
        ContextConfig { profiles }
    }

    pub fn estimate_tokens(&self, text: &str) -> usize {
        let bpe = cl100k_base().unwrap();
        bpe.encode_with_special_tokens(text).len()
    }

    pub fn audit_session(&self, session_files: &[PathBuf]) -> Result<usize, error::DecapodError> {
        let mut total = 0;
        for path in session_files {
            if path.exists() {
                let content = fs::read_to_string(path).map_err(error::DecapodError::IoError)?;
                total += self.estimate_tokens(&content);
            }
        }
        Ok(total)
    }

    pub fn pack_and_archive(
        &self,
        store: &Store,
        session_path: &Path,
        summary: &str,
    ) -> Result<PathBuf, error::DecapodError> {
        use std::time::{SystemTime, UNIX_EPOCH};

        // Validate inputs before attempting operations
        if summary.trim().is_empty() {
            return Err(error::DecapodError::ContextPackError(
                "Summary cannot be empty".to_string(),
            ));
        }

        // Check if session file exists
        if !session_path.exists() {
            return Err(error::DecapodError::ContextPackError(format!(
                "Session file not found: {}",
                session_path.display()
            )));
        }

        // Check if session is already archived
        match fs::read_to_string(session_path) {
            Ok(content) => {
                if content.contains("[Archived session:") {
                    return Err(error::DecapodError::ContextPackError(format!(
                        "Session file is already archived: {}",
                        session_path.display()
                    )));
                }
            }
            Err(_) => {
                return Err(error::DecapodError::ContextPackError(format!(
                    "Cannot read session file: {}",
                    session_path.display()
                )));
            }
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let archive_dir = self.root.join("memory/archive");

        // Create archive directory with graceful error
        if let Err(e) = fs::create_dir_all(&archive_dir) {
            return Err(error::DecapodError::ContextPackError(format!(
                "Failed to create archive directory '{}': {}",
                archive_dir.display(),
                e
            )));
        }

        let archive_id = format!("arc_{}", now);
        let archive_path = archive_dir.join(format!("{}.md", now));

        // Read session content with context
        let content = match fs::read_to_string(session_path) {
            Ok(c) => c,
            Err(e) => {
                return Err(error::DecapodError::ContextPackError(format!(
                    "Failed to read session file '{}': {}",
                    session_path.display(),
                    e
                )));
            }
        };

        // Write to archive with context
        if let Err(e) = fs::write(&archive_path, &content) {
            return Err(error::DecapodError::ContextPackError(format!(
                "Failed to write archive file '{}': {}",
                archive_path.display(),
                e
            )));
        }

        // Register in archive index
        archive::initialize_archive_db(&self.root)?;
        archive::register_archive(store, &archive_id, &archive_path, &content, summary)?;

        // MOVE-not-TRIM: Replace original with summary + pointer
        let pointer_content = format!(
            "
[Archived session: {}]
Summary: {}
Archive ID: {}
",
            archive_path.display(),
            summary,
            archive_id
        );

        if let Err(e) = fs::write(session_path, pointer_content) {
            // Archive was created but original file update failed
            return Err(error::DecapodError::ContextPackError(format!(
                "Archive created at '{}' but failed to update original file '{}': {}. Manual cleanup required.",
                archive_path.display(),
                session_path.display(),
                e
            )));
        }

        Ok(archive_path)
    }

    pub fn restore_archive(
        &self,
        archive_id: &str,
        profile_name: &str,
        current_files: &[PathBuf],
    ) -> Result<String, error::DecapodError> {
        let profile = self.get_profile(profile_name).ok_or_else(|| {
            error::DecapodError::ValidationError(format!("Profile '{}' not found", profile_name))
        })?;

        let archives = archive::list_archives(&Store {
            kind: crate::core::store::StoreKind::User,
            root: self.root.clone(),
        })?; // Simplified Store instantiation
        let entry = archives
            .iter()
            .find(|a| a.id == archive_id)
            .ok_or_else(|| {
                error::DecapodError::ValidationError(format!("Archive '{}' not found", archive_id))
            })?;

        let full_path = self.root.join(&entry.path);
        let archived_content =
            fs::read_to_string(full_path).map_err(error::DecapodError::IoError)?;

        let current_tokens = self.audit_session(current_files)?;
        let added_tokens = self.estimate_tokens(&archived_content);

        if current_tokens + added_tokens > profile.budget_tokens {
            println!(
                "⚠ RESTORE BLOCKED: budget of {} would be exceeded (total: {})",
                profile.budget_tokens,
                current_tokens + added_tokens
            );
            return Err(error::DecapodError::ValidationError(format!(
                "Restore blocked: budget exceeded ({} + {} > {})",
                current_tokens, added_tokens, profile.budget_tokens
            )));
        }

        println!(
            "✓ Restore approved within '{}' budget ({} tokens added)",
            profile_name, added_tokens
        );
        Ok(archived_content)
    }

    pub fn get_profile(&self, name: &str) -> Option<&ContextProfile> {
        self.config.profiles.get(name)
    }
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "context",
        "version": "0.1.0",
        "description": "Agent context and token budget management",
        "commands": [
            { "name": "audit", "parameters": ["profile", "files"] },
            { "name": "pack", "parameters": ["path", "summary"] },
            { "name": "restore", "parameters": ["archive_id", "profile"] }
        ],
        "storage": ["CONTEXT.json", "memory/archive/"]
    })
}
