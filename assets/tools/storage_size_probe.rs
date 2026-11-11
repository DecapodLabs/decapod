//! Reproducible, disposable SQLite storage frontier probe.
//!
//! Synthetic fixtures only. Never connects to a Decapod project or Supabase.
//! The runner measures allocated database bytes and exact logical round trips.
//! It does not authorize a production codec or a physical schema migration.
use decapod::core::db::{Connection, params};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, error::Error, fs, io::{Read, Write}, time::Instant};

const STREAMS: [&str; 4] = ["todo", "broker", "traces", "workunit"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Event {
    id: String,
    timestamp: String,
    sequence: i64,
    stream: String,
    subject_kind: Option<String>,
    subject_id: Option<String>,
    event_type: String,
    payload: String,
    actor: String,
}

#[derive(Clone, Copy)]
enum Layout { Rowid, StreamPrimaryKey, Dictionary, Compressed, CompressedDictionary }
impl Layout {
    const ALL: [Self; 5] = [
        Self::Rowid, Self::StreamPrimaryKey, Self::Dictionary,
        Self::Compressed, Self::CompressedDictionary,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Rowid => "rowid",
            Self::StreamPrimaryKey => "stream_pk",
            Self::Dictionary => "dictionary",
            Self::Compressed => "compressed",
            Self::CompressedDictionary => "compressed_dictionary",
        }
    }
    fn dictionary(self) -> bool {
        matches!(self, Self::Dictionary | Self::CompressedDictionary)
    }
    fn compressed(self) -> bool {
        matches!(self, Self::Compressed | Self::CompressedDictionary)
    }
}

#[derive(Serialize)]
struct ResultRow {
    layout: &'static str,
    rows: usize,
    file_bytes: u64,
    bytes_per_event: f64,
    reduction_percent: f64,
    free_pages: i64,
    btree_bytes: BTreeMap<String, i64>,
    dbstat_available: bool,
    read_64_ms_p50: f64,
    read_64_ms_p95: f64,
    read_64_ms_p99: f64,
    logical_roundtrip: &'static str,
}

fn fixture(i: usize) -> Event {
    let stream = STREAMS[i % STREAMS.len()];
    Event {
        id: format!("{i:026x}"),
        timestamp: format!("2026-11-10T{:02}:{:02}:{:02}Z", (i / 3600) % 24, (i / 60) % 60, i % 60),
        sequence: (i / STREAMS.len() + 1) as i64,
        stream: stream.into(),
        subject_kind: Some("task".into()),
        subject_id: Some(format!("feat_{:016x}", i % 173)),
        event_type: "task.claim".into(),
        payload: serde_json::json!({
            "id":format!("feat_{:016x}", i % 173),
            "revision":i % 19,
            "events":[{"rule":"govern.validate","passed":true,"proof":format!("sha256:{:064x}",i % 997)}],
            "context":{"repo":"DecapodLabs/decapod","scope":"src/decapod","owner":"agent-default"},
            "notes":"falsifiable validation and policy checkpoint"
        }).to_string(),
        actor: "decapod".into(),
    }
}

fn create(conn: &Connection, layout: Layout) -> Result<(), Box<dyn Error>> {
    let ddl = match layout {
        Layout::Rowid | Layout::Compressed => "
            CREATE TABLE events(id TEXT PRIMARY KEY, ts TEXT NOT NULL, seq INTEGER NOT NULL,
                stream TEXT NOT NULL, subject_kind TEXT, subject_id TEXT,
                event_type TEXT NOT NULL, payload BLOB NOT NULL, actor TEXT NOT NULL);
            CREATE UNIQUE INDEX events_sequence ON events(stream,seq);
            CREATE INDEX events_subject ON events(subject_kind,subject_id);",
        Layout::StreamPrimaryKey => "
            CREATE TABLE events(id TEXT NOT NULL UNIQUE, ts TEXT NOT NULL,
                seq INTEGER NOT NULL, stream TEXT NOT NULL, subject_kind TEXT,
                subject_id TEXT, event_type TEXT NOT NULL, payload BLOB NOT NULL,
                actor TEXT NOT NULL, PRIMARY KEY(stream,seq)) WITHOUT ROWID;
            CREATE INDEX events_subject ON events(subject_kind,subject_id);",
        Layout::Dictionary | Layout::CompressedDictionary => "
            CREATE TABLE atoms(id INTEGER PRIMARY KEY,kind INTEGER NOT NULL,
                value TEXT NOT NULL, UNIQUE(kind,value));
            CREATE TABLE events(id TEXT PRIMARY KEY, ts TEXT NOT NULL, seq INTEGER NOT NULL,
                stream_id INTEGER NOT NULL, subject_kind_id INTEGER, subject_id TEXT,
                event_type_id INTEGER NOT NULL, payload BLOB NOT NULL,
                actor_id INTEGER NOT NULL);
            CREATE UNIQUE INDEX events_sequence ON events(stream_id,seq);
            CREATE INDEX events_subject ON events(subject_kind_id,subject_id);",
    };
    conn.execute_batch(ddl)?;
    if layout.dictionary() {
        for (index, stream) in STREAMS.iter().enumerate() {
            conn.execute("INSERT INTO atoms(id,kind,value) VALUES(?1,0,?2)",
                params![(index + 1) as i64, stream])?;
        }
        for (id, kind, value) in [
            (5_i64, 1_i64, "task"), (6, 2, "task.claim"), (7, 3, "decapod"),
        ] {
            conn.execute("INSERT INTO atoms(id,kind,value) VALUES(?1,?2,?3)",
                params![id,kind,value])?;
        }
    }
    Ok(())
}

