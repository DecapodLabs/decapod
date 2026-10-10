// Moved from src/decapod/core/gatekeeper.rs
use super::*;
use tempfile::tempdir;

#[test]
fn test_glob_match() {
    assert!(glob_match("*", "foo"));
    assert!(glob_match("*.rs", "main.rs"));
    assert!(glob_match("**/.credentials", "foo/bar/.credentials"));
    assert!(glob_match("src/**", "src/lib.rs"));
    assert!(glob_match(".env*", ".env.local"));
}

#[test]
fn test_secret_patterns() {
    let patterns = secret_patterns();

    // AWS key
    let line = "AWS_KEY=AKIAIOSFODNN7EXAMPLE";
    assert!(patterns.iter().any(|p| p.is_match(line).unwrap_or(false)));

    // GitHub token
    let line = "token=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    assert!(patterns.iter().any(|p| p.is_match(line).unwrap_or(false)));

    // Private key
    let line = "-----BEGIN PRIVATE KEY-----";
    assert!(patterns.iter().any(|p| p.is_match(line).unwrap_or(false)));
}

#[test]
fn test_dangerous_patterns() {
    let patterns = dangerous_patterns();

    // eval with variable
    let line = "eval $CMD";
    assert!(patterns.iter().any(|p| p.is_match(line).unwrap_or(false)));

    // shell=True
    let line = "subprocess.run(cmd, shell=True)";
    assert!(patterns.iter().any(|p| p.is_match(line).unwrap_or(false)));
}

#[test]
fn test_gatekeeper_default_config() {
    let config = GatekeeperConfig::default();
    assert!(config.scan_secrets);
    assert!(config.scan_dangerous_patterns);
    assert!(!config.block_paths.is_empty());
}

#[test]
fn protected_paths_produce_typed_findings() {
    let tmp = tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("README.md"), "safe\n").expect("write fixture");
    let config = GatekeeperConfig {
        protected_paths: vec!["README.md".to_string()],
        ..GatekeeperConfig::default()
    };

    let result = run_gatekeeper(tmp.path(), &[PathBuf::from("README.md")], 0, &config)
        .expect("run gatekeeper");
    assert!(!result.passed);
    assert!(
        result
            .violations
            .iter()
            .any(|violation| violation.kind == ViolationKind::ProtectedPath)
    );
}

fn scan_text(path: &str, text: &str) -> GateResult {
    let tmp = tempdir().expect("tempdir");
    let path = PathBuf::from(path);
    std::fs::create_dir_all(tmp.path().join(path.parent().expect("parent")))
        .expect("create fixture directory");
    std::fs::write(tmp.path().join(&path), text).expect("write fixture");
    run_gatekeeper(tmp.path(), &[path], 0, &GatekeeperConfig::default()).expect("scan fixture")
}

#[test]
fn bearer_prose_is_not_an_authentication_scheme() {
    for text in [
        "The bearer is an opaque session credential.",
        "Cloud clients use authenticated HTTPS with user bearer credentials.",
        "Opening checks the bearer and context before the first request.",
        "A user/session bearer for the service is required.",
        r#"return Err(Config("Supabase service requires a user bearer token"));"#,
        r#"return Err(Config("invalid Supabase bearer token"));"#,
        "/// Require a separate machine-local bearer credential.",
    ] {
        let result = scan_text("source.rs", text);
        assert!(result.passed, "prose was classified as a secret: {text}");
    }
}

#[test]
fn bearer_scheme_syntax_keeps_short_and_word_like_credentials() {
    for text in [
        "Authorization: Bearer a",
        "authorization: bearer token",
        "Proxy-Authorization: Bearer credentials",
        r#"{"authorization": "Bearer is"}"#,
        r#"let value = "Bearer token";"#,
        r#"let value = "  Bearer token";"#,
        "let value = \"\tBearer token\";",
        "let value = \"\n  Bearer token\";",
        r#"{"authorization": "  Bearer token"}"#,
        "AUTH_HEADER=Bearer token",
        "AUTH_HEADER = '  Bearer token'",
        "auth_header: Bearer token",
        r###"let value = r##"Bearer credentials"##;"###,
        "'Bearer is'",
        "`Bearer token`",
        "Bearer a",
        "  bearer token",
        "// Bearer token",
        "/// Bearer credentials",
        "/* Bearer token */",
        "let x = 1; /* Bearer token */",
        "let x = 1; // Bearer token",
        "# Bearer is",
        "* Bearer material",
        "Authorization = 'Bearer aZ09-._~+/=='",
    ] {
        for path in ["source.rs", "docs/guide.md", "tests/fixture.txt"] {
            let result = scan_text(path, text);
            assert!(
                result
                    .violations
                    .iter()
                    .any(|v| v.kind == ViolationKind::SecretDetected),
                "missed credential syntax in {path}: {text}"
            );
        }
    }
}

