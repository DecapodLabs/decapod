//! A single transactional, source-pinned physical compaction inside migration 1100.
//! Never rename a customer table without verifying shape, keys, indexes,
//! exact row content, and foreign-key integrity. Canonical event TEXT is untouched.
use crate::core::db::{Connection, params};
use crate::core::error::DecapodError;
use crate::core::storage_codec::{self, StoredText};
use dactyl_db::Parameter;

fn unsafe_schema(why: &str) -> DecapodError {
    DecapodError::ValidationError(format!("RELATIONAL_COMPACTION_UNSAFE: {why}"))
}

pub fn apply(conn: &Connection) -> Result<(), DecapodError> {
    compact_pair(conn, "meta")?;
    compact_pair(conn, "task_tags")?;
    compact_summaries(conn)?;
    let violations: i64 =
        conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if violations > 0 {
        return Err(unsafe_schema("foreign-key integrity changed"));
    }
    Ok(())
}

fn compact_pair(conn: &Connection, table: &str) -> Result<(), DecapodError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(());
    }
    let (shape, ddl, insert, compare, allowed_index, secondary) = match table {
        "meta" => (
            "namespace:TEXT:1:1|key:TEXT:1:2|value:TEXT:1:0",
            "CREATE TABLE __decapod_compact_meta (namespace TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY(namespace,key)) WITHOUT ROWID",
            "INSERT INTO __decapod_compact_meta SELECT namespace,key,value FROM meta",
            "SELECT (SELECT count(*) FROM meta)=(SELECT count(*) FROM __decapod_compact_meta) AND NOT EXISTS(SELECT namespace,key,value FROM meta EXCEPT SELECT namespace,key,value FROM __decapod_compact_meta)",
            "",
            "",
        ),
        "task_tags" => (
            "task_id:TEXT:1:1|tag:TEXT:1:2",
            "CREATE TABLE __decapod_compact_task_tags (task_id TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY(task_id,tag), FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE) WITHOUT ROWID",
            "INSERT INTO __decapod_compact_task_tags SELECT task_id,tag FROM task_tags",
            "SELECT (SELECT count(*) FROM task_tags)=(SELECT count(*) FROM __decapod_compact_task_tags) AND NOT EXISTS(SELECT task_id,tag FROM task_tags EXCEPT SELECT task_id,tag FROM __decapod_compact_task_tags)",
            "idx_task_tags_tag",
            "CREATE INDEX idx_task_tags_tag ON task_tags(tag)",
        ),
        _ => return Err(unsafe_schema("unexpected table requested")),
    };
    let actual: String = conn.query_row(
        "SELECT group_concat(sig,'|') FROM (
           SELECT name||':'||upper(type)||':'||\"notnull\"||':'||pk AS sig
           FROM pragma_table_info(?1) ORDER BY cid)",
        [table],
        |r| r.get(0),
    )?;
    if actual != shape {
        return Err(unsafe_schema(&format!(
            "{table} has altered columns or primary key"
        )));
    }
    let without_rowid: i64 = conn.query_row(
        "SELECT wr FROM pragma_table_list WHERE \"schema\"='main' AND name=?1 AND type='table'",
        [table],
        |r| r.get(0),
    )?;
    if without_rowid == 1 {
        return Ok(());
    }
    let unsafe_index: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_index_list(?1) AS i
            WHERE i.name <> ('sqlite_autoindex_'||?1||'_1')
              AND i.name <> ?2)",
        params![table, allowed_index],
        |r| r.get(0),
    )?;
    let triggers: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger' AND tbl_name=?1)",
        [table],
        |r| r.get(0),
    )?;
    if unsafe_index || triggers {
        return Err(unsafe_schema(&format!(
            "{table} has unreviewed indexes/triggers"
        )));
    }
    if table == "task_tags" {
        let valid_fk: bool = conn.query_row(
            "SELECT (SELECT count(*) FROM pragma_foreign_key_list('task_tags'))=1
             AND EXISTS(SELECT 1 FROM pragma_foreign_key_list('task_tags')
             WHERE \"table\"='tasks' AND \"from\"='task_id'
               AND \"to\"='id' AND on_delete='CASCADE')",
            [],
            |r| r.get(0),
        )?;
        if !valid_fk {
            return Err(unsafe_schema("task_tags foreign key is unexpected"));
        }
        let valid_tag_index: bool = conn.query_row(
            "SELECT NOT EXISTS(
                SELECT 1 FROM pragma_index_list('task_tags') AS i
                WHERE i.name='idx_task_tags_tag'
                  AND (i.\"unique\"<>0 OR i.partial<>0
                    OR (SELECT group_concat(name,',') FROM (
                        SELECT name FROM pragma_index_info(i.name)
                        ORDER BY seqno))<>'tag')
            )",
            [],
            |r| r.get(0),
        )?;
        if !valid_tag_index {
            return Err(unsafe_schema("tag reverse lookup index was redefined"));
        }
    }
    conn.execute_batch(ddl)?;
    conn.execute_batch(insert)?;
    let exact: bool = conn.query_row(compare, [], |r| r.get(0))?;
    if !exact {
        return Err(unsafe_schema(&format!(
            "{table} failed lossless copy comparison"
        )));
    }
    // This function is called only with the two constants above.
    conn.execute_batch(&format!("DROP TABLE \"{table}\""))?;
    conn.execute_batch(&format!(
        "ALTER TABLE \"__decapod_compact_{table}\" RENAME TO \"{table}\""
    ))?;
    if !secondary.is_empty() {
        conn.execute_batch(secondary)?;
    }
    Ok(())
}

/// Rewrite only cold, derived LCM summaries; do not change event history.
fn compact_summaries(conn: &Connection) -> Result<(), DecapodError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='summaries')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(());
    }
    let mut cursor = String::new();
    loop {
        let mut stmt = conn.prepare(
            "SELECT summary_hash,original_hashes,summary_text FROM summaries
             WHERE summary_hash>?1 ORDER BY summary_hash LIMIT 16",
        )?;
        let mut rows = stmt.query([cursor.as_str()])?;
        let mut batch = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let params = row.parameters();
            let hashes = storage_codec::decode(&params[1])?;
            let summary = storage_codec::decode(&params[2])?;
            batch.push((id, params[1].clone(), params[2].clone(), hashes, summary));
        }
        if batch.is_empty() {
            break;
        }
        cursor = batch.last().expect("nonempty summary batch").0.clone();
        for (id, old_hashes, old_summary, hashes, summary) in batch {
            let new_hashes = storage_codec::encode(&hashes)?;
            let new_summary = storage_codec::encode(&summary)?;
            if matches!(old_hashes, Parameter::Text(_))
                && matches!(&new_hashes, StoredText::Zlib(_))
            {
                conn.execute(
                    "UPDATE summaries SET original_hashes=?1 WHERE summary_hash=?2",
                    params![new_hashes, id],
                )?;
            }
            if matches!(old_summary, Parameter::Text(_))
                && matches!(&new_summary, StoredText::Zlib(_))
            {
                conn.execute(
                    "UPDATE summaries SET summary_text=?1 WHERE summary_hash=?2",
                    params![new_summary, id],
                )?;
            }
            let physical: Vec<Parameter> = conn.query_row(
                "SELECT original_hashes,summary_text FROM summaries WHERE summary_hash=?1",
                [&id],
                |r| Ok(r.parameters()),
            )?;
            if storage_codec::decode(&physical[0])? != hashes
                || storage_codec::decode(&physical[1])? != summary
            {
                return Err(unsafe_schema("LCM text changed after compression"));
            }
        }
    }
    Ok(())
}