fn compress(text: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
    encoder.write_all(text.as_bytes())?;
    Ok(encoder.finish()?)
}
fn decode(data: &[u8], compressed: bool) -> Result<String, Box<dyn Error>> {
    if !compressed { return Ok(String::from_utf8(data.to_vec())?); }
    let mut decoder = ZlibDecoder::new(data);
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded)?;
    Ok(String::from_utf8(decoded)?)
}

fn insert(conn: &Connection, layout: Layout, event: &Event) -> Result<(), Box<dyn Error>> {
    let payload = if layout.compressed() {
        compress(&event.payload)?
    } else { event.payload.as_bytes().to_vec() };
    if layout.dictionary() {
        let stream_id = (STREAMS.iter().position(|item| *item == event.stream)
            .ok_or("unknown fixture stream")? + 1) as i64;
        conn.execute(
            "INSERT INTO events(id,ts,seq,stream_id,subject_kind_id,subject_id,event_type_id,payload,actor_id)
             VALUES (?1,?2,?3,?4,5,?5,6,?6,7)",
            params![&event.id,&event.timestamp,event.sequence,stream_id,&event.subject_id,payload],
        )?;
    } else {
        conn.execute(
            "INSERT INTO events(id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![&event.id,&event.timestamp,event.sequence,&event.stream,
                &event.subject_kind,&event.subject_id,&event.event_type,payload,&event.actor],
        )?;
    }
    Ok(())
}

fn select_sql(layout: Layout, suffix: &str) -> String {
    let prefix = if layout.dictionary() {
        "SELECT e.id,e.ts,e.seq,s.value,k.value,e.subject_id,t.value,e.payload,a.value
         FROM events e
         JOIN atoms s ON s.id=e.stream_id AND s.kind=0
         JOIN atoms k ON k.id=e.subject_kind_id AND k.kind=1
         JOIN atoms t ON t.id=e.event_type_id AND t.kind=2
         JOIN atoms a ON a.id=e.actor_id AND a.kind=3"
    } else {
        "SELECT id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor FROM events"
    };
    format!("{prefix} {suffix}")
}

fn all_events(conn: &Connection, layout: Layout) -> Result<Vec<Event>, Box<dyn Error>> {
    let query = select_sql(layout, "ORDER BY 1");
    let mut statement = conn.prepare(&query)?;
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?, r.get::<_, String>(6)?,
            r.get::<_, Vec<u8>>(7)?, r.get::<_, String>(8)?,
        ))
    })?;
    let mut all = Vec::new();
    for item in rows {
        let (id,timestamp,sequence,stream,subject_kind,subject_id,event_type,raw,actor) = item?;
        all.push(Event {
            id,timestamp,sequence,stream,subject_kind,subject_id,event_type,
            payload:decode(&raw,layout.compressed())?,actor,
        });
    }
    Ok(all)
}

fn event_digest(events: &[Event]) -> String {
    let mut digest = Sha256::new();
    for event in events {
        for field in [
            Some(event.id.as_str()), Some(event.timestamp.as_str()),
            Some(event.stream.as_str()), event.subject_kind.as_deref(),
            event.subject_id.as_deref(), Some(event.event_type.as_str()),
            Some(event.payload.as_str()), Some(event.actor.as_str()),
        ] {
            match field {
                None => digest.update([0]),
                Some(value) => {
                    digest.update([1]);
                    digest.update((value.len() as u64).to_le_bytes());
                    digest.update(value.as_bytes());
                }
            }
        }
        digest.update(event.sequence.to_le_bytes());
    }
    format!("{:x}",digest.finalize())
}

fn percentile(values: &[f64], n: usize) -> f64 {
    let index = (values.len() * n).div_ceil(100).saturating_sub(1);
    (values[index] * 1_000.0).round() / 1_000.0
}