#[test]
fn bearer_finding_preserves_multiline_source_location() {
    let result = scan_text("source.rs", "let value = \"\n  Bearer token\";");
    let finding = result
        .violations
        .iter()
        .find(|v| v.kind == ViolationKind::SecretDetected)
        .expect("multiline quoted value must still be detected");
    assert_eq!(finding.line, Some(2));
}

#[test]
fn prose_cannot_hide_another_credential_on_the_same_line() {
    for text in [
        r#"The bearer is opaque; Authorization: Bearer token"#,
        r#"let description = "user bearer credentials"; let value = "Bearer a";"#,
        r#"user bearer credentials; password=secret-server-data"#,
    ] {
        let result = scan_text("docs/guide.md", text);
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.kind == ViolationKind::SecretDetected),
            "prose suppressed a different secret: {text}"
        );
    }
}

#[test]
fn credential_shaped_fixtures_remain_blocked() {
    for text in [
        "AWS_KEY=AKIAIOSFODNN7EXAMPLE",
        "token=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "-----BEGIN PRIVATE KEY-----",
        "postgres://user:password@db.example/db",
        "password=secret-server-data",
        "Authorization: Bearer synthetic-user-bearer",
    ] {
        let result = scan_text("tests/fixture.rs", text);
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.kind == ViolationKind::SecretDetected),
            "credential fixture was exempted: {text}"
        );
    }
}

#[test]
fn ambiguous_format_and_shell_contexts_remain_findings() {
    let placeholder = scan_text("source.rs", r#"println!("Password: {password}");"#);
    assert!(
        placeholder
            .violations
            .iter()
            .any(|v| v.kind == ViolationKind::SecretDetected)
    );

    // A SQL-looking string can flow indirectly into a shell. A file-level
    // keyword test or an extension exemption cannot establish safe dataflow.
    for (path, text) in [
        (
            "source.rs",
            "let query = \"select $1, $2\";\nrun_external(query);",
        ),
        (
            "source.rs",
            "Command::new(\"sh\")\n.arg(\"-c\")\n.arg(\"echo $INPUT\");",
        ),
        (
            "source.rs",
            "let value = \"$(whoami)\";\nrun_external(value);",
        ),
        ("script.sh", "eval $CMD"),
        ("script.sh", "echo ${INPUT}"),
        ("source.py", "subprocess.run(cmd, shell=True)"),
        ("source.py", "exec(user_input)"),
    ] {
        let result = scan_text(path, text);
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.kind == ViolationKind::DangerousPattern),
            "ambiguous executable input was exempted: {text}"
        );
    }
}

fn scan_rust_expression(expression: &str) -> GateResult {
    scan_text("source.rs", &format!("fn example() {{ {expression} }}"))
}

fn has_secret(result: &GateResult) -> bool {
    result
        .violations
        .iter()
        .any(|finding| finding.kind == ViolationKind::SecretDetected)
}

#[test]
fn qualified_format_runtime_environment_is_not_a_hardcoded_password() {
    for expression in [
        r#"::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap());"#,
        r#"::std::format!("Password: {credential}", credential = (::std::env::var("APP_CREDENTIAL")?));"#,
        r##"::std::format!(r#"Password: {credential}"#, credential = ::std::env::var("APP_CREDENTIAL").unwrap());"##,
        r###"::std::format_args!(r##"Password: {credential}"##, credential = ::std::env::var("APP_CREDENTIAL").unwrap());"###,
        r#"::core::format_args!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap());"#,
    ] {
        let result = scan_rust_expression(expression);
        assert!(
            !has_secret(&result),
            "runtime source classified as a literal: {expression}: {:?}",
            result.violations
        );
    }
}

#[test]
fn format_context_uses_original_byte_locations() {
    for source in [
        "fn example() { let label = \"λ🦀\"; ::std::println!(\"Password: {credential}\", credential = ::std::env::var(\"APP_CREDENTIAL\").unwrap()); }",
        "fn example() {\r\n::std::println!(\"Password: {credential}\", credential = ::std::env::var(\"APP_CREDENTIAL\").unwrap());\r\n}",
        "fn example() {\n::std::println!(r###\"\nPassword: {credential}\n\"###, credential = ::std::env::var(\"APP_CREDENTIAL\").unwrap());\n}",
    ] {
        assert!(
            !has_secret(&scan_text("source.rs", source)),
            "wrong source span: {source}"
        );
    }
}

#[test]
fn formatting_cannot_exempt_ordinary_literals_or_other_same_line_secrets() {
    for expression in [
        r#"::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); let value = "password=secret-server-data";"#,
        r#"::std::println!("Password: {credential} password=secret-server-data", credential = ::std::env::var("APP_CREDENTIAL").unwrap());"#,
        r#"::std::println!("Password: {credential} Authorization: Bearer a", credential = ::std::env::var("APP_CREDENTIAL").unwrap());"#,
        r#"::std::println!("Password: {credential}", credential = ::std::env::var("password=secret-server-data").unwrap());"#,
        r#"::std::println!("Password: secret-server-data");"#,
    ] {
        assert!(
            has_secret(&scan_rust_expression(expression)),
            "missed literal: {expression}"
        );
    }
}

