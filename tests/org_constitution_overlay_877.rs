use decapod::core::context_bundle::{build_bundle, validate_bundle};
use decapod::core::context_capsule::query_embedded_capsule;
use decapod::core::interop::{builtin_manifest, validate_manifest};
use decapod::core::org_overlay::{
    DirectiveOverridability, DirectiveVisibility, ORG_OVERLAY_SCHEMA_VERSION,
    OrganizationDirective, OrganizationOverlayBundle, OverlayFreshnessState, OverlayTrustState,
    clear_imported_overlay, import_overlay_bundle, load_organization_overlay,
    resolve_authority_chain_with_now,
};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::tempdir;

fn decapod_bin() -> &'static str {
    env!("CARGO_BIN_EXE_decapod")
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn make_signed_bundle(
    org_id: &str,
    directives: Vec<OrganizationDirective>,
    issued_at_unix: u64,
    expires_at_unix: Option<u64>,
    offline_cached: bool,
) -> (OrganizationOverlayBundle, String) {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).expect("pkcs8");
    let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("keypair");
    let pk_hex = encode_hex(keypair.public_key().as_ref());

    let mut bundle = OrganizationOverlayBundle {
        schema_version: ORG_OVERLAY_SCHEMA_VERSION.to_string(),
        org_id: org_id.to_string(),
        org_name: Some("Acme Compliance Corp".to_string()),
        version: "2026.11.21-rc1".to_string(),
        source: "org-registry://acme/governance-v1".to_string(),
        issued_at_unix,
        expires_at_unix,
        offline_cached,
        directives,
        bundle_sha256: String::new(),
        signature_hex: None,
        signer_key_id: Some("acme-root-ed25519".to_string()),
    }
    .with_recomputed_digest()
    .expect("digest");

    let payload = bundle.canonical_payload_bytes().expect("payload");
    let sig = keypair.sign(&payload);
    bundle.signature_hex = Some(encode_hex(sig.as_ref()));
    (bundle, pk_hex)
}

fn init_minimal_repo(root: &Path, extra_config: &str) {
    fs::create_dir_all(root.join(".decapod/data")).expect("mkdir .decapod/data");
    fs::create_dir_all(root.join(".decapod/managed/specs")).expect("mkdir specs");
    let mut config = r#"schema_version = "1.0.0"
[init]
diagram_style = "mermaid"
entrypoints = []
[repo]
backend = "local"
product_summary = "Organization overlay test repo"
architecture_direction = "cli"
container_workspaces = false
[custody]
isolation = "worktree"
"#
    .to_string();
    if !extra_config.is_empty() {
        config.push('\n');
        config.push_str(extra_config);
        config.push('\n');
    }
    fs::write(root.join(".decapod/config.toml"), config).expect("write config.toml");

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("git run");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-b", "master"]);
    git(&["config", "user.email", "ci@example.com"]);
    git(&["config", "user.name", "CI Fixture"]);
    git(&["add", ".decapod/config.toml"]);
    git(&["commit", "-m", "chore: fixture init"]);
}

fn run_decapod(root: &Path, args: &[&str], stdin_body: Option<&str>) -> std::process::Output {
    let mut cmd = Command::new(decapod_bin());
    cmd.args(args)
        .current_dir(root)
        .env("DECAPOD_AGENT_ID", "test-agent-877")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .env_remove("SUPABASE_URL")
        .env_remove("SUPABASE_ANON_KEY");
    if let Some(body) = stdin_body {
        use std::io::Write;
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("spawn decapod");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(body.as_bytes())
            .expect("write stdin");
        child.wait_with_output().expect("wait output")
    } else {
        cmd.output().expect("run decapod")
    }
}

fn write_override(root: &Path, sections: &[(&str, &str)]) {
    let mut text = String::from("<!-- CHANGES ARE NOT PERMITTED ABOVE THIS LINE -->\n");
    for (id, body) in sections {
        text.push_str(&format!("### {id}\n````markdown\n{body}\n````\n\n"));
    }
    fs::write(root.join(".decapod/OVERRIDE.md"), text).expect("write OVERRIDE.md");
}

