// Moved from src/decapod/lib.rs
use super::*;

fn selector_result_for_input(options: &[&str], default: &[String], input: &[u8]) -> String {
    let mut selected = default
        .first()
        .and_then(|d| {
            options
                .iter()
                .position(|option| d.eq_ignore_ascii_case(option))
        })
        .unwrap_or(0);
    let mut typed = String::new();
    let mut index = 0;
    while index < input.len() {
        match input[index] {
            b'\r' | b'\n' => return selector_shown(options, selected, &typed),
            27 if input.get(index + 1) == Some(&b'[') => {
                match input.get(index + 2).copied() {
                    Some(b'A') => {
                        typed.clear();
                        selected = selected.checked_sub(1).unwrap_or_else(|| options.len() - 1);
                    }
                    Some(b'B') => {
                        typed.clear();
                        selected = (selected + 1) % options.len();
                    }
                    _ => {}
                }
                index += 3;
                continue;
            }
            byte => update_selector_from_byte(options, &mut selected, &mut typed, byte),
        }
        index += 1;
    }
    selector_shown(options, selected, &typed)
}

fn selector_render_for_input(
    options: &[&str],
    descriptions: Option<&[&str]>,
    default: &[String],
    input: &[u8],
) -> Vec<String> {
    let mut selected = selector_default_index(options, default).unwrap_or(0);
    let mut typed = String::new();
    let mut index = 0;
    while index < input.len() {
        match input[index] {
            b'\r' | b'\n' => break,
            27 if input.get(index + 1) == Some(&b'[') => {
                match input.get(index + 2).copied() {
                    Some(b'A') => {
                        typed.clear();
                        selected = selected.checked_sub(1).unwrap_or_else(|| options.len() - 1);
                    }
                    Some(b'B') => {
                        typed.clear();
                        selected = (selected + 1) % options.len();
                    }
                    _ => {}
                }
                index += 3;
                continue;
            }
            byte => update_selector_from_byte(options, &mut selected, &mut typed, byte),
        }
        index += 1;
    }
    selector_render_lines(
        options,
        descriptions,
        selected,
        selector_default_index(options, default),
        &typed,
        "    choice: ",
    )
}
use tempfile::tempdir;

#[test]
fn arrow_keys_move_selection_without_entering_input_text() {
    let default = vec!["Rust".to_string()];
    let selected = selector_result_for_input(LANGUAGES, &default, b"\x1b[B\x1b[B\x1b[A\n");

    assert_eq!(selected, "TypeScript");
    assert!(!selected.contains("\x1b"));
    assert!(!selected.contains("[B"));
}

#[test]
fn selector_render_tracks_current_selection() {
    let default = vec!["Python".to_string()];

    let default_lines = selector_render_for_input(LANGUAGES, None, &default, b"\n");
    assert_eq!(default_lines[0], "    choice: Python");
    assert!(
        default_lines
            .iter()
            .any(|line| line == "    > ✓  4. Python")
    );

    let typed_lines = selector_render_for_input(LANGUAGES, None, &default, b"go\n");
    assert_eq!(typed_lines[0], "    choice: go");
    assert!(typed_lines.iter().any(|line| line == "    >    5. Go"));

    let custom_lines = selector_render_for_input(LANGUAGES, None, &default, b"not-a-language\n");
    assert_eq!(custom_lines[0], "    choice: not-a-language");
}

#[test]
fn language_selector_limits_visible_options_and_marks_more_below() {
    let default = vec!["Rust".to_string()];

    let lines = selector_render_for_input(LANGUAGES, None, &default, b"\n");
    let option_lines = lines
        .iter()
        .filter(|line| line.contains(". "))
        .collect::<Vec<_>>();

    assert_eq!(option_lines.len(), SELECTOR_VISIBLE_OPTIONS);
    assert!(lines.iter().any(|line| line == "    ↓ more"));
    assert!(
        !lines
            .iter()
            .any(|line| line.contains(" 11. ") || line.contains(" 30. "))
    );
}

#[test]
fn language_selector_wraps_from_bottom_to_top() {
    let default = vec!["Other".to_string()];
    let selected = selector_result_for_input(LANGUAGES, &default, b"\x1b[B\n");

    assert_eq!(selected, "Rust");
}

