use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContextCapsuleSource {
    pub path: String,
    pub section: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContextCapsuleSnippet {
    pub source_path: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeterministicContextCapsule {
    pub topic: String,
    pub scope: String,
    pub task_id: Option<String>,
    pub workunit_id: Option<String>,
    pub sources: Vec<ContextCapsuleSource>,
    pub snippets: Vec<ContextCapsuleSnippet>,
    pub capsule_hash: String,
}

impl DeterministicContextCapsule {
    fn canonicalized_without_hash(&self) -> CanonicalCapsule {
        let mut sources = self.sources.clone();
        sources.sort();
        sources.dedup();

        let mut snippets = self.snippets.clone();
        snippets.sort();
        snippets.dedup();

        CanonicalCapsule {
            topic: self.topic.clone(),
            scope: self.scope.clone(),
            task_id: self.task_id.clone(),
            workunit_id: self.workunit_id.clone(),
            sources,
            snippets,
        }
    }

    pub fn canonical_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.canonicalized_without_hash())
    }

    pub fn computed_hash_hex(&self) -> Result<String, serde_json::Error> {
        let bytes = self.canonical_json_bytes()?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub fn with_recomputed_hash(&self) -> Result<Self, serde_json::Error> {
        let mut out = self.clone();
        out.capsule_hash = out.computed_hash_hex()?;
        Ok(out)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CanonicalCapsule {
    topic: String,
    scope: String,
    task_id: Option<String>,
    workunit_id: Option<String>,
    sources: Vec<ContextCapsuleSource>,
    snippets: Vec<ContextCapsuleSnippet>,
}