fn benchmark(layout: Layout, expected: &[Event], reads: usize) -> Result<ResultRow, Box<dyn Error>> {
    let temp = tempfile::Builder::new().prefix("decapod-storage-probe-").tempdir()?;
    let path = temp.path().join("fixture.db");
    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA page_size=4096; PRAGMA journal_mode=DELETE; PRAGMA synchronous=OFF;")?;
    create(&conn,layout)?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    for event in expected { insert(&conn,layout,event)?; }
    conn.execute_batch("COMMIT; VACUUM;")?;
    let integrity:String = conn.query_row("PRAGMA integrity_check",[],|row|row.get(0))?;
    if integrity != "ok" {return Err("SQLite integrity_check failed".into());}
    let replayed = all_events(&conn,layout)?;
    if replayed != expected || event_digest(&replayed) != event_digest(expected) {
        return Err(format!("lossless event reconstruction failed for {}",layout.name()).into());
    }
    let free_pages:i64 = conn.query_row("PRAGMA freelist_count",[],|row|row.get(0))?;
    let (btree_bytes,dbstat_available) = match conn.prepare("SELECT name,SUM(pgsize) FROM dbstat GROUP BY name") {
        Ok(mut stmt) => {
            let pairs = stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?
                .collect::<Result<BTreeMap<_,_>,_>>()?;
            (pairs,true)
        },
        Err(_) => (BTreeMap::new(),false),
    };
    let query = if layout.dictionary() {
        select_sql(layout,"WHERE e.stream_id=?1 ORDER BY e.seq DESC LIMIT 64")
    } else {
        select_sql(layout,"WHERE stream=?1 ORDER BY seq DESC LIMIT 64")
    };
    let mut samples = Vec::with_capacity(reads);
    for _ in 0..reads {
        let started = Instant::now();
        let mut statement=conn.prepare(&query)?;
        let mut rows=if layout.dictionary(){statement.query([1_i64])?}else{statement.query(["todo"])?};
        let mut count=0;
        while let Some(row)=rows.next()?{
            let bytes:Vec<u8>=row.get(7)?;
            if decode(&bytes,layout.compressed())?.is_empty(){
                return Err("decoded evidence payload is empty".into());
            }
            count+=1;
        }
        if count != 64{return Err("bounded event read returned wrong count".into());}
        samples.push(started.elapsed().as_secs_f64()*1000.0);
    }
    samples.sort_by(f64::total_cmp);
    let file_bytes=fs::metadata(&path)?.len();
    Ok(ResultRow {
        layout:layout.name(),rows:expected.len(),file_bytes,
        bytes_per_event:(file_bytes as f64 / expected.len() as f64 *100.0).round()/100.0,
        reduction_percent:0.0,free_pages,btree_bytes,dbstat_available,
        read_64_ms_p50:percentile(&samples,50),
        read_64_ms_p95:percentile(&samples,95),
        read_64_ms_p99:percentile(&samples,99),
        logical_roundtrip:"all_columns_byte_identical",
    })
}

fn arguments() -> Result<(usize,usize),Box<dyn Error>> {
    let mut rows=24_000_usize; let mut reads=500_usize;
    let mut args=std::env::args().skip(1);
    while let Some(arg)=args.next(){
        let value=args.next().ok_or(format!("missing value for {arg}"))?;
        match arg.as_str() {
            "--rows"=>rows=value.parse()?,
            "--reads"=>reads=value.parse()?,
            _=>return Err(format!("unknown option {arg}").into()),
        }
    }
    if !(64..=100_000).contains(&rows)||rows%4!=0 {
        return Err("rows must be divisible by 4 in the interval 64..100000".into());
    }
    if !(1..=10_000).contains(&reads) {
        return Err("reads must be within 1..10000".into());
    }
    Ok((rows,reads))
}

fn main() -> Result<(),Box<dyn Error>> {
    let (count,reads)=arguments()?;
    let events:Vec<Event>=(0..count).map(fixture).collect();
    let mut results=Vec::new();
    for layout in Layout::ALL {
        results.push(benchmark(layout,&events,reads)?);
    }
    let baseline=results[0].file_bytes as f64;
    for row in &mut results {
        row.reduction_percent=((baseline-row.file_bytes as f64)*10_000.0/baseline).round()/100.0;
    }
    let report=serde_json::json!({
        "schema":"decapod.storage-size-probe.v2",
        "classification":"synthetic_only_not_production_evidence",
        "durability":"fixture_synchronous_off_not_a_durability_benchmark",
        "sqlite_version":"from_linked_runtime",
        "source_event_sha256":event_digest(&events),
        "results":results,
    });
    println!("{}",serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lossy_compression_is_not_accepted() {
        let text="proof: sha256:".to_string()+"abcdef";
        assert_eq!(decode(&compress(&text).unwrap(),true).unwrap(),text);
    }
    #[test]
    fn four_stream_sequences_are_unique() {
        let inputs:Vec<_>=(0..96).map(fixture).collect();
        assert_eq!(inputs[0].sequence,1);
        assert_eq!(inputs[4].sequence,2);
        assert_eq!(inputs[3].stream,"workunit");
    }
}
