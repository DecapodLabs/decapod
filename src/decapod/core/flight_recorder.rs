//! Governance Flight Recorder
//!
//! A read-only timeline renderer that makes the "narrow corridor" legible to humans.
//! Renders governance events into a timeline: intent -> awareness -> mandate checks ->
//! claim -> workspace -> edits -> proofs -> publish.

use crate::core::error::DecapodError;
use crate::core::events;
use crate::core::store::Store;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

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
    let (all_events, sources, gaps) = read_timeline_events(store, limit);

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
                println!("  - {gap}");
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

fn read_timeline_events(
    store: &Store,
    limit: usize,
) -> (Vec<TimelineEvent>, Vec<String>, Vec<String>) {
    let mut all_events = Vec::new();
    let mut sources = Vec::new();
    let mut gaps = Vec::new();
    for (name, _) in events::STREAMS {
        match events::query(&store.root, name, limit) {
            Ok(records) => {
                if !records.is_empty() {
                    sources.push(name.to_string());
                }
                all_events.extend(records.into_iter().map(timeline_event));
            }
            Err(e) => gaps.push(format!("{name}: canonical read error - {e}")),
        }
    }
    all_events.sort_by(|a, b| a.ts.cmp(&b.ts));
    (all_events, sources, gaps)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max - 3])
    } else {
        s.to_string()
    }
}

fn timeline_event(record: events::StoredEvent) -> TimelineEvent {
    TimelineEvent {
        source: record.stream,
        ts: record.ts,
        event_id: record.event_id,
        op: record
            .payload
            .get("op")
            .or_else(|| record.payload.get("event_type"))
            .and_then(|value| value.as_str())
            .unwrap_or(&record.event_type)
            .to_string(),
        actor: Some(record.actor),
        session_id: record
            .payload
            .get("session_id")
            .and_then(|value| value.as_str())
            .map(String::from),
        correlation_id: record
            .payload
            .get("correlation_id")
            .and_then(|value| value.as_str())
            .map(String::from),
        status: record
            .payload
            .get("status")
            .and_then(|value| value.as_str())
            .map(String::from),
        details: record.payload,
    }
}

fn render_transcript(
    store: &Store,
    output_path: Option<&str>,
    actor_filter: Option<&str>,
) -> Result<(), DecapodError> {
    let mut all_events = Vec::new();

    for (name, _) in events::STREAMS {
        for ev in events::query(&store.root, name, 10000)? {
            let ev = timeline_event(ev);
            if let Some(filter) = actor_filter
                && ev.actor.as_deref() != Some(filter)
            {
                continue;
            }
            all_events.push(ev);
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
        md.push_str(&format!("Actor Filter: {f}\n"));
    }
    md.push_str("\n---\n\n");
    md.push_str("## Timeline\n\n");

    for ev in &all_events {
        md.push_str(&format!("### {} - {}\n\n", ev.ts, ev.op));
        md.push_str(&format!("- **Source:** {}\n", ev.source));
        md.push_str(&format!("- **Event ID:** {}\n", ev.event_id));
        if let Some(actor) = &ev.actor {
            md.push_str(&format!("- **Actor:** {actor}\n"));
        }
        if let Some(session) = &ev.session_id {
            md.push_str(&format!("- **Session:** {session}\n"));
        }
        if let Some(corr) = &ev.correlation_id {
            md.push_str(&format!("- **Correlation:** {corr}\n"));
        }
        if let Some(status) = &ev.status {
            md.push_str(&format!("- **Status:** {status}\n"));
        }
        md.push('\n');
    }

    md.push_str("---\n\n");
    md.push_str("*This transcript was generated by the Governance Flight Recorder.*\n");
    md.push_str("*It renders only existing event data; missing fields are shown as gaps.*\n");

    if let Some(path) = output_path {
        std::fs::write(path, &md).map_err(DecapodError::IoError)?;
        println!("Transcript written to: {path}");
    } else {
        println!("{md}");
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
        "storage": ["read-only over canonical SQLite event streams"],
        "notes": "Read-only rendering; never fabricates missing structure"
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/core/flight_recorder_tests.rs"]
mod tests;
