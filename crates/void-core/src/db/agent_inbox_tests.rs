use super::*;
use crate::db::agent_inbox::AgentInboxInsert;

fn test_db() -> Database {
    Database::open_in_memory().unwrap()
}

fn make_insert<'a>(
    callback_id: &'a str,
    item_type: &'a str,
    source: &'a str,
    title: &'a str,
) -> AgentInboxInsert<'a> {
    AgentInboxInsert {
        callback_id,
        item_type,
        source,
        title,
        body: "Test body content",
        priority: "normal",
        action_json: None,
        input_label: None,
        created_at: "2026-04-26T10:00:00Z",
    }
}

// ---- Migration ----

#[test]
fn migration_v12_creates_agent_inbox_table() {
    let db = test_db();
    let conn = db.conn().unwrap();
    let version: i32 = conn
        .query_row(
            "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    assert_eq!(version, 12);

    let count: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='agent_inbox'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

// ---- Insert ----

#[test]
fn insert_fyi_item() {
    let db = test_db();
    let insert = make_insert("cb-001", "fyi", "daily-routine", "FYI title");
    let item = db.agent_inbox_insert(&insert).unwrap();

    assert_eq!(item.callback_id, "cb-001");
    assert_eq!(item.item_type, "fyi");
    assert_eq!(item.source, "daily-routine");
    assert_eq!(item.title, "FYI title");
    assert_eq!(item.status, "unread");
    assert_eq!(item.priority, "normal");
    assert!(item.action_json.is_none());
    assert!(item.response_text.is_none());
}

#[test]
fn insert_action_item_with_action_json() {
    let db = test_db();
    let mut insert = make_insert("cb-002", "action", "triage-agent", "Reply needed");
    insert.action_json = Some(r#"{"command":"reply","void_message_id":"msg_123"}"#);
    let item = db.agent_inbox_insert(&insert).unwrap();

    assert_eq!(item.item_type, "action");
    assert!(item.action_json.is_some());
    assert!(item.action_json.unwrap().contains("reply"));
}

#[test]
fn insert_input_item_with_label() {
    let db = test_db();
    let mut insert = make_insert("cb-003", "input", "content-agent", "Need feedback");
    insert.input_label = Some("Your feedback");
    let item = db.agent_inbox_insert(&insert).unwrap();

    assert_eq!(item.item_type, "input");
    assert_eq!(item.input_label.as_deref(), Some("Your feedback"));
}

#[test]
fn insert_approval_item() {
    let db = test_db();
    let insert = make_insert("cb-004", "approval", "deploy-agent", "Approve deploy?");
    let item = db.agent_inbox_insert(&insert).unwrap();

    assert_eq!(item.item_type, "approval");
    assert_eq!(item.status, "unread");
}

#[test]
fn insert_high_priority() {
    let db = test_db();
    let mut insert = make_insert("cb-005", "fyi", "agent", "Urgent");
    insert.priority = "high";
    let item = db.agent_inbox_insert(&insert).unwrap();

    assert_eq!(item.priority, "high");
}

#[test]
fn insert_duplicate_callback_id_fails() {
    let db = test_db();
    let insert = make_insert("cb-dup", "fyi", "agent", "First");
    db.agent_inbox_insert(&insert).unwrap();

    let insert2 = make_insert("cb-dup", "fyi", "agent", "Second");
    let result = db.agent_inbox_insert(&insert2);
    assert!(result.is_err());
}

#[test]
fn insert_invalid_type_fails() {
    let db = test_db();
    let insert = make_insert("cb-bad-type", "invalid_type", "agent", "Bad");
    let result = db.agent_inbox_insert(&insert);
    assert!(result.is_err());
}

// ---- List ----

#[test]
fn list_returns_newest_first() {
    let db = test_db();
    let mut i1 = make_insert("cb-a", "fyi", "agent", "First");
    i1.created_at = "2026-04-26T09:00:00Z";
    db.agent_inbox_insert(&i1).unwrap();

    let mut i2 = make_insert("cb-b", "fyi", "agent", "Second");
    i2.created_at = "2026-04-26T10:00:00Z";
    db.agent_inbox_insert(&i2).unwrap();

    let items = db.agent_inbox_list(None, None, 10).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].callback_id, "cb-b");
    assert_eq!(items[1].callback_id, "cb-a");
}

#[test]
fn list_filter_by_status() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-1", "fyi", "a", "T1"))
        .unwrap();
    db.agent_inbox_insert(&make_insert("cb-2", "fyi", "a", "T2"))
        .unwrap();
    db.agent_inbox_archive(&["cb-1".to_string()]).unwrap();

    let unread = db.agent_inbox_list(Some("unread"), None, 10).unwrap();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].callback_id, "cb-2");

    let done = db.agent_inbox_list(Some("done"), None, 10).unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].callback_id, "cb-1");
}

#[test]
fn list_filter_by_type() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-f", "fyi", "a", "FYI"))
        .unwrap();
    db.agent_inbox_insert(&make_insert("cb-a", "approval", "a", "Approval"))
        .unwrap();
    db.agent_inbox_insert(&make_insert("cb-i", "input", "a", "Input"))
        .unwrap();

    let approvals = db.agent_inbox_list(None, Some("approval"), 10).unwrap();
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0].callback_id, "cb-a");

    let fyis = db.agent_inbox_list(None, Some("fyi"), 10).unwrap();
    assert_eq!(fyis.len(), 1);
}

