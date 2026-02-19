use sha2::{Digest, Sha256};
use std::path::Path;

pub struct StateCommitInput {
    pub base_sha: String,
    pub head_sha: String,
    pub ignore_policy_hash: String,
}

#[derive(Clone)]
pub struct StateCommitEntry {
    pub path: String,
    pub kind: u8, // 0 = file, 1 = symlink
    pub mode_exec: bool,
    pub content_hash: String,
    pub size: u64,
}

pub struct StateCommitOutput {
    pub scope_record_bytes: Vec<u8>,
    pub scope_record_hash: String,
    pub state_commit_root: String,
    pub entries: Vec<StateCommitEntry>,
}

pub fn run_git(repo_root: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn git_show(repo_root: &Path, sha: &str, path: &str) -> Result<String, String> {
    run_git(repo_root, &["show", &format!("{}:{}", sha, path)])
}

pub fn git_ls_tree(repo_root: &Path, sha: &str, path: &str) -> Result<String, String> {
    run_git(repo_root, &["ls-tree", "-r", sha, "--", path])
}

pub fn get_path_set(
    repo_root: &Path,
    base_sha: &str,
    head_sha: &str,
) -> Result<Vec<String>, String> {
    // Use three-dot syntax for merge-base diff (common ancestor...HEAD)
    // or space-separated for explicit range
    let output = run_git(
        repo_root,
        &[
            "-c",
            "core.quotepath=false",
            "diff",
            "--name-only",
            base_sha,
            head_sha,
        ],
    )?;

    Ok(output
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

fn parse_ls_tree_line(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.len() != 2 {
        return None;
    }
    let mode_type_oid: Vec<&str> = parts[0].split_whitespace().collect();
    if mode_type_oid.len() != 3 {
        return None;
    }
    let mode = mode_type_oid[0].to_string();
    let oid = mode_type_oid[2].to_string();
    Some((mode, oid))
}

pub fn get_entry(repo_root: &Path, head_sha: &str, path: &str) -> Result<StateCommitEntry, String> {
    let line = git_ls_repo(repo_root, head_sha, path)?;
    let (mode, _oid) = parse_ls_tree_line(&line).ok_or("failed to parse ls-tree")?;

    let kind = if mode == "120000" { 1 } else { 0 };
    let mode_exec = mode == "100755";

    let content = git_show(repo_root, head_sha, path)?;
    let content_bytes = content.as_bytes();
    let size = content_bytes.len() as u64;

    let mut hasher = Sha256::new();
    hasher.update(content_bytes);
    let content_hash = format!("{:x}", hasher.finalize());

    Ok(StateCommitEntry {
        path: path.to_string(),
        kind,
        mode_exec,
        content_hash,
        size,
    })
}

pub fn git_ls_repo(repo_root: &Path, sha: &str, path: &str) -> Result<String, String> {
    run_git(repo_root, &["ls-tree", "-r", sha, "--", path])
}

fn encode_uint(v: u64) -> Vec<u8> {
    if v < 24 {
        vec![v as u8]
    } else if v < 256 {
        vec![0x18, v as u8]
    } else if v < 65536 {
        vec![0x19, (v >> 8) as u8, v as u8]
    } else {
        panic!("uint too large")
    }
}

fn encode_string(s: &str) -> Vec<u8> {
    let data = s.as_bytes();
    let length = data.len();
    if length < 24 {
        let mut r = vec![0x60 + length as u8];
        r.extend_from_slice(data);
        r
    } else if length < 256 {
        let mut r = vec![0x78, length as u8];
        r.extend_from_slice(data);
        r
    } else {
        panic!("string too long")
    }
}

fn encode_bool(b: bool) -> Vec<u8> {
    vec![if b { 0xf5 } else { 0xf4 }]
}

fn encode_array(arr: &[Vec<u8>]) -> Vec<u8> {
    let length = arr.len();
    let header = if length < 24 {
        vec![0x80 + length as u8]
    } else if length < 256 {
        vec![0x98, length as u8]
    } else {
        panic!("array too long");
    };
    let mut r = header;
    for a in arr {
        r.extend_from_slice(a);
    }
    r
}

fn encode_map(mappings: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let length = mappings.len();
    let header = if length < 24 {
        vec![0xA0 + length as u8]
    } else if length < 256 {
        vec![0xB8, length as u8]
    } else {
        panic!("map too large");
    };
    let mut r = header;
    for (k, v) in mappings {
        r.extend_from_slice(&encode_uint(*k as u64));
        r.extend_from_slice(v);
    }
    r
}

pub fn compute_scope_record(
    entries: &[StateCommitEntry],
    base_sha: &str,
    head_sha: &str,
    ignore_policy_hash: &str,
) -> Vec<u8> {
    let mut sorted_entries = entries.to_vec();
    sorted_entries.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));

    let mut entry_arrays = Vec::new();
    for e in &sorted_entries {
        entry_arrays.push(encode_array(&[
            encode_string(&e.path),
            encode_uint(e.kind as u64),
            encode_bool(e.mode_exec),
            encode_string(&e.content_hash),
            encode_uint(e.size),
        ]));
    }

    let entries_bytes = encode_array(&entry_arrays);

    encode_map(&[
        (1, encode_string("state_commit.v1")),
        (2, encode_string(base_sha)),
        (3, encode_string(head_sha)),
        (4, encode_uint(1)),
        (5, encode_string(ignore_policy_hash)),
        (6, entries_bytes),
    ])
}

pub fn compute_merkle_root(entries: &[StateCommitEntry]) -> String {
    let mut sorted_entries = entries.to_vec();
    sorted_entries.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));

    let mut leaf_hashes = Vec::new();
    for e in &sorted_entries {
        let leaf = encode_array(&[
            encode_string(&e.path),
            encode_uint(e.kind as u64),
            encode_bool(e.mode_exec),
            encode_string(&e.content_hash),
        ]);
        let mut hasher = Sha256::new();
        hasher.update(leaf);
        leaf_hashes.push(format!("{:x}", hasher.finalize()));
    }

    while leaf_hashes.len() > 1 {
        if leaf_hashes.len() % 2 == 1 {
            leaf_hashes.push(leaf_hashes.last().unwrap().clone());
        }
        let mut new_level = Vec::new();
        for i in (0..leaf_hashes.len()).step_by(2) {
            let combined = format!("{}{}", leaf_hashes[i], leaf_hashes[i + 1]);
            let mut hasher = Sha256::new();
            hasher.update(combined.as_bytes());
            new_level.push(format!("{:x}", hasher.finalize()));
        }
        leaf_hashes = new_level;
    }

    leaf_hashes.first().cloned().unwrap_or_else(|| {
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()
    })
}

pub fn prove(input: &StateCommitInput, repo_root: &Path) -> Result<StateCommitOutput, String> {
    let paths = get_path_set(repo_root, &input.base_sha, &input.head_sha)?;

    let mut entries = Vec::new();
    for path in &paths {
        let entry = get_entry(repo_root, &input.head_sha, path)?;
        entries.push(entry);
    }

    let scope_record_bytes = compute_scope_record(
        &entries,
        &input.base_sha,
        &input.head_sha,
        &input.ignore_policy_hash,
    );

    let mut hasher = Sha256::new();
    hasher.update(&scope_record_bytes);
    let scope_record_hash = format!("{:x}", hasher.finalize());

    let state_commit_root = compute_merkle_root(&entries);

    Ok(StateCommitOutput {
        scope_record_bytes,
        scope_record_hash,
        state_commit_root,
        entries,
    })
}

pub fn verify(scope_record_bytes: &[u8], expected_root: &str) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(scope_record_bytes);
    let actual_hash = format!("{:x}", hasher.finalize());

    if actual_hash != expected_root {
        Err(format!(
            "STATE_COMMIT verification failed: expected {}, got {}",
            expected_root, actual_hash
        ))
    } else {
        Ok(actual_hash)
    }
}
