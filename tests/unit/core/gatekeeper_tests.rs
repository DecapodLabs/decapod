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
