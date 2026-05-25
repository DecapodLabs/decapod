use regex::Regex;
use std::fs;
use std::process::Command;

fn main() {
    let patterns = vec![
        Regex::new(r#"(?i)aws(.{0,20})?['"][0-9a-zA-Z/+=]{40}['"]"#).unwrap(),
        Regex::new(r#"(?i)(api[_-]?key|apikey|api_secret|secret[_-]?key)['"]?\s*[:=]\s*['"]?[a-zA-Z0-9_\-]{20,}['"]?"#).unwrap(),
        Regex::new(r#"(ghp|gho|ghu|ghs|ghr)_[a-zA-Z0-9_]{36,255}"#).unwrap(),
        Regex::new(r#"(?i)(password|passwd|pwd)['"]?\s*[:=]\s*['"]?[^\s'"]{8,}['"]?"#).unwrap(),
        Regex::new(r#"-----BEGIN (RSA |DSA |EC |OPENSSH )?PRIVATE KEY-----"#).unwrap(),
        Regex::new(r#"(?i)(postgres|mysql|mongodb|redis)://[^\s'"]+:[^\s'"]+@[^\s'"]+"#).unwrap(),
    ];

    let output = Command::new("git").arg("ls-files").output().unwrap();
    let files = String::from_utf8(output.stdout).unwrap();
    for f_name in files.lines() {
        if let Ok(content) = fs::read_to_string(f_name) {
            for (i, line) in content.lines().enumerate() {
                for (j, p) in patterns.iter().enumerate() {
                    if p.is_match(line) {
                        println!("Match found in {}:{} : pattern {}", f_name, i+1, j);
                    }
                }
            }
        }
    }
}