#[test]
fn unresolved_formatting_keeps_literal_and_unknown_argument_provenance() {
    for expression in [
        r#"::std::println!("Password: {credential}", credential = "secret-server-data");"#,
        r#"let credential = "secret-server-data"; ::std::println!("Password: {credential}");"#,
        r#"::std::println!("Password: {credential}");"#,
        r#"::std::println!("Password: {credential}", credential = unknown_source());"#,
        r#"::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap_or("secret-server-data"));"#,
        r#"::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap_or_else(|_| "secret-server-data".into()));"#,
        r#"::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").replace("x", "secret-server-data"));"#,
    ] {
        let result = scan_rust_expression(expression);
        assert!(
            has_secret(&result),
            "unresolved source was exempted: {expression}"
        );
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.message.contains("unresolved credential provenance")),
            "missing context: {expression}"
        );
    }
}

#[test]
fn opaque_or_ambiguous_macros_do_not_establish_runtime_provenance() {
    for source in [
        r#"fn example() { println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); }"#,
        r#"fn example() { std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); }"#,
        r#"fn example() { stringify!(::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap())); }"#,
        r#"macro_rules! example { () => { ::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); } }"#,
        r#"#[transform] fn example() { ::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); }"#,
        r#"#[cfg_attr(feature = "x", transform)] fn example() { ::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); }"#,
        r#"extern crate alternate as std; fn example() { ::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); }"#,
        r#"#![no_std] fn example() { ::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); }"#,
        r#"fn example( { ::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); }"#,
    ] {
        assert!(
            has_secret(&scan_text("source.rs", source)),
            "ambiguous context was exempted: {source}"
        );
    }
}

#[test]
fn format_literal_escapes_and_partial_values_remain_findings() {
    for value in [
        "{{credential}}",
        "{{{credential}}}",
        "prefix{credential}",
        "{credential}suffix",
        "{credential:?}",
        "{credential} unmatched{",
        r"\x7b{credential}\x7d",
        r"\u{7b}{credential}\u{7d}",
        r"\{credential}",
    ] {
        let expression = format!(
            r#"::std::println!("Password: {value}", credential = ::std::env::var("APP_CREDENTIAL").unwrap());"#
        );
        assert!(
            has_secret(&scan_rust_expression(&expression)),
            "literal/unsupported format was exempted: {value}"
        );
    }
}

#[test]
fn contextual_classification_does_not_change_non_rust_or_shell_checks() {
    let expression = r#"fn example() { ::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); }"#;
    for path in ["fixture.txt", "guide.md", "source.py"] {
        assert!(
            has_secret(&scan_text(path, expression)),
            "non-Rust input was exempted: {path}"
        );
    }
    let result = scan_rust_expression(
        r#"::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); let shell = "$(whoami)"; run_external(shell);"#,
    );
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.kind == ViolationKind::DangerousPattern)
    );
}

#[test]
fn stripped_prefixes_and_attributed_arguments_keep_findings() {
    let source = r#"fn example() { ::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); }"#;
    for prefix in ["\u{feff}", "#!/usr/bin/env rust-script\n"] {
        assert!(has_secret(&scan_text(
            "source.rs",
            &format!("{prefix}{source}")
        )));
    }
    for expression in [
        r#"::std::println!("Password: {credential}", credential = #[transform] ::std::env::var("APP_CREDENTIAL").unwrap());"#,
        r#"::std::println!("Password: {credential}", credential = (::std::env::var(#[transform] "APP_CREDENTIAL")).unwrap());"#,
        r#"::std::println!(#[transform] "Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap());"#,
    ] {
        assert!(
            has_secret(&scan_rust_expression(expression)),
            "attributed argument was exempted: {expression}"
        );
    }
}

#[test]
fn namespace_ambiguity_preserves_format_findings() {
    let expression = r#"fn example() { ::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); }"#;
    for prefix in [
        "mod std {}",
        "mod r#std {}",
        "use alternate as r#std;",
        "extern crate alternate as r#std;",
        "mod core {}",
        "use alternate as std;",
        "use alternate::{nested as core};",
        "use alternate::*;",
        "extern crate alternate as core;",
    ] {
        assert!(
            has_secret(&scan_text("source.rs", &format!("{prefix}\n{expression}"))),
            "ambiguous namespace was trusted: {prefix}"
        );
    }
}

#[test]
fn attributed_duplicate_or_additional_arguments_cannot_hide_literals() {
    for expression in [
        r#"::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap(), #[transform] credential = "literal-secret");"#,
        r#"::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap(), #[transform] other = unknown());"#,
    ] {
        assert!(
            has_secret(&scan_rust_expression(expression)),
            "attributed ambiguity was discarded: {expression}"
        );
    }
}

