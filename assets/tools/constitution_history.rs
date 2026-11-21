//! Reproducible, read-only chronology of source predicates in accepted history.
//! Commit/tag timestamps are repository metadata, not observed publication dates.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::{path::Path, process::Command};

#[derive(Clone, Copy)]
struct Mechanism {
    id: &'static str,
    paths: &'static [&'static str],
    literals: &'static [&'static str],
}

const ASSETS: &[&str] = &[
    "src/assets.rs",
    "src/core/assets.rs",
    "src/decapod/core/assets.rs",
];
const DOCS: &[&str] = &["src/core/docs.rs", "src/decapod/core/docs.rs"];
const MECHANISMS: &[Mechanism] = &[
    Mechanism {
        id: "embedded_markdown_constitution",
        paths: ASSETS,
        literals: &["CONSTITUTION_CORE_DECAPOD", "include_str!"],
    },
    Mechanism {
        id: "generated_agent_entrypoint_template",
        paths: ASSETS,
        literals: &["TEMPLATE_AGENTS", "AGENTS.md", "include_str!"],
    },
    Mechanism {
        id: "repository_override_resolution",
        paths: ASSETS,
        literals: &["get_override_doc", "OVERRIDE.md", "get_merged_doc"],
    },
    Mechanism {
        id: "section_fragment_with_source_hash",
        paths: DOCS,
        literals: &["pub fn get_fragment", "Sha256", "excerpt"],
    },
    Mechanism {
        id: "task_query_scoped_selection",
        paths: DOCS,
        literals: &[
            "pub fn resolve_scoped_fragments",
            "touched_paths",
            "intent_tags",
        ],
    },
    Mechanism {
        id: "global_json_node_corpus",
        paths: &[
            "assets/constitution.json",
            "constitution.json",
            "constitution/constitution.json",
        ],
        literals: &["\"nodes\"", "\"core/DECAPOD\""],
    },
    Mechanism {
        id: "compressed_json_embedding",
        paths: &[
            "build/compress_constitution.rs",
            "assets/build/compress_constitution.rs",
        ],
        literals: &["GzEncoder", "serde_json::to_string(&node)"],
    },
    Mechanism {
        id: "state_bound_context_capsule",
        paths: &[
            "src/core/context_capsule.rs",
            "src/decapod/core/context_capsule.rs",
        ],
        literals: &["capsule_hash", "task_id", "pub fn query_embedded_capsule"],
    },
    Mechanism {
        id: "override_source_provenance",
        paths: ASSETS,
        literals: &[
            "ResolvedOverrideEvidence",
            "source_hash",
            "body_hash",
            "precedence",
        ],
    },
    Mechanism {
        id: "native_workspace_refusal",
        paths: &["src/core/workspace.rs", "src/decapod/core/workspace.rs"],
        literals: &[
            "git.is_protected",
            "BlockerKind::ProtectedBranch",
            "can_work",
        ],
    },
    Mechanism {
        id: "native_proof_gate",
        paths: &["src/core/workunit.rs", "src/decapod/core/workunit.rs"],
        literals: &["validate_verified_manifest", "proof"],
    },
];

#[derive(Debug, Serialize)]
struct Fact {
    mechanism: String,
    commit: Option<String>,
    author_date: Option<String>,
    committer_date: Option<String>,
    source_path: Option<String>,
    source_blob: Option<String>,
    predicate: Vec<String>,
    first_containing_version_tag: Option<String>,
    tag_date: Option<String>,
    github_release_publication: Option<PublishedRelease>,
    evidence_class: &'static str,
    provenance: Option<String>,
}

