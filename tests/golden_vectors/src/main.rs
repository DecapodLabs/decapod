use std::path::PathBuf;
use std::process::Command;

const FIXTURE_REPO: &str = "/home/arx/projects/decapod/tests/fixtures/state_commit_repo";
const GOLDEN_DIR: &str = "/home/arx/projects/decapod/tests/golden/state_commit/v1";
const BASE_SHA: &str = "6eb442a";
const HEAD_SHA: &str = "58b7c5f";
const SPEC_VERSION: &str = "v1";

fn run_git(args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(FIXTURE_REPO)
        .output()
        .expect("git failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn git_show(sha: &str, path: &str) -> String {
    run_git(&["show", &format!("{}:{}", sha, path)])
}

fn git_ls_tree(sha: &str, path: &str) -> String {
    run_git(&["ls-tree", "-r", sha, "--", path])
}

fn sha256(data: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn get_path_set() -> Vec<String> {
    let output = run_git(&["-c", "core.quotepath=false", "diff", "--name-only", BASE_SHA, HEAD_SHA]);
    output.lines().map(|s| s.to_string()).filter(|s| !s.is_empty() && !s.starts_with("golden/")).collect()
}

#[derive(Debug, Clone)]
struct Entry {
    path: String,
    kind: u8,
    mode_exec: bool,
    content_hash: String,
    size: u64,
}

fn parse_ls_tree_line(line: &str) -> (String, String, String, String) {
    let parts: Vec<&str> = line.split('\t').collect();
    let mode_type_oid: Vec<&str> = parts[0].split_whitespace().collect();
    let mode = mode_type_oid[0].to_string();
    let obj_type = mode_type_oid[1].to_string();
    let oid = mode_type_oid[2].to_string();
    let path = parts[1].to_string();
    (mode, obj_type, oid, path)
}

fn get_entry(path: &str) -> Entry {
    let line = git_ls_tree(HEAD_SHA, path);
    let (mode, _obj_type, _oid, _path) = parse_ls_tree_line(&line);
    
    let kind = if mode == "120000" { 1 } else { 0 };
    let mode_exec = mode == "100755";
    
    let content = git_show(HEAD_SHA, path);
    let content_bytes = content.as_bytes();
    let size = content_bytes.len() as u64;
    
    Entry {
        path: path.to_string(),
        kind,
        mode_exec,
        content_hash: sha256(content_bytes),
        size,
    }
}

fn encode_uint(v: u64) -> Vec<u8> {
    if v < 24 { vec![v as u8] }
    else if v < 256 { vec![0x18, v as u8] }
    else if v < 65536 { vec![0x19, (v >> 8) as u8, v as u8] }
    else { panic!("uint too large") }
}

fn encode_string(s: &str) -> Vec<u8> {
    let data = s.as_bytes();
    let length = data.len();
    let mut r = if length < 24 { vec![0x60 + length as u8] }
    else if length < 256 { vec![0x78, length as u8] }
    else { panic!("string too long") };
    r.extend_from_slice(data);
    r
}

fn encode_bool(b: bool) -> Vec<u8> { vec![if b { 0xf5 } else { 0xf4 }] }

fn encode_array(arr: &[Vec<u8>]) -> Vec<u8> {
    let length = arr.len();
    let header = if length < 24 { vec![0x80 + length as u8] }
    else if length < 256 { vec![0x98, length as u8] }
    else { panic!("array too long") };
    let mut r = header;
    for a in arr { r.extend_from_slice(a); }
    r
}

fn encode_map(mappings: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let length = mappings.len();
    let header = if length < 24 { vec![0xA0 + length as u8] }
    else if length < 256 { vec![0xB8, length as u8] }
    else { panic!("map too large") };
    let mut r = header;
    for (k, v) in mappings {
        r.extend_from_slice(&encode_uint(*k as u64));
        r.extend_from_slice(v);
    }
    r
}

fn build_canonical_scope_record(entries: &[Entry]) -> Vec<u8> {
    let mut sorted_entries = entries.to_vec();
    sorted_entries.sort_by(|a, b| a.path.as_bytes().cmp(&b.path.as_bytes()));
    
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
        (2, encode_string(BASE_SHA)),
        (3, encode_string(HEAD_SHA)),
        (4, encode_uint(1)),
        (5, encode_string("da39a3ee5e6b4b0d3255bfef95601890afd80709")),
        (6, entries_bytes),
    ])
}