#[test]
fn issue_877_covers_base_org_only_project_only_and_all_three_with_provenance() {
    // 1. base-only
    let tmp_base = tempdir().unwrap();
    init_minimal_repo(tmp_base.path(), "");
    let base_res = resolve_authority_chain_with_now(tmp_base.path(), 1_750_000_000).unwrap();
    assert!(!base_res.chain.organization_overlay.present);
    assert_eq!(
        base_res.chain.organization_overlay.trust_state,
        OverlayTrustState::Absent
    );
    assert_eq!(
        base_res.chain.authoritative_order,
        vec![
            "binary_embedded",
            "organization_overlay",
            "repository_project_override"
        ]
    );
    assert_eq!(
        base_res.chain.non_authoritative_projection,
        "task_context_projection"
    );
    assert!(base_res.chain.task_projection_isolation_enforced);
    assert!(base_res.chain.directives.is_empty());

    // Verify interop profile advertises the 4-layer authority hierarchy cleanly.
    let manifest = builtin_manifest();
    validate_manifest(&manifest).expect("interop manifest must remain valid");
    let hierarchy = manifest
        .authority_hierarchy
        .as_ref()
        .expect("authority_hierarchy present on interop manifest");
    assert_eq!(hierarchy.len(), 4);
    assert!(hierarchy[0].authoritative && hierarchy[0].layer == "binary_embedded");
    assert!(hierarchy[1].authoritative && hierarchy[1].layer == "organization_overlay");
    assert!(hierarchy[2].authoritative && hierarchy[2].layer == "repository_project_override");
    assert!(!hierarchy[3].authoritative && hierarchy[3].layer == "task_context_projection");

    // 2. organization-only (Layer 1 + Layer 2 signed overlay, no OVERRIDE.md)
    let tmp_org = tempdir().unwrap();
    let (bundle_org, pk_org) = make_signed_bundle(
        "org_acme",
        vec![OrganizationDirective {
            directive_id: "core/DECAPOD".to_string(),
            overridability: DirectiveOverridability::NonOverridable,
            visibility: DirectiveVisibility::Shared,
            body: "Org baseline: require signed commits on all release branches.".to_string(),
            required_terms: vec![],
            prohibited_weakening_terms: vec![],
            rationale: Some("SOC2 compliance".to_string()),
        }],
        1_700_000_000,
        Some(4_000_000_000),
        false,
    );
    init_minimal_repo(
        tmp_org.path(),
        &format!(
            "[context.organization_overlay]\npath = \".decapod/org/overlay.json\"\norg_id = \"org_acme\"\nsha256 = \"{}\"\npublic_key_hex = \"{}\"\n",
            bundle_org.bundle_sha256, pk_org
        ),
    );
    fs::create_dir_all(tmp_org.path().join(".decapod/org")).unwrap();
    fs::write(
        tmp_org.path().join(".decapod/org/overlay.json"),
        serde_json::to_vec_pretty(&bundle_org).unwrap(),
    )
    .unwrap();

    let org_res = resolve_authority_chain_with_now(tmp_org.path(), 1_750_000_000).unwrap();
    assert_eq!(
        org_res.chain.organization_overlay.trust_state,
        OverlayTrustState::VerifiedSignature
    );
    assert_eq!(org_res.chain.directives.len(), 1);
    assert_eq!(
        org_res.chain.directives[0].effective_layer,
        "organization_overlay"
    );
    assert_eq!(
        org_res.chain.directives[0].precedence_decision,
        "organization_non_overridable_enforced"
    );

    // CLI + RPC surfaces work offline without any cloud account.
    let status_out = run_decapod(
        tmp_org.path(),
        &["constitution", "overlay", "status", "--format", "json"],
        None,
    );
    assert!(
        status_out.status.success(),
        "overlay status failed: {}",
        String::from_utf8_lossy(&status_out.stderr)
    );
    let status_json: Value = serde_json::from_slice(&status_out.stdout).expect("status json");
    assert_eq!(
        status_json["organization_overlay"]["trust_state"],
        "verified_signature"
    );

    let rpc_out = run_decapod(
        tmp_org.path(),
        &["rpc", "--op", "context.resolve"],
        Some(r#"{"op":"context.resolve","params":{"query":"DECAPOD"}}"#),
    );
    assert!(
        rpc_out.status.success(),
        "context.resolve failed: {}",
        String::from_utf8_lossy(&rpc_out.stderr)
    );
    let rpc_json: Value = serde_json::from_slice(&rpc_out.stdout).expect("rpc json");
    assert!(rpc_json["success"].as_bool().unwrap_or(false));
    assert_eq!(
        rpc_json["result"]["authority_chain"]["organization_overlay"]["org_id"],
        "org_acme"
    );

    // 3. project-only (Layer 1 + Layer 3 OVERRIDE.md, no org overlay)
    let tmp_proj = tempdir().unwrap();
    init_minimal_repo(tmp_proj.path(), "");
    write_override(
        tmp_proj.path(),
        &[("core/DEMANDS", "Project-only directive text.")],
    );
    let proj_res = resolve_authority_chain_with_now(tmp_proj.path(), 1_750_000_000).unwrap();
    assert!(!proj_res.chain.organization_overlay.present);
    assert_eq!(proj_res.chain.directives.len(), 1);
    assert_eq!(
        proj_res.chain.directives[0].effective_layer,
        "repository_project_override"
    );
    assert_eq!(
        proj_res.chain.directives[0].precedence_decision,
        "project_override_applied"
    );

    // 4. all-three: Overridable overridden by project, ExtendOnly extended additively, NonOverridable kept from org
    let tmp_all = tempdir().unwrap();
    let (bundle_all, pk_all) = make_signed_bundle(
        "org_fleet",
        vec![
            OrganizationDirective {
                directive_id: "core/DECAPOD".to_string(),
                overridability: DirectiveOverridability::NonOverridable,
                visibility: DirectiveVisibility::Shared,
                body: "Org NonOverridable: mandatory traceability proof.".to_string(),
                required_terms: vec![],
                prohibited_weakening_terms: vec![],
                rationale: None,
            },
            OrganizationDirective {
                directive_id: "core/DEMANDS".to_string(),
                overridability: DirectiveOverridability::ExtendOnly,
                visibility: DirectiveVisibility::Shared,
                body: "Org ExtendOnly: retain audit log.".to_string(),
                required_terms: vec!["audit log".to_string()],
                prohibited_weakening_terms: vec![],
                rationale: None,
            },
            OrganizationDirective {
                directive_id: "interfaces/CONTROL_PLANE".to_string(),
                overridability: DirectiveOverridability::Overridable,
                visibility: DirectiveVisibility::Shared,
                body: "Org Overridable: default RPC timeout 30s.".to_string(),
                required_terms: vec![],
                prohibited_weakening_terms: vec![],
                rationale: None,
            },
        ],
        1_700_000_000,
        Some(4_000_000_000),
        false,
    );
    init_minimal_repo(
        tmp_all.path(),
        &format!(
            "[context.organization_overlay]\npath = \".decapod/org-overlay.json\"\norg_id = \"org_fleet\"\nsha256 = \"{}\"\npublic_key_hex = \"{}\"\n",
            bundle_all.bundle_sha256, pk_all
        ),
    );
    fs::write(
        tmp_all.path().join(".decapod/org-overlay.json"),
        serde_json::to_vec_pretty(&bundle_all).unwrap(),
    )
    .unwrap();
    write_override(
        tmp_all.path(),
        &[
            (
                "core/DEMANDS",
                "Project extension: keep audit log immutable for 365 days.",
            ),
            (
                "interfaces/CONTROL_PLANE",
                "Project override: custom RPC timeout 15s for this repo.",
            ),
        ],
    );

    let all_res = resolve_authority_chain_with_now(tmp_all.path(), 1_750_000_000).unwrap();
    let by_id: std::collections::BTreeMap<_, _> = all_res
        .chain
        .directives
        .iter()
        .map(|d| (d.directive_id.as_str(), d))
        .collect();
    assert_eq!(
        by_id["core/DECAPOD"].precedence_decision,
        "organization_non_overridable_enforced"
    );
    assert_eq!(
        by_id["core/DEMANDS"].precedence_decision,
        "organization_extended_by_project"
    );
    assert_eq!(
        by_id["interfaces/CONTROL_PLANE"].precedence_decision,
        "organization_overridden_by_project"
    );

    // Context capsule includes complete authority_chain provenance & Layer 4 isolation.
    let capsule = query_embedded_capsule(
        tmp_all.path(),
        "audit log",
        "core",
        Some("task_877"),
        None,
        6,
    )
    .expect("context capsule");
    let chain = capsule.authority_chain.as_ref().expect("authority_chain");
    assert!(chain.task_projection_isolation_enforced);
    assert_eq!(chain.directives.len(), 3);
}

#[test]
fn issue_877_covers_conflicting_stale_tampered_offline_and_private_redaction() {
    // 5. conflicting: NonOverridable conflict + ExtendOnly weakening fail closed + unverified org_id rejected
    let tmp_conflict = tempdir().unwrap();
    let (bundle_c, pk_c) = make_signed_bundle(
        "org_strict",
        vec![OrganizationDirective {
            directive_id: "core/DECAPOD".to_string(),
            overridability: DirectiveOverridability::NonOverridable,
            visibility: DirectiveVisibility::Shared,
            body: "Mandatory org gate.".to_string(),
            required_terms: vec![],
            prohibited_weakening_terms: vec![],
            rationale: None,
        }],
        1_700_000_000,
        Some(4_000_000_000),
        false,
    );
    init_minimal_repo(
        tmp_conflict.path(),
        &format!(
            "[context.organization_overlay]\npath = \".decapod/org.json\"\norg_id = \"org_strict\"\npublic_key_hex = \"{}\"\n",
            pk_c
        ),
    );
    fs::write(
        tmp_conflict.path().join(".decapod/org.json"),
        serde_json::to_vec_pretty(&bundle_c).unwrap(),
    )
    .unwrap();
    write_override(
        tmp_conflict.path(),
        &[("core/DECAPOD", "Project overrides mandatory org gate.")],
    );
    let verify_fail = run_decapod(
        tmp_conflict.path(),
        &["constitution", "overlay", "verify"],
        None,
    );
    assert!(!verify_fail.status.success());
    let stderr = String::from_utf8_lossy(&verify_fail.stderr);
    assert!(
        stderr.contains("ORG_OVERLAY_NON_OVERRIDABLE_CONFLICT"),
        "expected non_overridable conflict diagnostic, got: {stderr}"
    );

    // 6. stale: expired bundle fails closed on verify and context.resolve, while status surfaces freshness=stale
    let tmp_stale = tempdir().unwrap();
    let (bundle_s, pk_s) = make_signed_bundle(
        "org_stale",
        vec![OrganizationDirective {
            directive_id: "core/DECAPOD".to_string(),
            overridability: DirectiveOverridability::Overridable,
            visibility: DirectiveVisibility::Shared,
            body: "Stale org control.".to_string(),
            required_terms: vec![],
            prohibited_weakening_terms: vec![],
            rationale: None,
        }],
        1_000_000_000,
        Some(1_000_000_100), // expired well in the past
        false,
    );
    init_minimal_repo(
        tmp_stale.path(),
        &format!(
            "[context.organization_overlay]\npath = \".decapod/org.json\"\norg_id = \"org_stale\"\npublic_key_hex = \"{}\"\n",
            pk_s
        ),
    );
    fs::write(
        tmp_stale.path().join(".decapod/org.json"),
        serde_json::to_vec_pretty(&bundle_s).unwrap(),
    )
    .unwrap();

    let status_out = run_decapod(
        tmp_stale.path(),
        &["constitution", "overlay", "status", "--format", "json"],
        None,
    );
    assert!(status_out.status.success());
    let status_json: Value = serde_json::from_slice(&status_out.stdout).unwrap();
    assert_eq!(
        status_json["organization_overlay"]["freshness_state"],
        "stale"
    );

    let stale_verify = run_decapod(
        tmp_stale.path(),
        &["constitution", "overlay", "verify"],
        None,
    );
    assert!(!stale_verify.status.success());
    assert!(String::from_utf8_lossy(&stale_verify.stderr).contains("ORG_OVERLAY_STALE"));

    // 7. tampered: digest or signature mismatch fails closed
    let tmp_tamper = tempdir().unwrap();
    let (mut bundle_t, pk_t) = make_signed_bundle(
        "org_tamper",
        vec![OrganizationDirective {
            directive_id: "core/DECAPOD".to_string(),
            overridability: DirectiveOverridability::Overridable,
            visibility: DirectiveVisibility::Shared,
            body: "Authentic rule.".to_string(),
            required_terms: vec![],
            prohibited_weakening_terms: vec![],
            rationale: None,
        }],
        1_700_000_000,
        Some(4_000_000_000),
        false,
    );
    init_minimal_repo(
        tmp_tamper.path(),
        &format!(
            "[context.organization_overlay]\npath = \".decapod/org.json\"\norg_id = \"org_tamper\"\npublic_key_hex = \"{}\"\n",
            pk_t
        ),
    );
    bundle_t.directives[0].body = "Attacker mutated rule.".to_string();
    fs::write(
        tmp_tamper.path().join(".decapod/org.json"),
        serde_json::to_vec_pretty(&bundle_t).unwrap(),
    )
    .unwrap();
    let tamper_rpc = run_decapod(
        tmp_tamper.path(),
        &["rpc", "--op", "context.resolve"],
        Some(r#"{"op":"context.resolve","params":{"query":"DECAPOD"}}"#),
    );
    assert!(!tamper_rpc.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&tamper_rpc.stdout),
        String::from_utf8_lossy(&tamper_rpc.stderr)
    );
    assert!(
        combined.contains("ORG_OVERLAY_TAMPERED"),
        "expected ORG_OVERLAY_TAMPERED, got: {combined}"
    );

    // 8. offline + portable bundle redaction of organization_private policy
    let tmp_offline = tempdir().unwrap();
    let secret_policy_text = "ORG_SECRET_INTERNAL_TOKEN_ROTATION_KEY_DO_NOT_LEAK_42";
    let (bundle_off, pk_off) = make_signed_bundle(
        "org_offline",
        vec![OrganizationDirective {
            directive_id: "core/DECAPOD".to_string(),
            overridability: DirectiveOverridability::NonOverridable,
            visibility: DirectiveVisibility::OrganizationPrivate,
            body: format!("Private org control: {secret_policy_text}."),
            required_terms: vec![],
            prohibited_weakening_terms: vec![],
            rationale: Some("Internal incident procedure".to_string()),
        }],
        1_700_000_000,
        Some(4_000_000_000),
        true,
    );
    init_minimal_repo(
        tmp_offline.path(),
        &format!(
            "[context.organization_overlay]\norg_id = \"org_offline\"\nsha256 = \"{}\"\npublic_key_hex = \"{}\"\nfreshness_policy = \"offline_cached\"\n",
            bundle_off.bundle_sha256, pk_off
        ),
    );
    let external_bundle = tmp_offline.path().join("external-bundle.json");
    fs::write(
        &external_bundle,
        serde_json::to_vec_pretty(&bundle_off).unwrap(),
    )
    .unwrap();

    // Importing an unsigned/tampered copy fails closed; importing the signed bundle succeeds.
    let imported = import_overlay_bundle(tmp_offline.path(), &external_bundle).expect("import ok");
    assert_eq!(imported.trust_state, OverlayTrustState::VerifiedSignature);
    assert_eq!(
        imported.freshness_state,
        OverlayFreshnessState::OfflineCached
    );
    assert!(imported.offline_usable);

    // Tampered import bundle fails closed and does not replace valid cache.
    let tampered_external = tmp_offline.path().join("tampered-external.json");
    let mut tampered_bundle = bundle_off.clone();
    tampered_bundle.directives[0].body = "Malicious unverified modification".to_string();
    fs::write(
        &tampered_external,
        serde_json::to_vec_pretty(&tampered_bundle).unwrap(),
    )
    .unwrap();
    let import_err = import_overlay_bundle(tmp_offline.path(), &tampered_external).unwrap_err();
    assert!(import_err.to_string().contains("ORG_OVERLAY_TAMPERED"));

    // Delete external source file to simulate fully disconnected offline repo.
    fs::remove_file(&external_bundle).unwrap();
    let loaded_offline = load_organization_overlay(tmp_offline.path()).expect("load offline");
    assert!(loaded_offline.status.present);
    assert!(loaded_offline.status.offline_usable);
    assert_eq!(
        loaded_offline.status.freshness_state,
        OverlayFreshnessState::OfflineCached
    );

    // Build portable context bundle: MUST preserve provenance hash/directive_id WITHOUT leaking private policy body!
    let capsule = query_embedded_capsule(
        tmp_offline.path(),
        "DECAPOD",
        "core",
        Some("task_off"),
        None,
        4,
    )
    .expect("capsule offline");
    let portable = build_bundle(
        tmp_offline.path(),
        capsule,
        None,
        vec![],
        vec![secret_policy_text.to_string()],
    )
    .expect("portable bundle with redacted org_private content");
    let serialized = serde_json::to_string(&portable).expect("serialize portable");
    assert!(
        !serialized.contains(secret_policy_text),
        "organization_private body leaked into portable context bundle: {serialized}"
    );
    validate_bundle(tmp_offline.path(), &portable).expect("redacted bundle validates");
    let bundle_chain = portable
        .capsule
        .authority_chain
        .as_ref()
        .expect("authority_chain preserved in portable bundle");
    assert!(bundle_chain.directives[0].redacted_for_portability);

    assert!(clear_imported_overlay(tmp_offline.path()).expect("clear imported"));
}

#[test]
fn issue_877_candidate_downgrade_and_layer4_non_authority_enforced() {
    let tmp = tempdir().unwrap();
    let (bundle, pk) = make_signed_bundle(
        "org_guard",
        vec![
            OrganizationDirective {
                directive_id: "core/DECAPOD".to_string(),
                overridability: DirectiveOverridability::NonOverridable,
                visibility: DirectiveVisibility::Shared,
                body: "Org mandate: retain signed verification.".to_string(),
                required_terms: vec![],
                prohibited_weakening_terms: vec![],
                rationale: None,
            },
            OrganizationDirective {
                directive_id: "core/DEMANDS".to_string(),
                overridability: DirectiveOverridability::ExtendOnly,
                visibility: DirectiveVisibility::Shared,
                body: "Org extend_only: preserve proof traceability.".to_string(),
                required_terms: vec!["traceability".to_string()],
                prohibited_weakening_terms: vec![],
                rationale: None,
            },
        ],
        1_700_000_000,
        Some(4_000_000_000),
        false,
    );
    init_minimal_repo(
        tmp.path(),
        &format!(
            "[context.organization_overlay]\npath = \".decapod/org.json\"\norg_id = \"org_guard\"\nsha256 = \"{}\"\npublic_key_hex = \"{}\"\n",
            bundle.bundle_sha256, pk
        ),
    );
    fs::write(
        tmp.path().join(".decapod/org.json"),
        serde_json::to_vec_pretty(&bundle).unwrap(),
    )
    .unwrap();
    let git = |dir: &Path, args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(tmp.path(), &["add", ".decapod/org.json"]);
    git(tmp.path(), &["commit", "-m", "chore: commit org overlay"]);

    // Verify ExtendOnly weakening fails closed through CLI `constitution overlay verify`.
    write_override(
        tmp.path(),
        &[(
            "core/DEMANDS",
            "Project amendment: waive traceability checks for experiments.",
        )],
    );
    let extend_fail = run_decapod(tmp.path(), &["constitution", "overlay", "verify"], None);
    assert!(!extend_fail.status.success());
    assert!(
        String::from_utf8_lossy(&extend_fail.stderr)
            .contains("ORG_OVERLAY_EXTEND_ONLY_WEAKENING_REJECTED")
    );
    fs::remove_file(tmp.path().join(".decapod/OVERRIDE.md")).unwrap();

    // `constitution get core/DECAPOD --resolved` exposes layer provenance and org content.
    let get_resolved = run_decapod(
        tmp.path(),
        &["constitution", "get", "core/DECAPOD", "--resolved"],
        None,
    );
    assert!(
        get_resolved.status.success(),
        "get --resolved failed: {}",
        String::from_utf8_lossy(&get_resolved.stderr)
    );
    let get_json: Value = serde_json::from_slice(&get_resolved.stdout).unwrap();
    assert_eq!(
        get_json["authority_chain"]["organization_overlay"]["org_id"],
        "org_guard"
    );
    assert!(
        get_json["merged_markdown"]
            .as_str()
            .unwrap_or("")
            .contains("Org mandate: retain signed verification.")
    );

    // Layer 4 task capsule artifacts cannot claim authoritative status over Layers 1-3.
    let res = resolve_authority_chain_with_now(tmp.path(), 1_750_000_000).unwrap();
    let layer4 = res
        .chain
        .layers
        .iter()
        .find(|l| l.layer == "task_context_projection")
        .expect("layer 4 summary");
    assert!(!layer4.authoritative);
    assert!(res.chain.task_projection_isolation_enforced);

    // Candidate worktree dropping accepted [context.organization_overlay] fails validation.
    let wt_dir = tmp.path().join(".decapod/workspaces/cand-877");
    fs::create_dir_all(tmp.path().join(".decapod/workspaces")).unwrap();
    git(
        tmp.path(),
        &[
            "worktree",
            "add",
            "-b",
            "agent/test/877",
            wt_dir.to_str().unwrap(),
        ],
    );
    let cand_cfg = fs::read_to_string(wt_dir.join(".decapod/config.toml")).unwrap();
    let stripped_cfg = cand_cfg
        .split("[context.organization_overlay]")
        .next()
        .unwrap()
        .to_string();
    fs::write(wt_dir.join(".decapod/config.toml"), stripped_cfg).unwrap();
    let validate_err = decapod::core::validate::validate_project_config_downgrade_check(&wt_dir);
    assert!(validate_err.is_err());
    assert!(
        validate_err
            .unwrap_err()
            .to_string()
            .contains("ORG_OVERLAY_POLICY_DOWNGRADE")
    );
}