#[derive(Debug, Serialize)]
struct Chronology {
    schema_version: &'static str,
    repository: &'static str,
    accepted_ref: String,
    ordering: &'static str,
    limits: &'static str,
    public_evidence_hash: Option<String>,
    public_evidence_class: Option<String>,
    facts: Vec<Fact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishedRelease {
    published_at: String,
    url: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicEvidence {
    schema_version: String,
    source: String,
    observed_at: String,
    evidence_class: String,
    tags: BTreeMap<String, String>,
    releases: BTreeMap<String, PublishedRelease>,
}

impl PublicEvidence {
    fn validate(&self, root: &Path) -> Result<(), String> {
        if self.schema_version != "decapod.public-history-input.v1"
            || self.source != "https://github.com/DecapodLabs/decapod"
            || !matches!(
                self.evidence_class.as_str(),
                "operator_captured_public_forge" | "synthetic_fixture"
            )
        {
            return Err("unsupported public-history input/provenance".into());
        }
        for (tag, object) in &self.tags {
            if !version_tag(tag)
                || object.len() != 40
                || !object.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("invalid public tag identity".into());
            }
            if git(root, &["rev-parse", &format!("refs/tags/{tag}")])? != *object {
                return Err(format!(
                    "public tag object differs: {tag}; preserve history and fetch/reconcile the observed public object"
                ));
            }
        }
        for (tag, release) in &self.releases {
            if !self.tags.contains_key(tag)
                || release.url
                    != format!("https://github.com/DecapodLabs/decapod/releases/tag/{tag}")
                || time::OffsetDateTime::parse(
                    &release.published_at,
                    &time::format_description::well_known::Rfc3339,
                )
                .is_err()
            {
                return Err("invalid public release identity or timestamp".into());
            }
        }
        Ok(())
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args([
            "--no-replace-objects",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    String::from_utf8(out.stdout)
        .map(|s| s.trim_end().into())
        .map_err(|e| e.to_string())
}

fn matching_source(
    root: &Path,
    revision: &str,
    mechanism: Mechanism,
) -> Result<Option<(String, String)>, String> {
    let mut args = vec!["ls-tree", "-r", "--name-only", revision, "--"];
    args.extend(mechanism.paths);
    for path in git(root, &args)?.lines() {
        let object = format!("{revision}:{path}");
        let content = git(root, &["show", &object])?;
        if mechanism
            .literals
            .iter()
            .all(|literal| content.contains(literal))
        {
            return Ok(Some((path.into(), git(root, &["rev-parse", &object])?)));
        }
    }
    Ok(None)
}

fn version_tag(tag: &str) -> bool {
    let Some(v) = tag.strip_prefix('v') else {
        return false;
    };
    let parts: Vec<_> = v.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

fn generate(
    root: &Path,
    reference: &str,
    public: Option<&PublicEvidence>,
) -> Result<Chronology, String> {
    if reference.len() != 40 || !reference.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("--ref requires a complete immutable commit SHA".into());
    }
    if git(root, &["rev-parse", "--is-shallow-repository"])? != "false" {
        return Err(
            "full history required; shallow history cannot establish first source evidence".into(),
        );
    }
    if git(root, &["cat-file", "-t", reference])? != "commit" {
        return Err("--ref must identify a commit".into());
    }
    if let Some(public) = public {
        public.validate(root)?;
    }
    let mut tags: Vec<_> = public
        .map(|p| p.releases.keys().cloned().collect())
        .unwrap_or_default();
    if let Some(public) = public {
        tags.sort_by(|a, b| {
            public.releases[a]
                .published_at
                .cmp(&public.releases[b].published_at)
                .then_with(|| a.cmp(b))
        });
    }
    let mut facts = Vec::new();
    for mechanism in MECHANISMS {
        let mut args = vec!["log", "--reverse", "--topo-order", "--format=%H", reference];
        if let Some(public) = public {
            args.extend(public.tags.values().map(String::as_str));
        }
        args.push("--");
        args.extend(mechanism.paths);
        let mut found = None;
        for commit in git(root, &args)?.lines() {
            if let Some((path, blob)) = matching_source(root, commit, *mechanism)? {
                found = Some((commit.to_string(), path, blob));
                break;
            }
        }
        let mut fact = Fact {
            mechanism: mechanism.id.into(),
            commit: None,
            author_date: None,
            committer_date: None,
            source_path: None,
            source_blob: None,
            predicate: mechanism.literals.iter().map(|s| (*s).into()).collect(),
            first_containing_version_tag: None,
            tag_date: None,
            github_release_publication: None,
            evidence_class: "repository_source",
            provenance: None,
        };
        if let Some((commit, path, blob)) = found {
            let dates = git(root, &["show", "-s", "--format=%aI%n%cI", &commit])?;
            let mut dates = dates.lines();
            fact.author_date = dates.next().map(str::to_string);
            fact.committer_date = dates.next().map(str::to_string);
            fact.provenance = Some(format!(
                "https://github.com/DecapodLabs/decapod/blob/{commit}/{path}"
            ));
            for tag in &tags {
                if matching_source(root, tag, *mechanism)?.is_some() {
                    fact.first_containing_version_tag = Some(tag.clone());
                    fact.tag_date = Some(git(
                        root,
                        &[
                            "for-each-ref",
                            "--format=%(creatordate:iso-strict)",
                            &format!("refs/tags/{tag}"),
                        ],
                    )?);
                    fact.github_release_publication =
                        public.and_then(|p| p.releases.get(tag)).cloned();
                    break;
                }
            }
            fact.commit = Some(commit);
            fact.source_path = Some(path);
            fact.source_blob = Some(blob);
        }
        facts.push(fact);
    }
    Ok(Chronology {
        schema_version: "decapod.constitution-history.v1",
        repository: "DecapodLabs/decapod",
        accepted_ref: reference.into(),
        ordering: "first matching commit in reverse topological accepted/public-tag history; containing releases by observed original publication timestamp",
        limits: "Source predicates establish inspected implementation bytes, not execution, design inception, priority or effectiveness. Commit/tag dates are mutable repository metadata. Optional forge metadata is an explicitly captured observation, not authenticated by this generator. No arbitrary local refs or private artifacts are read.",
        public_evidence_hash: public
            .map(|p| serde_json::to_vec(p).map(|b| format!("{:x}", Sha256::digest(b))))
            .transpose()
            .map_err(|e| e.to_string())?,
        public_evidence_class: public.map(|p| p.evidence_class.clone()),
        facts,
    })
}

#[allow(dead_code)] // Also compiled into the existing Constitution regression target.
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !matches!(args.len(), 2 | 4)
        || args[0] != "--ref"
        || (args.len() == 4 && args[2] != "--public-evidence")
    {
        return Err("usage: decapod-constitution-history --ref <full-accepted-SHA> [--public-evidence <captured-json>]".into());
    }
    let public = args
        .get(3)
        .map(|path| {
            std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    serde_json::from_slice::<PublicEvidence>(&bytes).map_err(|e| e.to_string())
                })
        })
        .transpose()?;
    let result = generate(Path::new("."), &args[1], public.as_ref())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_mutable_refs_before_observing_history() {
        assert!(
            generate(Path::new("."), "master", None)
                .unwrap_err()
                .contains("immutable")
        );
    }
    #[test]
    fn accepts_only_plain_version_tags() {
        assert!(version_tag("v0.148.9"));
        for t in ["archive/private", "v1.2.3-experiment", "v1.2", "v.2.3"] {
            assert!(!version_tag(t));
        }
    }

    #[test]
    fn chronology_is_reproducible_and_excludes_unaccepted_private_history() {
        let root = tempfile::tempdir().unwrap();
        git(root.path(), &["init", "-b", "master"]).unwrap();
        git(root.path(), &["config", "user.name", "Fixture"]).unwrap();
        git(
            root.path(),
            &["config", "user.email", "fixture@example.invalid"],
        )
        .unwrap();
        git(root.path(), &["config", "commit.gpgSign", "false"]).unwrap();
        std::fs::create_dir_all(root.path().join("src/core")).unwrap();
        std::fs::write(
            root.path().join("src/core/assets.rs"),
            "CONSTITUTION_CORE_DECAPOD include_str! fixture",
        )
        .unwrap();
        git(root.path(), &["add", "."]).unwrap();
        git(root.path(), &["commit", "-m", "public fixture"]).unwrap();
        let accepted = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
        git(root.path(), &["tag", "v0.0.1"]).unwrap();
        git(root.path(), &["checkout", "--orphan", "private-prototype"]).unwrap();
        std::fs::write(
            root.path().join("src/core/assets.rs"),
            "get_override_doc OVERRIDE.md get_merged_doc",
        )
        .unwrap();
        git(root.path(), &["add", "."]).unwrap();
        git(root.path(), &["commit", "-m", "unaccepted private fixture"]).unwrap();
        let public = PublicEvidence {
            schema_version: "decapod.public-history-input.v1".into(),
            source: "https://github.com/DecapodLabs/decapod".into(),
            observed_at: "2026-01-01T00:00:00Z".into(),
            evidence_class: "synthetic_fixture".into(),
            tags: BTreeMap::from([("v0.0.1".into(), accepted.clone())]),
            releases: BTreeMap::from([(
                "v0.0.1".into(),
                PublishedRelease {
                    published_at: "2026-01-01T00:00:00Z".into(),
                    url: "https://github.com/DecapodLabs/decapod/releases/tag/v0.0.1".into(),
                },
            )]),
        };
        let first = generate(root.path(), &accepted, Some(&public)).unwrap();
        let second = generate(root.path(), &accepted, Some(&public)).unwrap();
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
        let embedding = first
            .facts
            .iter()
            .find(|f| f.mechanism == "embedded_markdown_constitution")
            .unwrap();
        assert_eq!(embedding.commit.as_deref(), Some(accepted.as_str()));
        assert_eq!(
            embedding.first_containing_version_tag.as_deref(),
            Some("v0.0.1")
        );
        assert_eq!(
            embedding
                .github_release_publication
                .as_ref()
                .unwrap()
                .published_at,
            "2026-01-01T00:00:00Z"
        );
        assert_eq!(
            first.public_evidence_class.as_deref(),
            Some("synthetic_fixture")
        );
        let private = first
            .facts
            .iter()
            .find(|f| f.mechanism == "repository_override_resolution")
            .unwrap();
        assert!(private.commit.is_none());
    }
}
