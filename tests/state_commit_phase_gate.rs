fn requires_state_commit_vectors(file_path: &str) -> bool {
    let state_commit_paths = [
        "state_commit",
        "state-commit",
        "golden_vector",
        "scope_record",
    ];

    state_commit_paths.iter().any(|p| file_path.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_gate_positive_state_commit_code() {
        let trigger_files = vec![
            "tests/golden_vectors/src/main.rs",
            "src/state_commit.rs",
            "tests/golden/scope_record.cbor",
        ];

        for file in trigger_files {
            let result = requires_state_commit_vectors(file);
            assert!(result, "File {} should require STATE_COMMIT vectors", file);
        }
    }

    #[test]
    fn test_phase_gate_negative_unrelated() {
        let non_trigger_files = vec![
            "README.md",
            "docs/foo.md",
            "src/lib.rs",
            "tests/other_test.rs",
        ];

        for file in non_trigger_files {
            let result = requires_state_commit_vectors(file);
            assert!(
                !result,
                "File {} should NOT require STATE_COMMIT vectors",
                file
            );
        }
    }
}