#[test]
fn unsupported_or_duplicate_format_bindings_fail_closed() {
    for suffix in [
        r#", credential = "literal-secret""#,
        r#", r#credential = "literal-secret""#,
        r#", (credential) = "literal-secret""#,
        r#", "literal-secret""#,
        r#", other = "literal-secret""#,
    ] {
        let expression = format!(
            r#"::std::println!("Password: {{credential}}", credential = ::std::env::var("APP_CREDENTIAL").unwrap(){suffix});"#
        );
        assert!(
            has_secret(&scan_rust_expression(&expression)),
            "unsupported binding was ignored: {suffix}"
        );
    }
}

#[test]
fn repeated_extraction_is_not_assumed_to_be_a_standard_library_operation() {
    for value in [
        r#"::std::env::var("APP_CREDENTIAL").unwrap().unwrap()"#,
        r#"(::std::env::var("APP_CREDENTIAL")?).unwrap()"#,
        r#"::std::env::var("APP_CREDENTIAL").unwrap()?"#,
        r#"::std::env::var("APP_CREDENTIAL")??"#,
        r#"<Source as ::std::env>::var("APP_CREDENTIAL").unwrap()"#,
    ] {
        let expression =
            format!(r#"::std::println!("Password: {{credential}}", credential = {value});"#);
        assert!(
            has_secret(&scan_rust_expression(&expression)),
            "unproven extraction chain was exempted: {value}"
        );
    }
}

#[test]
fn immutable_environment_captures_have_bounded_provenance() {
    for expression in [
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); ::std::println!("Password: {credential}");"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL")?; ::std::println!("Password: {credential}");"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); let copy = credential; ::std::println!("Password: {copy}");"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); { ::std::println!("Password: {credential}"); }"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); ::std::println!("Password: {value}", value = credential);"#,
    ] {
        assert!(
            !has_secret(&scan_rust_expression(expression)),
            "missed runtime capture: {expression}"
        );
    }
}

#[test]
fn capture_shadowing_mutation_and_opaque_execution_remain_findings() {
    for expression in [
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); let credential = "literal-secret"; ::std::println!("Password: {credential}");"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); { let credential = "literal-secret"; ::std::println!("Password: {credential}"); }"#,
        r#"let mut credential = ::std::env::var("APP_CREDENTIAL").unwrap(); credential = "literal-secret".into(); ::std::println!("Password: {credential}");"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap_or("literal-secret".into()); ::std::println!("Password: {credential}");"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); replace!(credential); ::std::println!("Password: {credential}");"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); let callback = |credential| ::std::println!("Password: {credential}");"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); fn inner() { ::std::println!("Password: {credential}"); }"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); match input { Some(credential) => ::std::println!("Password: {credential}"), _ => {} }"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); ::std::println!("Password: {credential}", other = { credential = "literal-secret".into(); 1 });"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); unsafe { mutate_pointer(&credential); } ::std::println!("Password: {credential}");"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); println!("Password: {credential}");"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); ::std::println!("Password: {credential} password=literal-secret");"#,
    ] {
        assert!(
            has_secret(&scan_rust_expression(expression)),
            "unproven capture was exempted: {expression}"
        );
    }
}

#[test]
fn included_native_sources_remain_inside_explicit_scans() {
    let tmp = tempdir().unwrap();
    std::fs::write(
        tmp.path().join("source.rs"),
        r#"fn example() { let template = format!(include_str!("startup.sh"), name = "test"); }"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("startup.sh"),
        "echo \"${INPUT}\"; eval $COMMAND\n",
    )
    .unwrap();
    let result = run_gatekeeper(
        tmp.path(),
        &[PathBuf::from("source.rs")],
        0,
        &GatekeeperConfig::default(),
    )
    .unwrap();
    assert!(
        result
            .violations
            .iter()
            .any(|finding| finding.kind == ViolationKind::DangerousPattern
                && finding.path == Path::new("startup.sh"))
    );
    std::fs::write(tmp.path().join("startup.sh"), "echo \"${INPUT}\"\n").unwrap();
    assert!(
        run_gatekeeper(
            tmp.path(),
            &[PathBuf::from("source.rs")],
            0,
            &GatekeeperConfig::default()
        )
        .unwrap()
        .passed
    );
    std::fs::write(
        tmp.path().join("startup.sh"),
        "echo \"${INPUT}\"; password=literal-secret\n",
    )
    .unwrap();
    assert!(has_secret(
        &run_gatekeeper(
            tmp.path(),
            &[PathBuf::from("source.rs")],
            0,
            &GatekeeperConfig::default()
        )
        .unwrap()
    ));
}

#[test]
fn missing_dynamic_or_escaping_include_dependencies_fail_closed() {
    for source in [
        r#"fn example() { include_str!("missing.sh"); }"#,
        r#"fn example() { include_str!(concat!("startup", ".sh")); }"#,
        r#"fn example() { include_str!("/tmp/startup.sh"); }"#,
    ] {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("source.rs"), source).unwrap();
        assert!(
            run_gatekeeper(
                tmp.path(),
                &[PathBuf::from("source.rs")],
                0,
                &GatekeeperConfig::default()
            )
            .is_err(),
            "unresolved include passed: {source}"
        );
    }
}