#[test]
fn list_respects_limit() {
    let db = test_db();
    for i in 0..5 {
        let id = format!("cb-{i}");
        db.agent_inbox_insert(&make_insert(&id, "fyi", "a", "T"))
            .unwrap();
    }

    let items = db.agent_inbox_list(None, None, 3).unwrap();
    assert_eq!(items.len(), 3);
}

#[test]
fn list_combined_filters() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-1", "fyi", "a", "T"))
        .unwrap();
    db.agent_inbox_insert(&make_insert("cb-2", "approval", "a", "T"))
        .unwrap();
    db.agent_inbox_insert(&make_insert("cb-3", "fyi", "a", "T"))
        .unwrap();
    db.agent_inbox_archive(&["cb-3".to_string()]).unwrap();

    let result = db
        .agent_inbox_list(Some("unread"), Some("fyi"), 10)
        .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].callback_id, "cb-1");
}

// ---- Get ----

#[test]
fn get_existing_item() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-get", "fyi", "a", "Title"))
        .unwrap();

    let item = db.agent_inbox_get("cb-get").unwrap();
    assert!(item.is_some());
    assert_eq!(item.unwrap().title, "Title");
}

#[test]
fn get_nonexistent_returns_none() {
    let db = test_db();
    let item = db.agent_inbox_get("nonexistent").unwrap();
    assert!(item.is_none());
}

#[test]
fn get_includes_response_after_respond() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-resp", "approval", "a", "Approve?"))
        .unwrap();
    db.agent_inbox_respond("cb-resp", "approved", Some("LGTM"))
        .unwrap();

    let item = db.agent_inbox_get("cb-resp").unwrap().unwrap();
    assert_eq!(item.response_text.as_deref(), Some("approved"));
    assert_eq!(item.response_comment.as_deref(), Some("LGTM"));
    assert_eq!(item.status, "done");
    assert!(item.responded_at.is_some());
}

// ---- Respond ----

#[test]
fn respond_sets_done_status() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-r1", "approval", "a", "T"))
        .unwrap();

    let updated = db.agent_inbox_respond("cb-r1", "rejected", None).unwrap();
    assert!(updated);

    let item = db.agent_inbox_get("cb-r1").unwrap().unwrap();
    assert_eq!(item.status, "done");
    assert_eq!(item.response_text.as_deref(), Some("rejected"));
    assert!(item.response_comment.is_none());
}

#[test]
fn respond_nonexistent_returns_false() {
    let db = test_db();
    let updated = db.agent_inbox_respond("no-such-id", "ok", None).unwrap();
    assert!(!updated);
}

#[test]
fn respond_with_comment() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-rc", "input", "a", "T"))
        .unwrap();
    db.agent_inbox_respond("cb-rc", "Here is my input", Some("Additional note"))
        .unwrap();

    let item = db.agent_inbox_get("cb-rc").unwrap().unwrap();
    assert_eq!(item.response_text.as_deref(), Some("Here is my input"));
    assert_eq!(item.response_comment.as_deref(), Some("Additional note"));
}

// ---- Mark read ----

#[test]
fn mark_read_transitions_from_unread() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-mr", "fyi", "a", "T"))
        .unwrap();

    let updated = db.agent_inbox_mark_read("cb-mr").unwrap();
    assert!(updated);

    let item = db.agent_inbox_get("cb-mr").unwrap().unwrap();
    assert_eq!(item.status, "read");
}

#[test]
fn mark_read_idempotent_on_already_read() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-mr2", "fyi", "a", "T"))
        .unwrap();
    db.agent_inbox_mark_read("cb-mr2").unwrap();

    let updated = db.agent_inbox_mark_read("cb-mr2").unwrap();
    assert!(!updated);
}

#[test]
fn mark_read_does_not_affect_done_items() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-mr3", "fyi", "a", "T"))
        .unwrap();
    db.agent_inbox_archive(&["cb-mr3".to_string()]).unwrap();

    let updated = db.agent_inbox_mark_read("cb-mr3").unwrap();
    assert!(!updated);

    let item = db.agent_inbox_get("cb-mr3").unwrap().unwrap();
    assert_eq!(item.status, "done");
}

// ---- Archive ----

#[test]
fn archive_single_item() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-ar1", "fyi", "a", "T"))
        .unwrap();

    let count = db.agent_inbox_archive(&["cb-ar1".to_string()]).unwrap();
    assert_eq!(count, 1);

    let item = db.agent_inbox_get("cb-ar1").unwrap().unwrap();
    assert_eq!(item.status, "done");
}

#[test]
fn archive_multiple_items() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-b1", "fyi", "a", "T"))
        .unwrap();
    db.agent_inbox_insert(&make_insert("cb-b2", "fyi", "a", "T"))
        .unwrap();
    db.agent_inbox_insert(&make_insert("cb-b3", "fyi", "a", "T"))
        .unwrap();

    let count = db
        .agent_inbox_archive(&[
            "cb-b1".to_string(),
            "cb-b2".to_string(),
            "cb-b3".to_string(),
        ])
        .unwrap();
    assert_eq!(count, 3);
}

#[test]
fn archive_empty_list_returns_zero() {
    let db = test_db();
    let count = db.agent_inbox_archive(&[]).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn archive_already_done_not_counted() {
    let db = test_db();
    db.agent_inbox_insert(&make_insert("cb-ad", "fyi", "a", "T"))
        .unwrap();
    db.agent_inbox_archive(&["cb-ad".to_string()]).unwrap();

    let count = db.agent_inbox_archive(&["cb-ad".to_string()]).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn archive_nonexistent_returns_zero() {
    let db = test_db();
    let count = db.agent_inbox_archive(&["no-such-id".to_string()]).unwrap();
    assert_eq!(count, 0);
}
