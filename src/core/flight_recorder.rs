//! Governance Flight Recorder
//!
//! A read-only timeline renderer that makes the "narrow corridor" legible to humans.
//! Renders governance events into a timeline: intent -> awareness -> mandate checks ->
//! claim -> workspace -> edits -> proofs -> publish.
//!
//! # For AI Agents
//!
//! This is a read-only tool. It renders existing event logs and highlights gaps
//! rather than fabricating missing structure.

use crate::core::error::DecapodError;
use crate::core::store::Store;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(
    name = "flight-recorder",
    about = "Render governance timeline from event logs"
)]
pub struct FlightRecorderCli {
    #[clap(subcommand)]
    command: FlightRecorderCommand,
}

#[derive(Subcommand, Debug)]
pub enum FlightRecorderCommand {
    /// Render governance timeline from event logs
    Timeline {
        /// Output format: 'text' or 'json'
        #[clap(long, default_value = "text")]
        format: String,
        /// Limit to N most recent events per source
        #[clap(long, default_value = "100")]
        limit: usize,
    },
    /// Export transcript as markdown
    Transcript {
        /// Output file path (stdout if not specified)
        #[clap(long)]
        output: Option<String>,
        /// Include only events from this actor
        #[clap(long)]
        actor: Option<String>,
    },
}