#[test]
fn crate_provenance_requires_real_manifest_edition_and_target() {
    let tmp = tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "fn sample() {}").unwrap();
    std::fs::write(tmp.path().join("src/other.rs"), "fn sample() {}").unwrap();
    for edition in ["2015", "2018", "2021", "2024"] {
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            format!("[package]\nname='example'\nversion='0.1.0'\nedition='{edition}'\n"),
        )
        .unwrap();
        assert_eq!(
            source_dependencies::modern_crate_root(tmp.path(), Path::new("src/lib.rs")),
            edition != "2015"
        );
        assert!(!source_dependencies::modern_crate_root(
            tmp.path(),
            Path::new("src/other.rs")
        ));
    }
}

#[test]
fn explicit_quoted_passwords_keep_short_and_escaped_values() {
    for text in [
        r#"password="a""#,
        r#"password = 'ab'"#,
        r#"{"password": "a"}"#,
        r#"let password = "a";"#,
        r#"password="a\"b""#,
        r#"password="a'b""#,
        r#"password='a"b'"#,
        r#"password=""; passwd="x""#,
        r#"password="two words""#,
        r#"let payload = "{\"password\":\"a\"}";"#,
    ] {
        for path in ["source.rs", "guide.md", "tests/fixture.txt"] {
            assert!(
                has_secret(&scan_text(path, text)),
                "missed explicit password {path}: {text}"
            );
        }
    }
    for text in [
        r#"password="""#,
        r#"password=''"#,
        "fn example(password: &str) {}",
    ] {
        assert!(
            !has_secret(&scan_text("source.rs", text)),
            "empty value or type was treated as password: {text}"
        );
    }
}

#[test]
fn dynamic_native_shell_boundaries_do_not_depend_on_regex_suffixes() {
    for text in [
        "sh -c \"$1\"",
        "eval \"$1\"",
        "source \"$1\"",
        "\"$1\" argument",
    ] {
        let result = scan_text("script.sh", text);
        assert!(
            result
                .violations
                .iter()
                .any(|finding| finding.kind == ViolationKind::DangerousPattern),
            "missed dynamic execution: {text}"
        );
    }
}

fn scan_crate_text(text: &str) -> GateResult {
    let tmp = tempdir().unwrap();
    std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname='scanner-example'\nversion='0.1.0'\nedition='2024'\n[lib]\npath='source.rs'\n").unwrap();
    std::fs::write(tmp.path().join("source.rs"), text).unwrap();
    run_gatekeeper(
        tmp.path(),
        &[PathBuf::from("source.rs")],
        0,
        &GatekeeperConfig::default(),
    )
    .unwrap()
}

#[test]
fn primitive_boolean_text_sinks_prove_inert_literals_only() {
    for source in [
        r#"fn example(current: &str) { ::std::primitive::str::contains(current, "FROM $DECAPOD_IMAGE"); }"#,
        r#"fn example(line: &str) { <::std::primitive::str as ::std::cmp::PartialEq>::eq(line, "FROM $DECAPOD_IMAGE"); }"#,
    ] {
        assert!(
            scan_crate_text(source).passed,
            "primitive literal comparison was flagged: {source}"
        );
        assert!(
            !scan_text("source.rs", source).passed,
            "unknown crate edition was trusted"
        );
    }
    for source in [
        r#"fn example(current: &str) { current.contains("FROM $DECAPOD_IMAGE"); }"#,
        r#"fn example(current: &str) { local::contains(current, "FROM $DECAPOD_IMAGE"); }"#,
        r#"mod std {} fn example(current: &str) { ::std::primitive::str::contains(current, "FROM $DECAPOD_IMAGE"); }"#,
        r#"use alternate as std; fn example(current: &str) { ::std::primitive::str::contains(current, "FROM $DECAPOD_IMAGE"); }"#,
        r#"#[rewrite] fn example(current: &str) { ::std::primitive::str::contains(current, "FROM $DECAPOD_IMAGE"); }"#,
        r#"#[rewrite] trait Example { fn run(current: &str) { ::std::primitive::str::contains(current, "FROM $DECAPOD_IMAGE"); } }"#,
        r#"trait Example { #[rewrite] fn run(current: &str) { ::std::primitive::str::contains(current, "FROM $DECAPOD_IMAGE"); } }"#,
        r#"#[rewrite] const VALUE: bool = ::std::primitive::str::contains("source", "FROM $DECAPOD_IMAGE");"#,
        r#"#[rewrite] static VALUE: bool = ::std::primitive::str::contains("source", "FROM $DECAPOD_IMAGE");"#,
        r#"struct Example; impl Example { #[rewrite] const VALUE: bool = ::std::primitive::str::contains("source", "FROM $DECAPOD_IMAGE"); }"#,
        r#"fn example(current: &str) { stringify!(::std::primitive::str::contains(current, "FROM $DECAPOD_IMAGE")); }"#,
        r#"fn example(current: &str) { <::std::primitive::str as Custom>::eq(current, "FROM $DECAPOD_IMAGE"); }"#,
        r#"fn example(current: &str) { <Custom as ::std::cmp::PartialEq>::eq(current, "FROM $DECAPOD_IMAGE"); }"#,
        r#"fn example(current: &str) { <::std::primitive::str as ::std::cmp::PartialEq<Custom>>::eq(current, "FROM $DECAPOD_IMAGE"); }"#,
        r#"fn example(current: &str) { let value = "FROM $DECAPOD_IMAGE"; ::std::primitive::str::contains(current, value); run_external(value); }"#,
        r#"fn example(current: &str) { ::std::primitive::str::contains(current, "FROM $DECAPOD_IMAGE"); run_external("${INPUT}"); }"#,
    ] {
        assert!(
            !scan_crate_text(source).passed,
            "unproven text flow was exempted: {source}"
        );
    }
    assert!(has_secret(&scan_crate_text(
        r#"fn example(current: &str) { ::std::primitive::str::contains(current, "password='x'"); }"#
    )));
}