#[test]
fn language_selector_shows_wrap_hint_at_bottom() {
    let default = vec!["Other".to_string()];

    let lines = selector_render_for_input(LANGUAGES, None, &default, b"\n");

    assert!(lines.iter().any(|line| line == "    ↑ more"));
    assert!(lines.iter().any(|line| line == "    ↓ wraps to 1"));
    assert!(lines.iter().any(|line| line == "    > ✓ 30. Other"));
}

#[test]
fn language_selector_numeric_typing_moves_selection_into_view() {
    let default = vec!["Rust".to_string()];

    let lines = selector_render_for_input(LANGUAGES, None, &default, b"30\n");

    assert_eq!(lines[0], "    choice: 30");
    assert!(lines.iter().any(|line| line == "    >   30. Other"));
    assert!(!lines.iter().any(|line| line.contains("  1. Rust")));
}

#[test]
fn language_selector_text_typing_moves_selection_into_view() {
    let default = vec!["Rust".to_string()];

    let lines = selector_render_for_input(LANGUAGES, None, &default, b"powershell\n");

    assert_eq!(lines[0], "    choice: powershell");
    assert!(lines.iter().any(|line| line == "    >   29. PowerShell"));
    assert!(!lines.iter().any(|line| line.contains("  1. Rust")));
}

#[test]
fn enter_accepts_inferred_default_language() {
    let default = vec!["Python".to_string()];
    let selected = selector_result_for_input(LANGUAGES, &default, b"\n");

    assert_eq!(selected, "Python");
}

#[test]
fn numeric_language_selection_targets_numbered_option() {
    let default = vec!["Rust".to_string()];
    let selected = selector_result_for_input(LANGUAGES, &default, b"4\n");

    assert_eq!(selected, "Python");
    assert_eq!(parse_language_choice(&selected), vec!["Python".to_string()]);
}

#[test]
fn typed_language_selection_targets_matching_language() {
    let default = vec!["Rust".to_string()];
    let selected = selector_result_for_input(LANGUAGES, &default, b"python\n");

    assert_eq!(selected, "Python");
    assert_eq!(parse_language_choice("python"), vec!["Python".to_string()]);
}

#[test]
fn selector_render_shows_one_navigable_option_list() {
    let default = vec!["cli".to_string()];
    let options = ARCH_DIRECTIONS
        .iter()
        .map(|(arch, _)| *arch)
        .collect::<Vec<_>>();
    let descriptions = ARCH_DIRECTIONS
        .iter()
        .map(|(_, description)| *description)
        .collect::<Vec<_>>();

    let lines = selector_render_for_input(&options, Some(&descriptions), &default, b"\x1b[B\n");

    assert_eq!(lines[0], "    choice: lambda");
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("lambda -> Lambda/Serverless"))
            .count(),
        1
    );
    assert!(lines.iter().any(|line| line.starts_with("    >")));
}

#[test]
fn diagram_notation_selector_uses_terminal_readable_options() {
    let default = vec![String::from("ascii")];

    let lines = selector_render_for_input(
        DIAGRAM_NOTATION_OPTIONS,
        Some(DIAGRAM_NOTATION_DESCRIPTIONS),
        &default,
        b"\x1b[B\n",
    );

    assert_eq!(lines[0], "    choice: mermaid");
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("ascii -> ASCII/text blocks"))
            .count(),
        1
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("mermaid -> Mermaid diagrams"))
    );
}

#[test]
fn diagram_notation_choice_accepts_text_aliases() {
    assert_eq!(
        parse_diagram_style_choice("", InitDiagramStyle::Mermaid).unwrap(),
        InitDiagramStyle::Mermaid
    );
    assert_eq!(
        parse_diagram_style_choice("text", InitDiagramStyle::Mermaid).unwrap(),
        InitDiagramStyle::Ascii
    );
    assert_eq!(
        parse_diagram_style_choice("ascii/text", InitDiagramStyle::Mermaid).unwrap(),
        InitDiagramStyle::Ascii
    );
    assert_eq!(
        parse_diagram_style_choice("2", InitDiagramStyle::Ascii).unwrap(),
        InitDiagramStyle::Mermaid
    );
    assert!(parse_diagram_style_choice("plantuml", InitDiagramStyle::Ascii).is_err());
}