fn compute_merkle_root(entries: &[Entry]) -> String {
    let mut sorted_entries = entries.to_vec();
    sorted_entries.sort_by(|a, b| a.path.as_bytes().cmp(&b.path.as_bytes()));
    
    let mut leaf_hashes = Vec::new();
    for e in &sorted_entries {
        let leaf = encode_array(&[
            encode_string(&e.path),
            encode_uint(e.kind as u64),
            encode_bool(e.mode_exec),
            encode_string(&e.content_hash),
        ]);
        leaf_hashes.push(sha256(&leaf));
    }
    
    if leaf_hashes.is_empty() {
        return "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string();
    }
    
    while leaf_hashes.len() > 1 {
        if leaf_hashes.len() % 2 == 1 {
            leaf_hashes.push(leaf_hashes.last().unwrap().clone());
        }
        let mut new_level = Vec::new();
        for i in (0..leaf_hashes.len()).step_by(2) {
            let combined = format!("{}{}", leaf_hashes[i], leaf_hashes[i+1]);
            new_level.push(sha256(combined.as_bytes()));
        }
        leaf_hashes = new_level;
    }
    
    leaf_hashes.first().unwrap().clone()
}

trait Hex {
    fn hex(&self) -> String;
}

impl Hex for Vec<u8> {
    fn hex(&self) -> String {
        self.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

fn main() {
    println!("STATE_COMMIT SPEC_VERSION: {}", SPEC_VERSION);
    println!("Fixture repo: {}", FIXTURE_REPO);
    println!("base_sha: {}", BASE_SHA);
    println!("head_sha: {}", HEAD_SHA);
    println!();
    
    let paths = get_path_set();
    println!("Path set ({} files):", paths.len());
    for p in &paths {
        println!("  {}", p);
    }
    println!();
    
    let mut entries = Vec::new();
    for path in &paths {
        let entry = get_entry(path);
        println!("Entry: {} kind={} exec={} hash={}...", entry.path, entry.kind, entry.mode_exec, &entry.content_hash[..16]);
        entries.push(entry);
    }
    println!();
    
    let cbor_bytes = build_canonical_scope_record(&entries);
    let scope_record_hash = sha256(&cbor_bytes);
    let merkle_root = compute_merkle_root(&entries);
    
    println!("=== GOLDEN OUTPUTS (SPEC_VERSION: {}) ===", SPEC_VERSION);
    println!("scope_record.cbor ({} bytes)", cbor_bytes.len());
    println!("scope_record_hash: {}", scope_record_hash);
    println!("state_commit_root: {}", merkle_root);
    println!();
    
    let out_dir = PathBuf::from(GOLDEN_DIR);
    std::fs::create_dir_all(&out_dir).unwrap();
    
    std::fs::write(out_dir.join("scope_record.cbor"), &cbor_bytes).unwrap();
    std::fs::write(out_dir.join("scope_record.cbor.hex"), cbor_bytes.hex()).unwrap();
    std::fs::write(out_dir.join("scope_record_hash.txt"), scope_record_hash.clone()).unwrap();
    std::fs::write(out_dir.join("state_commit_root.txt"), merkle_root.clone()).unwrap();
    
    println!("Written to {}/", out_dir.display());
    
    // Verify immutability contract
    println!("\n=== IMMUTABILITY CONTRACT ===");
    println!("These golden vectors represent STATE_COMMIT v1 protocol.");
    println!("Any byte-level change requires a SPEC_VERSION bump to v2.");
    println!("scope_record_hash: {}", scope_record_hash);
    println!("state_commit_root: {}", merkle_root);
}
