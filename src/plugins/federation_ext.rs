use crate::core::broker::DbBroker;
use crate::core::error;
use crate::plugins::federation::{FederationNode, federation_db_path};
use std::path::Path;

pub fn list_nodes(
    root: &Path,
    node_type: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    scope: Option<String>,
) -> Result<Vec<FederationNode>, error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = federation_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "federation.list", |conn| {
        let mut conditions = vec!["1=1".to_string()];
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];

        if let Some(ref nt) = node_type {
            let idx = param_values.len() + 1;
            conditions.push(format!("node_type = ?{}", idx));
            param_values.push(Box::new(nt.clone()));
        }
        if let Some(ref s) = status {
            let idx = param_values.len() + 1;
            conditions.push(format!("status = ?{}", idx));
            param_values.push(Box::new(s.clone()));
        }
        if let Some(ref p) = priority {
            let idx = param_values.len() + 1;
            conditions.push(format!("priority = ?{}", idx));
            param_values.push(Box::new(p.clone()));
        }
        if let Some(ref sc) = scope {
            let idx = param_values.len() + 1;
            conditions.push(format!("scope = ?{}", idx));
            param_values.push(Box::new(sc.clone()));
        }

        let sql = format!(
            "SELECT id, node_type, status, priority, confidence, title, body, scope, tags,
                        created_at, updated_at, effective_from, effective_to, actor
                 FROM nodes WHERE {} ORDER BY updated_at DESC",
            conditions.join(" AND ")
        );

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();

        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(FederationNode {
                id: row.get(0)?,
                node_type: row.get(1)?,
                status: row.get(2)?,
                priority: row.get(3)?,
                confidence: row.get(4)?,
                title: row.get(5)?,
                body: row.get(6)?,
                scope: row.get(7)?,
                tags: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                effective_from: row.get(11)?,
                effective_to: row.get(12)?,
                actor: row.get(13)?,
                sources: None,
                edges: None,
            })
        })?;

        let mut nodes = Vec::new();
        for r in rows {
            nodes.push(r?);
        }
        Ok(nodes)
    })
}