#[test]
fn commit_message_hook_preserves_original_master_bytes() {
    use sha2::{Digest, Sha256};
    let hook = include_str!("../../../src/decapod/hooks/commit-msg.sh");
    assert_eq!(
        format!("{:x}", Sha256::digest(hook.as_bytes())),
        "a9430c4833b2f639b926af841061f066f3879c6de108528a9c566621de96e182"
    );
    let result = scan_text("commit-msg.sh", hook);
    assert!(
        result
            .violations
            .iter()
            .any(|finding| finding.kind == ViolationKind::DangerousPattern
                && finding.line == Some(3)),
        "actual command substitution must remain"
    );
    assert!(
        !result
            .violations
            .iter()
            .any(|finding| finding.kind == ViolationKind::DangerousPattern
                && matches!(finding.line, Some(4 | 8))),
        "quoted subject data should be classified in shell context"
    );
}

#[test]
fn old_literal_password_export_instruction_remains_a_secret_fixture() {
    let text = "Export before running other commands: DECAPOD_AGENT_ID='{}' and DECAPOD_SESSION_PASSWORD='<token>'";
    assert!(has_secret(&scan_text("tests/fixture.txt", text)));
}

#[test]
fn visible_standard_library_replacement_cannot_supply_primitive_proof() {
    let tmp = tempdir().unwrap();
    std::fs::write(tmp.path().join("source.rs"), "fn example() {}").unwrap();
    let base =
        "[package]\nname='example'\nversion='0.1.0'\nedition='2024'\n[lib]\npath='source.rs'\n";
    for suffix in [
        "[dependencies]\nstd={path='fake'}\n",
        "[dev-dependencies]\nstd='1'\n",
        "[build-dependencies]\nstd={package='fake',version='1'}\n",
        "[target.'cfg(unix)'.dependencies]\nstd='1'\n",
        "[workspace.dependencies]\nstd='1'\n",
        "[patch.crates-io]\nstd={path='fake'}\n",
        "[replace]\n'std:0.1.0'={path='fake'}\n",
    ] {
        std::fs::write(tmp.path().join("Cargo.toml"), format!("{base}{suffix}")).unwrap();
        assert!(
            !source_dependencies::modern_crate_root(tmp.path(), Path::new("source.rs")),
            "visible std replacement was trusted: {suffix}"
        );
    }
}

#[test]
fn explicit_unquoted_password_assignments_keep_short_values() {
    for text in [
        "password=a",
        "PASSWORD = abc",
        "export PASSWORD=x",
        "pwd=1",
        "passwd=$VALUE",
        "password=<token>",
    ] {
        for path in ["source.rs", "script.sh", "guide.md", "tests/fixture.txt"] {
            assert!(
                has_secret(&scan_text(path, text)),
                "missed unquoted password {path}: {text}"
            );
        }
    }
    for text in [
        "password=",
        "password == a",
        "password => value",
        "fn example(password: &str) {}",
    ] {
        assert!(
            !has_secret(&scan_text("source.rs", text)),
            "non-value was classified as a password: {text}"
        );
    }
    assert!(!has_secret(&scan_rust_expression(
        r#"let password = ::std::env::var("APP_CREDENTIAL").unwrap();"#
    )));
    assert!(!has_secret(&scan_rust_expression(
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); let password = credential;"#
    )));
    for expression in [
        r#"let password = ::std::env::var("password=a").unwrap();"#,
        r#"let password = ::std::env::var("APP_CREDENTIAL").unwrap_or("x".into());"#,
        r#"let password = unknown();"#,
        r#"let password = ::std::env::var("APP_CREDENTIAL").unwrap(); let other = "password=a";"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); let password = credential;password=x;"#,
    ] {
        assert!(
            has_secret(&scan_rust_expression(expression)),
            "initializer proof hid a literal/unknown source: {expression}"
        );
    }
}

