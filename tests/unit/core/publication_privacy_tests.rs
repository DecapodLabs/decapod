use super::*;

const TEST_IDENTIFIER: &str = "sample.private:identifier";

#[test]
fn patch_checks_new_content_and_paths_without_disclosing_the_match() {
    let rules = [Rule {
        source: RuleSource::LocalPolicy,
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
    assert!(read_local_rules(&policy).unwrap().is_empty());
    fs_permissions::ensure_private_dir(directory.path()).unwrap();
    let mut file =
        fs_permissions::open_private_file(&policy, OpenOptions::new().create_new(true).write(true))
            .unwrap();
    use std::io::Write;
    writeln!(file, "\n{TEST_IDENTIFIER}").unwrap();
    drop(file);

    let rules = read_local_rules(&policy).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].line, 2);
    assert!(check_match(&rules, b"sampleXprivate:identifier", false).is_ok());
    assert!(check_match(&rules, TEST_IDENTIFIER.as_bytes(), false).is_err());
}

#[test]
fn config_deny_rules_flag_authored_source_without_self_flagging_config_toml() {
    let parsed = parse_config_rules(
        format!("[governance]\npublication_deny_identifiers = [\"{TEST_IDENTIFIER}\"]\n")
            .as_bytes(),
    )
    .unwrap();
    assert_eq!(parsed, vec![TEST_IDENTIFIER.to_owned()]);

    let rules = [Rule {
        source: RuleSource::Config,
        line: 1,
        literal: TEST_IDENTIFIER.to_owned(),
    }];

    // Adding the rule itself to .decapod/config.toml does not self-flag.
    let config_patch = format!(
        "diff --git a/.decapod/config.toml b/.decapod/config.toml\n@@ -1,0 +1,1 @@\n+publication_deny_identifiers = [\"{TEST_IDENTIFIER}\"]\n"
    );
    assert!(check_patch(&rules, config_patch.as_bytes()).is_ok());

    // Adding the identifier to any source file fails closed and redacts the literal.
    let leak_patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n@@ -1,0 +1,1 @@\n+const LEAK: &str = \"{TEST_IDENTIFIER}\";\n"
    );
    let err = check_patch(&rules, leak_patch.as_bytes())
        .unwrap_err()
        .to_string();
    assert!(err.contains("PUBLICATION_PRIVATE_IDENTIFIER"));
    assert!(err.contains("governance.publication_deny_identifiers rule #1"));
    assert!(!err.contains(TEST_IDENTIFIER));
}