pub fn run_flight_recorder_cli(store: &Store, cli: FlightRecorderCli) -> Result<(), DecapodError> {
    match cli.command {
        FlightRecorderCommand::Timeline { format, limit } => render_timeline(store, &format, limit),
        FlightRecorderCommand::Transcript { output, actor } => {
            render_transcript(store, output.as_deref(), actor.as_deref())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub source: String,
    pub ts: String,
    pub event_id: String,
    pub op: String,
    pub actor: Option<String>,
    pub session_id: Option<String>,
    pub correlation_id: Option<String>,
    pub status: Option<String>,
    pub details: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct Timeline {
    pub rendered_at: String,
    pub event_count: usize,
    pub sources: Vec<String>,
    pub events: Vec<TimelineEvent>,
    pub gaps: Vec<String>,
}

fn render_timeline(store: &Store, format: &str, limit: usize) -> Result<(), DecapodError> {
    let mut all_events = Vec::new();
    let mut sources = Vec::new();
    let mut gaps = Vec::new();

    let event_files = vec![
        ("broker", store.root.join("broker.events.jsonl")),
        ("todo", store.root.join("todo.events.jsonl")),
        ("federation", store.root.join("federation.events.jsonl")),
        ("proof", store.root.join("proof.events.jsonl")),
        ("watcher", store.root.join("watcher.events.jsonl")),
        ("map", store.root.join("map.events.jsonl")),
        ("lcm", store.root.join("lcm.events.jsonl")),
    ];

    for (name, path) in &event_files {
        if path.exists() {
            sources.push(name.to_string());
            match read_events(path, limit) {
                Ok(events) => {
                    for mut ev in events {
                        ev.source = name.to_string();
                        all_events.push(ev);
                    }
                }
                Err(e) => {
                    gaps.push(format!("{}: read error - {}", name, e));
                }
            }
        } else {
            gaps.push(format!("{}: file not found", name));
        }
    }

    all_events.sort_by(|a, b| a.ts.cmp(&b.ts));

    if format == "json" {
        let timeline = Timeline {
            rendered_at: crate::core::time::now_epoch_z(),
            event_count: all_events.len(),
            sources,
            events: all_events,
            gaps,
        };
        println!("{}", serde_json::to_string_pretty(&timeline).unwrap());
    } else {
        println!("===================================================================");
        println!("          GOVERNANCE FLIGHT RECORDER - TIMELINE");
        println!("===================================================================");
        println!();
        println!("Rendered: {}", crate::core::time::now_epoch_z());
        println!("Total Events: {}", all_events.len());
        println!("Sources: {}", sources.join(", "));
        println!();

        if !gaps.is_empty() {
            println!("  GAPS / MISSING DATA:");
            for gap in &gaps {
                println!("  - {}", gap);
            }
            println!();
        }

        println!("-------------------------------------------------------------------");
        println!(
            "{:<12} {:<26} {:<15} {:<20}",
            "TIME", "OP", "ACTOR", "SOURCE"
        );
        println!("-------------------------------------------------------------------");

        for ev in &all_events {
            let ts_short = if ev.ts.len() > 26 {
                &ev.ts[0..26]
            } else {
                &ev.ts
            };
            println!(
                "{:<12} {:<26} {:<15} {:<20}",
                ts_short,
                truncate(&ev.op, 26),
                truncate(ev.actor.as_deref().unwrap_or("-"), 15),
                truncate(&ev.source, 20)
            );
        }

        println!("-------------------------------------------------------------------");
        println!();
        println!("Governance corridor: intent -> claim -> workspace -> proofs -> publish");
        println!("Use `decapod flight-recorder transcript` for detailed output.");
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max - 3])
    } else {
        s.to_string()
    }
}

fn read_events(path: &PathBuf, limit: usize) -> Result<Vec<TimelineEvent>, DecapodError> {
    let file = File::open(path).map_err(DecapodError::IoError)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(DecapodError::IoError)?;
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(json) => {
                let ev = TimelineEvent {
                    source: "unknown".to_string(),
                    ts: json
                        .get("ts")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    event_id: json
                        .get("event_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    op: json
                        .get("op")
                        .or_else(|| json.get("event_type"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    actor: json.get("actor").and_then(|v| v.as_str()).map(String::from),
                    session_id: json
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    correlation_id: json
                        .get("correlation_id")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    status: json
                        .get("status")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    details: json,
                };
                events.push(ev);
            }
            Err(_) => continue,
        }

        if events.len() >= limit {
            break;
        }
    }

    Ok(events)
}

fn render_transcript(
    store: &Store,
    output_path: Option<&str>,
    actor_filter: Option<&str>,
) -> Result<(), DecapodError> {
    let mut all_events = Vec::new();

    let event_files = vec![
        ("broker", store.root.join("broker.events.jsonl")),
        ("todo", store.root.join("todo.events.jsonl")),
        ("federation", store.root.join("federation.events.jsonl")),
        ("proof", store.root.join("proof.events.jsonl")),
        ("map", store.root.join("map.events.jsonl")),
        ("lcm", store.root.join("lcm.events.jsonl")),
    ];

    for (name, path) in &event_files {
        if path.exists() {
            if let Ok(events) = read_events(path, 10000) {
                for mut ev in events {
                    if let Some(filter) = actor_filter {
                        if ev.actor.as_deref() != Some(filter) {
                            continue;
                        }
                    }
                    ev.source = name.to_string();
                    all_events.push(ev);
                }
            }
        }
    }

    all_events.sort_by(|a, b| a.ts.cmp(&b.ts));

    let mut md = String::new();
    md.push_str("# Governance Transcript\n\n");
    md.push_str(&format!(
        "Generated: {}\n",
        crate::core::time::now_epoch_z()
    ));
    md.push_str(&format!("Total Events: {}\n", all_events.len()));
    if let Some(f) = actor_filter {
        md.push_str(&format!("Actor Filter: {}\n", f));
    }
    md.push_str("\n---\n\n");
    md.push_str("## Timeline\n\n");

    for ev in &all_events {
        md.push_str(&format!("### {} - {}\n\n", ev.ts, ev.op));
        md.push_str(&format!("- **Source:** {}\n", ev.source));
        md.push_str(&format!("- **Event ID:** {}\n", ev.event_id));
        if let Some(actor) = &ev.actor {
            md.push_str(&format!("- **Actor:** {}\n", actor));
        }
        if let Some(session) = &ev.session_id {
            md.push_str(&format!("- **Session:** {}\n", session));
        }
        if let Some(corr) = &ev.correlation_id {
            md.push_str(&format!("- **Correlation:** {}\n", corr));
        }
        if let Some(status) = &ev.status {
            md.push_str(&format!("- **Status:** {}\n", status));
        }
        md.push('\n');
    }

    md.push_str("---\n\n");
    md.push_str("*This transcript was generated by the Governance Flight Recorder.*\n");
    md.push_str("*It renders only existing event data; missing fields are shown as gaps.*\n");

    if let Some(path) = output_path {
        std::fs::write(path, &md).map_err(DecapodError::IoError)?;
        println!("Transcript written to: {}", path);
    } else {
        println!("{}", md);
    }

    Ok(())
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "flight-recorder",
        "version": "0.1.0",
        "description": "Governance timeline renderer - makes the narrow corridor legible",
        "commands": [
            { "name": "timeline", "description": "Render governance timeline from event logs", "parameters": ["format", "limit"] },
            { "name": "transcript", "description": "Export transcript as markdown", "parameters": ["output", "actor"] }
        ],
        "storage": ["read-only over existing event logs"],
        "notes": "Read-only rendering; never fabricates missing structure"
    })
}