#[test]
fn typed_rust_password_bindings_keep_literal_and_unknown_values() {
    for expression in [
        r#"let password: &str="a";"#,
        r#"let passwd: String = "ab".into();"#,
        r#"let mut pwd: &'static str = "x";"#,
        r#"let ref password: &str = "a";"#,
        r#"let ref mut passwd: &str = "a";"#,
        r#"let pwd: [u8; 1] = *b"a";"#,
        r#"let ref mut pwd: [u8; LENGTH] = *b"a";"#,
        "let\nref\nmut\npwd:\n[u8;\n1]\n= *b\"a\";",
        r#"let r#password: &str = "a\"b";"#,
        r##"let password: &str = r#"a"#;"##,
        "let pwd: u8 = 1;",
        "let passwd: char = 'x';",
        r#"const PASSWORD: &str = "a";"#,
        r#"static mut PWD: &str = "a";"#,
        "let\npassword\n:\n&\nstr\n=\n\"a\";",
        "let\tpasswd\t:\tString\t=\t\"a\".into();",
        "let pwd:\r\n&str =\r\n\"a\";",
        "let password: &str = \"two\nwords\";",
        r#"let password: String = unknown();"#,
        r#"let password: String = ::std::env::var("APP_CREDENTIAL").unwrap_or("x".into());"#,
    ] {
        assert!(
            has_secret(&scan_rust_expression(expression)),
            "missed typed password binding: {expression}"
        );
    }
    // Detection is independent of parser success and of the source filename.
    for path in ["source.rs", "guide.md", "tests/fixture.txt"] {
        assert!(has_secret(&scan_text(
            path,
            r#"fn f(){let password: &str="a";} incomplete ("#
        )));
    }
}

#[test]
fn typed_rust_password_bindings_preserve_runtime_initializer_scope() {
    for expression in [
        r#"let password: String = ::std::env::var("APP_CREDENTIAL").unwrap();"#,
        "let\npasswd\n:\nString\n=\n::std::env::var(\"APP_CREDENTIAL\").unwrap();",
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); let pwd: String = credential;"#,
        r#"let password: &str = "";"#,
        "let password: &str;",
        "let password: &str; let other = \"a\";",
        "let ref password: &str; let other = \"a\";",
        "let pwd: [u8; 1]; let other = b\"a\";",
        "let pwd: [u8; 1];\nlet other = b\"a\";",
    ] {
        assert!(
            !has_secret(&scan_rust_expression(expression)),
            "runtime or empty typed binding was classified as a secret: {expression}"
        );
    }
    for expression in [
        r#"let password: String = ::std::env::var("APP_CREDENTIAL").unwrap(); let passwd: &str = "x";"#,
        r#"let password: String = ::std::env::var("APP_CREDENTIAL").unwrap(); let ref mut pwd: [u8; 1] = *b"a";"#,
        r#"let password: &str = "x"; let passwd: String = ::std::env::var("APP_CREDENTIAL").unwrap();"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); let pwd: String = credential; let password: &str = "x";"#,
        r#"let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); ::std::println!("Password: {credential}"); let password: &str = "x";"#,
        r#"let password: String = ::std::env::var("password=a").unwrap();"#,
        r#"let password: String = ::std::env::var("APP_CREDENTIAL").unwrap(); let other = "password=a";"#,
        r#"let password: String = ::std::env::var("APP_CREDENTIAL").unwrap(); let passwd: &str = "{password}";"#,
    ] {
        assert!(
            has_secret(&scan_rust_expression(expression)),
            "runtime proof hid another typed literal: {expression}"
        );
    }
}

#[test]
fn typed_rust_password_binding_reports_the_value_line() {
    let result = scan_text("source.rs", "fn f() {\nlet password:\n&str =\n\"a\";\n}");
    assert!(result.violations.iter().any(|violation| {
        violation.kind == ViolationKind::SecretDetected && violation.line == Some(4)
    }));
}

#[test]
fn included_source_coverage_is_independent_of_suffix_and_input_order() {
    for name in ["payload.txt", "payload.custom", "payload"] {
        let tmp = tempdir().unwrap();
        std::fs::write(
            tmp.path().join("source.rs"),
            format!(
                r#"fn example() {{ Command::new("sh").arg("-c").arg(include_str!("{name}")); }}"#
            ),
        )
        .unwrap();
        std::fs::write(tmp.path().join(name), "eval \"$INPUT\"\n").unwrap();
        for paths in [
            vec![PathBuf::from("source.rs")],
            vec![PathBuf::from("source.rs"), PathBuf::from(name)],
            vec![PathBuf::from(name), PathBuf::from("source.rs")],
        ] {
            let result =
                run_gatekeeper(tmp.path(), &paths, 0, &GatekeeperConfig::default()).unwrap();
            assert!(
                result
                    .violations
                    .iter()
                    .any(|finding| finding.kind == ViolationKind::DangerousPattern
                        && finding.path == Path::new(name)),
                "included source vanished: {name}, {paths:?}"
            );
        }
    }
}

