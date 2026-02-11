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
            serde_json::from_str(&content).map_err(|e| error::DecapodError::ValidationError(e.to_string()))?
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
        profiles.insert("main".to_string(), ContextProfile {
            budget_tokens: 32000,
            required_files: vec!["OPERATOR.md".to_string(), "SYSTEM.md".to_string()],
            optional_files: vec!["INTEGRATIONS.md".to_string(), "LEDGER.md".to_string()],
        });
        profiles.insert("recovery".to_string(), ContextProfile {
            budget_tokens: 64000,
            required_files: vec!["SYSTEM.md".to_string()],
            optional_files: vec![],
        });
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

    pub fn pack_and_archive(&self, store: &Store, session_path: &Path, summary: &str) -> Result<PathBuf, error::DecapodError> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let archive_dir = self.root.join("memory/archive");
        fs::create_dir_all(&archive_dir).map_err(error::DecapodError::IoError)?;

        let archive_id = format!("arc_{}", now);
        let archive_path = archive_dir.join(format!("{}.md", now));
        
        let content = fs::read_to_string(session_path).map_err(error::DecapodError::IoError)?;
        fs::write(&archive_path, &content).map_err(error::DecapodError::IoError)?;

        // Register in archive index
        archive::initialize_archive_db(&self.root)?;
        archive::register_archive(store, &archive_id, &archive_path, &content, summary)?;

        // MOVE-not-TRIM: Replace original with summary + pointer
        let pointer_content = format!("
[Archived session: {}]
Summary: {}
Archive ID: {}
", archive_path.display(), summary, archive_id);
        fs::write(session_path, pointer_content).map_err(error::DecapodError::IoError)?;

        Ok(archive_path)
    }

    pub fn restore_archive(&self, archive_id: &str, profile_name: &str, current_files: &[PathBuf]) -> Result<String, error::DecapodError> {
        let profile = self.get_profile(profile_name).ok_or_else(|| {
            error::DecapodError::ValidationError(format!("Profile '{}' not found", profile_name))
        })?;

        let archives = archive::list_archives(&Store {
            kind: crate::core::store::StoreKind::User,
            root: self.root.clone(),
        })?; // Simplified Store instantiation
        let entry = archives.iter().find(|a| a.id == archive_id).ok_or_else(|| {
            error::DecapodError::ValidationError(format!("Archive '{}' not found", archive_id))
        })?;

        let full_path = self.root.join(&entry.path);
        let archived_content = fs::read_to_string(full_path).map_err(error::DecapodError::IoError)?;
        
        let current_tokens = self.audit_session(current_files)?;
        let added_tokens = self.estimate_tokens(&archived_content);

        if current_tokens + added_tokens > profile.budget_tokens {
            println!("⚠ RESTORE BLOCKED: budget of {} would be exceeded (total: {})", profile.budget_tokens, current_tokens + added_tokens);
            return Err(error::DecapodError::ValidationError(format!(
                "Restore blocked: budget exceeded ({} + {} > {})",
                current_tokens, added_tokens, profile.budget_tokens
            )));
        }

        println!("✓ Restore approved within '{}' budget ({} tokens added)", profile_name, added_tokens);
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
