//! CI-only nonpublishing entrypoint. Requests must come from the reviewed planner;
//! parsing a request file does not authorize untrusted tool paths or source code.
use decapod::core::{
    bounded_process::Cancellation,
    derived_release_driver::{self, DriverInputs, DriverLimits, Stage},
};
use std::{collections::BTreeMap, io, path::PathBuf};
fn invalid(s: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, s.into())
}
#[path = "derived_release_aggregate.rs"]
mod aggregate;
fn main() {
    if let Err(error) = run() {
        eprintln!("Derived release proof failed: {error}");
        std::process::exit(1);
    }
}
fn run() -> io::Result<()> {
    let raw: Vec<_> = std::env::args().skip(1).collect();
    if raw.first().is_some_and(|a| a == "--aggregate") {
        if raw.len() != 2 {
            return Err(invalid("aggregate requires one fixed artifact directory"));
        }
        return aggregate::run(&PathBuf::from(&raw[1]));
    }
    let mut args = raw.into_iter();
    let mut values = BTreeMap::new();
    while let Some(key) = args.next() {
        let value = args.next().ok_or_else(|| invalid("missing option value"))?;
        if !matches!(
            key.as_str(),
            "--request" | "--scratch" | "--stage" | "--target" | "--transport-root"
        ) || values.insert(key, value).is_some()
        {
            return Err(invalid("unexpected or duplicate option"));
        }
    }
    let take = |key: &str| {
        values
            .get(key)
            .cloned()
            .ok_or_else(|| invalid(format!("missing {key}")))
    };
    let request_path = PathBuf::from(take("--request")?);
    let metadata = std::fs::symlink_metadata(&request_path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024 {
        return Err(invalid("request must be a bounded regular file"));
    }
    let inputs: DriverInputs = serde_json::from_slice(&std::fs::read(request_path)?)
        .map_err(|e| invalid(e.to_string()))?;
    let scratch = PathBuf::from(take("--scratch")?).canonicalize()?;
    let stage = match take("--stage")?.as_str() {
        "prepare" => None,
        "plan" => Some(Stage::Plan),
        "package" => Some(Stage::Package),
        "local" => Some(Stage::Local {
            target: take("--target")?,
        }),
        "global" => Some(Stage::Global),
        "candidate" => Some(Stage::Candidate),
        _ => return Err(invalid("unknown proof stage")),
    };
    if values.contains_key("--target") && !matches!(stage, Some(Stage::Local { .. })) {
        return Err(invalid("only local stage accepts a target"));
    }
    let cancel = Cancellation::default();
    let mut prepared = if let Some(root) = values.get("--transport-root") {
        let root = PathBuf::from(root);
        let metadata = std::fs::symlink_metadata(root.join("transport.json"))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024
        {
            return Err(invalid("invalid transport receipt size/type"));
        }
        let receipt: derived_release_driver::ReleaseTransport =
            serde_json::from_slice(&std::fs::read(root.join("transport.json"))?)
                .map_err(|e| invalid(e.to_string()))?;
        derived_release_driver::import_release(
            &inputs,
            &scratch,
            DriverLimits::default(),
            &cancel,
            &root.join("derived.bundle"),
            &receipt,
        )?
    } else {
        if stage.is_some() {
            return Err(invalid(
                "execution stages require the single prepared transport",
            ));
        }
        derived_release_driver::prepare(&inputs, &scratch, DriverLimits::default(), &cancel)?
    };
    let report = stage.map(|stage| prepared.run(stage)).transpose()?;
    if let Some(report) = &report {
        prepared.export_artifacts(report, &scratch.join("release-artifacts"))?;
        prepared.export_evidence(&scratch.join("evidence"))?;
    } else {
        if values.contains_key("--transport-root") {
            return Err(invalid("cannot prepare an imported derivative"));
        }
        // Resolve the real production plan before matrix jobs are admitted.
        // The graph adapter compares its exported target/runner inventory with
        // both the source dist configuration and the finite workflow matrix.
        prepared.run(Stage::Plan)?;
        prepared.export_transport(&scratch.join("transport"))?;
    }
    let output = serde_json::json!({"schema":"derived-release-execution/2","generation_binding":prepared.binding,
        "generated":prepared.generated,"tool_setup_sha256":prepared.tool_setup_sha256,
        "preparation_records":prepared.preparation_records,"stage":report});
    let output_path = scratch.join("derived-release-proof.json");
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&output_path)?;
    serde_json::to_writer_pretty(&mut f, &output)?;
    f.write_all(b"\n")?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