#[test]
fn extensionless_shebang_sources_keep_dynamic_execution_checks() {
    let result = scan_text("script", "#!/bin/sh\necho \"${INPUT}\"\neval \"$1\"\n");
    assert!(
        result
            .violations
            .iter()
            .any(|finding| finding.kind == ViolationKind::DangerousPattern
                && finding.line == Some(3))
    );
    // An evaluator anywhere in a script intentionally invalidates local
    // quoting evidence. Check the safe standalone grammar separately.
    assert!(scan_text("script", "#!/bin/sh\necho \"${INPUT}\"\n").passed);
}

#[test]
fn explicit_shell_interpreters_cannot_borrow_included_dockerfile_grammar() {
    for expression in [
        r#"::std::process::Command::new("sh").arg("-c").arg(include_str!("Dockerfile.payload"));"#,
        "::std::process::Command::new(\"/bin/sh\")\n.arg(\"-c\")\n.arg(include_str!(\"Dockerfile.payload\"));",
        r#"let mut command = Command::new("sh"); command.arg("-c"); command.arg(include_str!("Dockerfile.payload"));"#,
        r#"use ::std::process::Command as Process; Process::new("sh").args(["-c", include_str!("Dockerfile.payload")]);"#,
        r#"type Process = ::std::process::Command; Process::new("sh").arg(include_str!("Dockerfile.payload"));"#,
        r#"::std::process::Command::new(interpreter).args(arguments).arg(include_str!("Dockerfile.payload"));"#,
        r#"opaque!(::std::process::Command::new("sh").arg("-c").arg(include_str!("Dockerfile.payload")));"#,
    ] {
        let tmp = tempdir().unwrap();
        std::fs::write(
            tmp.path().join("source.rs"),
            format!("fn example() {{ {expression} }}"),
        )
        .unwrap();
        std::fs::write(tmp.path().join("Dockerfile.payload"), "FROM $INPUT\n").unwrap();
        let result = run_gatekeeper(
            tmp.path(),
            &[PathBuf::from("source.rs")],
            0,
            &GatekeeperConfig::default(),
        )
        .unwrap();
        assert!(
            result
                .violations
                .iter()
                .any(|finding| finding.kind == ViolationKind::DangerousPattern
                    && finding.path == Path::new("source.rs")
                    && finding.message.contains("interpreter boundary")),
            "explicit interpreter was hidden by an included filename: {expression}"
        );
    }
}

#[test]
fn visible_std_manifest_substitutions_veto_every_password_exemption() {
    for source in [
        r#"fn example() { ::std::println!("Password: {credential}", credential = ::std::env::var("APP_CREDENTIAL").unwrap()); }"#,
        r#"fn example() { let credential = ::std::env::var("APP_CREDENTIAL").unwrap(); ::std::println!("Password: {credential}"); }"#,
        r#"fn example() { let password = ::std::env::var("APP_CREDENTIAL").unwrap(); }"#,
    ] {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("source.rs"), source).unwrap();
        let paths = [PathBuf::from("source.rs")];
        assert!(
            !has_secret(
                &run_gatekeeper(tmp.path(), &paths, 0, &GatekeeperConfig::default()).unwrap()
            ),
            "source-only baseline changed"
        );
        for manifest in [
            "[package]\nname='example'\nversion='0.1.0'\nedition='2024'\n[lib]\npath='source.rs'\n[dependencies]\nstd={path='fake-std'}\n",
            "[package]\nname='example'\nversion='0.1.0'\nedition='2024'\n[patch.crates-io]\nstd={path='fake-std'}\n",
            "[package\n",
        ] {
            std::fs::write(tmp.path().join("Cargo.toml"), manifest).unwrap();
            assert!(
                has_secret(
                    &run_gatekeeper(tmp.path(), &paths, 0, &GatekeeperConfig::default()).unwrap()
                ),
                "known manifest ambiguity still lent password proof: {source}, {manifest}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn unresolved_manifest_links_do_not_become_absent_evidence() {
    let tmp = tempdir().unwrap();
    std::fs::write(
        tmp.path().join("source.rs"),
        r#"fn example() { let password = ::std::env::var("APP_CREDENTIAL").unwrap(); }"#,
    )
    .unwrap();
    std::os::unix::fs::symlink("missing-manifest", tmp.path().join("Cargo.toml")).unwrap();
    assert!(has_secret(
        &run_gatekeeper(
            tmp.path(),
            &[PathBuf::from("source.rs")],
            0,
            &GatekeeperConfig::default()
        )
        .unwrap()
    ));
}
