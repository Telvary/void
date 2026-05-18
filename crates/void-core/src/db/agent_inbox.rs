//! Agent inbox persistence: submit, list, get, respond, archive.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::DbError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentInboxItem {
    pub id: i64,
    pub callback_id: String,
    pub item_type: String,
    pub source: String,
    pub title: String,
    pub body: String,
    pub priority: String,
    pub status: String,
    pub action_json: Option<String>,
    pub input_label: Option<String>,
    pub response_text: Option<String>,
    pub response_comment: Option<String>,
    pub created_at: String,
    pub responded_at: Option<String>,
}

#[derive(Debug)]
pub struct AgentInboxInsert<'a> {
    pub callback_id: &'a str,
    pub item_type: &'a str,
    pub source: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub priority: &'a str,
    pub action_json: Option<&'a str>,
    pub input_label: Option<&'a str>,
    pub created_at: &'a str,
}

pub(super) fn insert(
    conn: &Connection,
    item: &AgentInboxInsert<'_>,
) -> Result<AgentInboxItem, DbError> {
    conn.execute(
        "INSERT INTO agent_inbox (callback_id, item_type, source, title, body, priority, action_json, input_label, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            item.callback_id,
            item.item_type,
            item.source,
            item.title,
            item.body,
            item.priority,
            item.action_json,
            item.input_label,
            item.created_at,
        ],
    )?;
    get(conn, item.callback_id)?
        .ok_or_else(|| DbError::Other("failed to retrieve inserted agent inbox item".into()))
}

pub(super) fn list(
    conn: &Connection,
    status_filter: Option<&str>,
    type_filter: Option<&str>,
    limit: i64,
) -> Result<Vec<AgentInboxItem>, DbError> {
    let mut sql = String::from(
        "SELECT id, callback_id, item_type, source, title, body, priority, status,
                action_json, input_label, response_text, response_comment, created_at, responded_at
         FROM agent_inbox WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(status) = status_filter {
        sql.push_str(" AND status = ?");
        param_values.push(Box::new(status.to_string()));
    }
    if let Some(item_type) = type_filter {
        sql.push_str(" AND item_type = ?");
        param_values.push(Box::new(item_type.to_string()));
    }

    sql.push_str(" ORDER BY id DESC LIMIT ?");
    param_values.push(Box::new(limit));

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_ref.as_slice(), row_to_item)?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

pub(super) fn get(conn: &Connection, callback_id: &str) -> Result<Option<AgentInboxItem>, DbError> {
    conn.query_row(
        "SELECT id, callback_id, item_type, source, title, body, priority, status,
                action_json, input_label, response_text, response_comment, created_at, responded_at
         FROM agent_inbox WHERE callback_id = ?1",
        params![callback_id],
        row_to_item,
    )
    .optional()
    .map_err(DbError::from)
}

pub(super) fn respond(
    conn: &Connection,
    callback_id: &str,
    response: &str,
    comment: Option<&str>,
) -> Result<bool, DbError> {
    let now = chrono::Utc::now().to_rfc3339();
    let affected = conn.execute(
        "UPDATE agent_inbox SET response_text = ?1, response_comment = ?2, status = 'done', responded_at = ?3
         WHERE callback_id = ?4",
        params![response, comment, now, callback_id],
    )?;
    Ok(affected > 0)
}

pub(super) fn mark_read(conn: &Connection, callback_id: &str) -> Result<bool, DbError> {
    let affected = conn.execute(
        "UPDATE agent_inbox SET status = 'read' WHERE callback_id = ?1 AND status = 'unread'",
        params![callback_id],
    )?;
    Ok(affected > 0)
}

pub(super) fn archive(conn: &Connection, callback_ids: &[String]) -> Result<usize, DbError> {
    if callback_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = (1..=callback_ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "UPDATE agent_inbox SET status = 'done' WHERE callback_id IN ({}) AND status != 'done'",
        placeholders.join(", ")
    );
    let params: Vec<&dyn rusqlite::types::ToSql> = callback_ids
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();
    let affected = conn.execute(&sql, params.as_slice())?;
    Ok(affected)
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentInboxItem> {
    Ok(AgentInboxItem {
        id: row.get(0)?,
        callback_id: row.get(1)?,
        item_type: row.get(2)?,
        source: row.get(3)?,
        title: row.get(4)?,
        body: row.get(5)?,
        priority: row.get(6)?,
        status: row.get(7)?,
        action_json: row.get(8)?,
        input_label: row.get(9)?,
        response_text: row.get(10)?,
        response_comment: row.get(11)?,
        created_at: row.get(12)?,
        responded_at: row.get(13)?,
    })
}