#[test]
fn comma_separated_language_selection_is_preserved() {
    assert_eq!(
        parse_language_choice("4, shell, typescript"),
        vec![
            "Python".to_string(),
            "Shell".to_string(),
            "TypeScript".to_string()
        ]
    );
}

#[test]
fn line_prompt_escape_backstop_strips_raw_ansi_sequences() {
    assert_eq!(
        strip_ansi_escape_sequences("^[[B\x1b[Bpython\x1b[A"),
        "^[[Bpython"
    );
}

#[test]
fn inferred_language_wins_over_architecture_recommendation_for_default() {
    assert_eq!(
        language_choice_seed(&["Python".to_string()], &["Rust".to_string()]),
        vec!["Python".to_string()]
    );
}

#[test]
fn mixed_scripts_repo_infers_multiple_languages_without_compiled_bias()
-> Result<(), error::DecapodError> {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("task.py"), "print('ok')\n").expect("python fixture");
    fs::write(tmp.path().join("deploy.sh"), "#!/usr/bin/env bash\n").expect("shell fixture");
    fs::write(tmp.path().join("env.zsh"), "printenv\n").expect("zsh fixture");
    fs::write(tmp.path().join("tool.ts"), "export const ok = true;\n").expect("ts fixture");
    fs::write(tmp.path().join("probe.go"), "package main\n").expect("go fixture");

    let ctx = infer_repo_context(tmp.path())?;

    assert!(ctx.primary_languages.contains(&"go".to_string()));
    assert!(ctx.primary_languages.contains(&"python".to_string()));
    assert!(ctx.primary_languages.contains(&"shell".to_string()));
    assert!(ctx.primary_languages.contains(&"typescript".to_string()));
    assert_ne!(ctx.primary_languages, vec!["rust".to_string()]);
    Ok(())
}

#[test]
fn test_branch_contains_todo_ticket_id() {
    assert!(branch_contains_todo_ticket_id(
        "agent/unknown/bugs_01kvtvsvteg1t4ds"
    ));
    assert!(branch_contains_todo_ticket_id(
        "agent/unknown/bugs-01kvtvsvteg1t4ds"
    ));
    assert!(branch_contains_todo_ticket_id(
        "agent/unknown/feat-01kvtvsvteg1t4ds"
    ));
    assert!(branch_contains_todo_ticket_id(
        "agent/unknown/todo-01kvtr-plus-2-1782239277"
    ));
    assert!(!branch_contains_todo_ticket_id(
        "agent/unknown/some-feature-branch"
    ));
}

#[test]
fn dockerfile_orientation_separates_workspace_and_application_containers() {
    let mut packet = crate::core::rpc::OrientationPacket {
        user_goal: "Dockerfile packaging".to_string(),
        task_id: None,
        governed_plan: crate::core::rpc::GovernedPlanContext {
            status: "missing".to_string(),
            path: ".decapod/governance/plan.json".to_string(),
            title: None,
            intent: None,
            state: None,
            todo_ids: vec![],
            proof_hooks: vec![],
            unresolved_items: vec![],
            forbidden_paths: vec![],
            file_touch_budget: None,
            task_binding: "not_available".to_string(),
        },
        constraints: vec![],
        allowed_scope: vec![],
        forbidden_scope: vec![],
        relevant_areas: vec![],
        proof_required: vec![],
        known_unknowns: vec![],
        decision_gates: vec![],
        next_action: String::new(),
    };

    apply_container_orientation_constraints(&mut packet, "dockerfile packaging");

    assert!(
        packet
            .relevant_areas
            .contains(&"architecture/CONTAINERS".to_string())
    );
    assert!(
        packet
            .constraints
            .iter()
            .any(|constraint| constraint.contains("root Dockerfile must package"))
    );
    assert!(
        packet
            .proof_required
            .iter()
            .any(|proof| proof.contains("root Dockerfile"))
    );
}
