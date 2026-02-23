use std::fs;
use std::path::Path;

pub const LOCAL_PROJECT_SPECS_DIR: &str = ".decapod/generated/specs";
pub const LOCAL_PROJECT_SPECS_README: &str = ".decapod/generated/specs/README.md";
pub const LOCAL_PROJECT_SPECS_INTENT: &str = ".decapod/generated/specs/intent.md";
pub const LOCAL_PROJECT_SPECS_ARCHITECTURE: &str = ".decapod/generated/specs/architecture.md";
pub const LOCAL_PROJECT_SPECS_INTERFACES: &str = ".decapod/generated/specs/interfaces.md";
pub const LOCAL_PROJECT_SPECS_VALIDATION: &str = ".decapod/generated/specs/validation.md";

#[derive(Clone, Copy, Debug)]
pub struct LocalProjectSpec {
    pub path: &'static str,
    pub role: &'static str,
    pub constitution_ref: &'static str,
}

pub const LOCAL_PROJECT_SPECS: &[LocalProjectSpec] = &[
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_README,
        role: "specs_index",
        constitution_ref: "interfaces/PROJECT_SPECS.md#Canonical Local Project Specs Set",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_INTENT,
        role: "intent_purpose",
        constitution_ref: "specs/INTENT.md",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_ARCHITECTURE,
        role: "implementation_architecture",
        constitution_ref: "interfaces/ARCHITECTURE_FOUNDATIONS.md",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_INTERFACES,
        role: "service_contracts",
        constitution_ref: "interfaces/CONTROL_PLANE.md",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_VALIDATION,
        role: "proof_and_gate_plan",
        constitution_ref: "interfaces/TESTING.md",
    },
];

#[derive(Debug, Clone, Default)]
pub struct LocalProjectSpecsContext {
    pub intent: Option<String>,
    pub architecture: Option<String>,
    pub interfaces: Option<String>,
    pub validation: Option<String>,
    pub canonical_paths: Vec<String>,
    pub constitution_refs: Vec<String>,
    pub update_guidance: String,
}

fn read_if_exists(project_root: &Path, rel_path: &str) -> Option<String> {
    let path = project_root.join(rel_path);
    if !path.exists() {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn first_meaningful_line(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('-'))
        .map(|s| s.to_string())
}

pub fn local_project_specs_context(project_root: &Path) -> LocalProjectSpecsContext {
    let mut ctx = LocalProjectSpecsContext::default();
    for spec in LOCAL_PROJECT_SPECS {
        ctx.canonical_paths.push(spec.path.to_string());
        ctx.constitution_refs
            .push(spec.constitution_ref.to_string());
    }
    ctx.constitution_refs.sort();
    ctx.constitution_refs.dedup();

    ctx.intent = read_if_exists(project_root, LOCAL_PROJECT_SPECS_INTENT)
        .and_then(|s| first_meaningful_line(&s));
    ctx.architecture = read_if_exists(project_root, LOCAL_PROJECT_SPECS_ARCHITECTURE)
        .and_then(|s| first_meaningful_line(&s));
    ctx.interfaces = read_if_exists(project_root, LOCAL_PROJECT_SPECS_INTERFACES)
        .and_then(|s| first_meaningful_line(&s));
    ctx.validation = read_if_exists(project_root, LOCAL_PROJECT_SPECS_VALIDATION)
        .and_then(|s| first_meaningful_line(&s));
    ctx.update_guidance = "Treat .decapod/generated/specs/*.md as living project contracts: when user intent, interfaces, architecture, or proof gates change, update these specs before implementation proceeds.".to_string();
    ctx
}
