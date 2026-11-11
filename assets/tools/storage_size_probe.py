#!/usr/bin/env python3
"""Deterministic, disposable SQLite physical-layout probe.

This is a sizing instrument, not a Decapod runtime codec or a production
migration. Only synthetic evidence is used. The process never opens a project
database, and the fixture directory is deleted before exit.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import statistics
import tempfile
import time
import zlib

LAYOUTS = ("rowid", "stream_pk", "dictionary", "compressed", "compressed_dictionary")
STREAMS = ("todo", "broker", "traces", "workunit")
COLUMNS = ("event_id", "ts", "seq", "stream", "subject_kind",
           "subject_id", "event_type", "payload", "actor")


def synthetic_rows(count: int):
    for i in range(count):
        payload = json.dumps({
            "id": f"feat_{i % 173:016x}",
            "revision": i % 19,
            "events": [{
                "rule": "govern.validate", "passed": True,
                "proof": "sha256:" + f"{i % 997:064x}",
            }],
            "context": {
                "repo": "DecapodLabs/decapod",
                "scope": "src/decapod",
                "owner": "agent-default",
            },
            "notes": "falsifiable validation and policy checkpoint",
        }, sort_keys=True, separators=(",", ":"))
        yield (f"{i:026x}",
               f"2026-11-10T{(i // 3600) % 24:02d}:{(i // 60) % 60:02d}:{i % 60:02d}Z",
               i // 4 + 1, STREAMS[i % len(STREAMS)], "task",
               f"feat_{i % 173:016x}", "task.claim", payload, "decapod")


def initialize(conn: sqlite3.Connection, mode: str):
    if mode == "stream_pk":
        conn.executescript("""
            CREATE TABLE events(
                event_id TEXT NOT NULL UNIQUE, ts TEXT NOT NULL,
                seq INTEGER NOT NULL, stream TEXT NOT NULL,
                subject_kind TEXT, subject_id TEXT,
                event_type TEXT NOT NULL, payload TEXT NOT NULL,
                actor TEXT NOT NULL, PRIMARY KEY(stream, seq)
            ) WITHOUT ROWID;
            CREATE INDEX idx_subject ON events(subject_kind, subject_id);
        """)
    elif mode in ("dictionary", "compressed_dictionary"):
        conn.executescript("""
            CREATE TABLE atoms(
                id INTEGER PRIMARY KEY, kind INTEGER NOT NULL,
                value TEXT NOT NULL, UNIQUE(kind, value)
            );
            CREATE TABLE events(
                event_id TEXT PRIMARY KEY, ts TEXT NOT NULL,
                seq INTEGER NOT NULL, stream_id INTEGER NOT NULL,
                subject_kind INTEGER, subject_id TEXT,
                event_type_id INTEGER NOT NULL, payload BLOB NOT NULL,
                actor_id INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX idx_stream_seq ON events(stream_id, seq);
            CREATE INDEX idx_subject ON events(subject_kind, subject_id);
        """)
    else:
        payload_type = "BLOB" if mode == "compressed" else "TEXT"
        conn.executescript(f"""
            CREATE TABLE events(
                event_id TEXT PRIMARY KEY, ts TEXT NOT NULL,
                seq INTEGER NOT NULL, stream TEXT NOT NULL,
                subject_kind TEXT, subject_id TEXT,
                event_type TEXT NOT NULL, payload {payload_type} NOT NULL,
                actor TEXT NOT NULL
            );
            CREATE UNIQUE INDEX idx_stream_seq ON events(stream, seq);
            CREATE INDEX idx_subject ON events(subject_kind, subject_id);
        """)


def encoded_rows(conn: sqlite3.Connection, mode: str, rows: list[tuple]):
    dictionary = mode in ("dictionary", "compressed_dictionary")
    compressed = mode in ("compressed", "compressed_dictionary")
    if dictionary:
        atoms = [(0, name) for name in STREAMS] + [
            (1, "task"), (2, "task.claim"), (3, "decapod"),
        ]
        conn.executemany("INSERT INTO atoms(kind, value) VALUES (?, ?)", atoms)
        ids = {pair: i + 1 for i, pair in enumerate(atoms)}
    for row in rows:
        event_id, ts, seq, stream, kind, subject, event_type, payload, actor = row
        if dictionary:
            yield (event_id, ts, seq, ids[0, stream], ids[1, kind],
                   subject, ids[2, event_type],
                   zlib.compress(payload.encode(), 6) if compressed else payload.encode(),
                   ids[3, actor])
        else:
            yield (event_id, ts, seq, stream, kind, subject, event_type,
                   zlib.compress(payload.encode(), 6) if compressed else payload, actor)


def decode_payload(value: bytes | str, compressed: bool) -> str:
    if compressed:
        return zlib.decompress(value).decode()
    return value.decode() if isinstance(value, bytes) else value


def run_variant(mode: str, rows: list[tuple], reads: int):
    with tempfile.TemporaryDirectory(prefix="decapod-size-probe-") as temp:
        path = Path(temp) / "fixture.db"
        conn = sqlite3.connect(path)
        conn.execute("PRAGMA page_size=4096")
        conn.execute("PRAGMA journal_mode=DELETE")
        conn.execute("PRAGMA synchronous=OFF")  # fixture construction only
        initialize(conn, mode)
        conn.executemany("INSERT INTO events VALUES (?,?,?,?,?,?,?,?,?)",
                         encoded_rows(conn, mode, rows))
        conn.commit()
        conn.execute("VACUUM")  # disposable fixture only; not a runtime setting
        assert conn.execute("PRAGMA integrity_check").fetchone()[0] == "ok"

        dictionary = mode in ("dictionary", "compressed_dictionary")
        compressed = mode in ("compressed", "compressed_dictionary")
        stream_col = "stream_id" if dictionary else "stream"
        stream_key = 1 if dictionary else "todo"
        query = (f"SELECT event_id, payload FROM events WHERE {stream_col}=? "
                 "ORDER BY seq DESC LIMIT 64")
        ids = {}
        raw_rows = conn.execute("SELECT event_id, payload FROM events").fetchall()
        ids.update((name, decode_payload(value, compressed)) for name, value in raw_rows)
        assert len(ids) == len(rows)
        for expected in rows:
            assert ids[expected[0]] == expected[7], "evidence bytes changed"

        latencies = []
        for _ in range(reads):
            start = time.perf_counter_ns()
            selected = conn.execute(query, (stream_key,)).fetchall()
            decoded = [decode_payload(value, compressed) for _, value in selected]
            assert len(decoded) == min(64, len(rows) // len(STREAMS))
            latencies.append((time.perf_counter_ns() - start) / 1_000_000)

        btrees = dict(conn.execute(
            "SELECT name, SUM(pgsize) FROM dbstat GROUP BY name ORDER BY name"))
        total = path.stat().st_size
        conn.close()
        ordered = sorted(latencies)
        p95 = ordered[(len(ordered) * 95 + 99) // 100 - 1]
        return {
            "layout": mode, "rows": len(rows),
            "file_bytes": total,
            "bytes_per_event": round(total / len(rows), 2),
            "btree_bytes": btrees,
            "read_64_ms_p50": round(statistics.median(latencies), 4),
            "read_64_ms_p95": round(p95, 4),
            "payload_roundtrip": "byte-identical",
        }


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--rows", type=int, default=24_000)
    cli.add_argument("--reads", type=int, default=500)
    args = cli.parse_args()
    if not 64 <= args.rows <= 100_000 or args.rows % len(STREAMS):
        cli.error("--rows must be a multiple of four between 64 and 100000")
    if not 1 <= args.reads <= 10_000:
        cli.error("--reads must be between 1 and 10000")
    rows = list(synthetic_rows(args.rows))
    digest = hashlib.sha256()
    for row in rows:
        digest.update(row[0].encode())
        digest.update(b"\0")
        digest.update(row[7].encode())
        digest.update(b"\n")
    result = [run_variant(mode, rows, args.reads) for mode in LAYOUTS]
    baseline = result[0]["file_bytes"]
    for row in result:
        row["reduction_percent"] = round(
            100 * (baseline - row["file_bytes"]) / baseline, 2)
    print(json.dumps({
        "schema": "decapod.storage-size-probe.v1",
        "classification": "synthetic_only_not_production_evidence",
        "sqlite_version": sqlite3.sqlite_version,
        "source_payload_sha256": digest.hexdigest(),
        "results": result,
    }, sort_keys=True, indent=2))


if __name__ == "__main__":
    main()
