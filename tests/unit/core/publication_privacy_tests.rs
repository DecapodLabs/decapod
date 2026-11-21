use super::*;

const TEST_IDENTIFIER: &str = "sample.private:identifier";

#[test]
fn patch_checks_new_content_and_paths_without_disclosing_the_match() {
    let rules = [Rule {
        line: 3,
        literal: TEST_IDENTIFIER.to_owned(),
    }];
    let removed = format!(
        "diff --git a/{TEST_IDENTIFIER} b/{TEST_IDENTIFIER}\ndeleted file mode 100644\n-{TEST_IDENTIFIER}"
    );
    assert!(check_patch(&rules, removed.as_bytes()).is_ok());

    for patch in [
        format!("diff --git a/{TEST_IDENTIFIER} b/{TEST_IDENTIFIER}\nnew file mode 100644"),
        format!("@@ -0,0 +1 @@\n+identifier: {TEST_IDENTIFIER}"),
    ] {
        let error = check_patch(&rules, patch.as_bytes())
            .unwrap_err()
            .to_string();
        assert!(error.contains("PUBLICATION_PRIVATE_IDENTIFIER"));
        assert!(error.contains("line 3"));
        assert!(!error.contains(TEST_IDENTIFIER));
    }
}

#[test]
fn local_rules_are_optional_and_interpret_metacharacters_literally() {
    let directory = tempfile::tempdir().unwrap();
    let policy = directory.path().join("publication-deny.txt");
    assert!(read_rules(&policy).unwrap().is_empty());
    fs_permissions::ensure_private_dir(directory.path()).unwrap();
    let mut file =
        fs_permissions::open_private_file(&policy, OpenOptions::new().create_new(true).write(true))
            .unwrap();
    use std::io::Write;
    writeln!(file, "\n{TEST_IDENTIFIER}").unwrap();
    drop(file);

    let rules = read_rules(&policy).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].line, 2);
    assert!(check_match(&rules, b"sampleXprivate:identifier").is_ok());
    assert!(check_match(&rules, TEST_IDENTIFIER.as_bytes()).is_err());
}
