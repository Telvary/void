//! Read-path smoke tests against a real, seeded on-disk store.
//!
//! We seed `<store>/void.db` using void-core's public `Database::open` +
//! `upsert_conversation` / `upsert_message`, then run the read commands and
//! assert exit 0 and that seeded content appears in stdout (JSON output).

use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use void_core::db::Database;
use void_core::models::{Conversation, ConversationKind, Message};

/// An isolated store with a config file whose `store.path` points at the store
/// dir, plus a seeded `void.db`.
struct SeededStore {
    _dir: TempDir,
    store: String,
    config: String,
}

fn make_conversation(id: &str, ext_id: &str, name: &str, kind: ConversationKind) -> Conversation {
    Conversation {
        id: id.into(),
        connection_id: "test-slack".into(),
        connector: "slack".into(),
        external_id: ext_id.into(),
        name: Some(name.into()),
        kind,
        last_message_at: Some(1_700_000_000),
        unread_count: 0,
        is_muted: false,
        metadata: None,
    }
}

fn make_message(id: &str, conv_id: &str, sender: &str, body: &str, ts: i64) -> Message {
    Message {
        id: id.into(),
        conversation_id: conv_id.into(),
        connection_id: "test-slack".into(),
        connector: "slack".into(),
        external_id: format!("ext-{id}"),
        sender: sender.into(),
        sender_name: Some("Alice Example".into()),
        sender_avatar_url: None,
        body: Some(body.into()),
        timestamp: ts,
        synced_at: None,
        is_archived: false,
        reply_to_id: None,
        media_type: None,
        metadata: None,
        context_id: None,
        context: None,
    }
}

fn seed_db(db_path: &Path) {
    let db = Database::open(db_path).expect("open db for seeding");

    // A DM conversation (excluded from `channels`) and a channel conversation.
    let dm = make_conversation(
        "c-dm",
        "C-DM-EXT",
        "Direct With Alice",
        ConversationKind::Dm,
    );
    let channel = make_conversation(
        "c-chan",
        "C-CHAN-EXT",
        "general-announcements",
        ConversationKind::Channel,
    );
    db.upsert_conversation(&dm).expect("upsert dm");
    db.upsert_conversation(&channel).expect("upsert channel");

    // Messages. `sender != connection_id` so they surface as contacts too.
    db.upsert_message(&make_message(
        "m1",
        "c-dm",
        "alice@example.com",
        "ZEBRAFISH lunch plans",
        1_700_000_100,
    ))
    .expect("upsert m1");
    db.upsert_message(&make_message(
        "m2",
        "c-chan",
        "bob@example.com",
        "QUOKKA deploy is live",
        1_700_000_200,
    ))
    .expect("upsert m2");
}

impl SeededStore {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let store_dir = dir.path().join("store");
        std::fs::create_dir_all(&store_dir).expect("create store dir");
        let store = store_dir.to_string_lossy().into_owned();
        let config = dir
            .path()
            .join("config.toml")
            .to_string_lossy()
            .into_owned();

        // Config in local mode with store.path pinned to our tempdir so any
        // code path that reloads the config (e.g. doctor) stays isolated.
        // Escape backslashes so a Windows path is a valid TOML basic string
        // (the unescaped `store` is still what we pass to `--store`).
        let store_toml = store.replace('\\', "\\\\");
        let config_contents = format!("[store]\nmode = \"local\"\npath = \"{store_toml}\"\n");
        std::fs::write(&config, config_contents).expect("write config");

        seed_db(&store_dir.join("void.db"));

        Self {
            _dir: dir,
            store,
            config,
        }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("void").expect("void binary");
        c.arg("--store")
            .arg(&self.store)
            .arg("--config")
            .arg(&self.config);
        c
    }
}

#[test]
fn inbox_shows_seeded_messages() {
    let sb = SeededStore::new();
    sb.cmd()
        .arg("inbox")
        .assert()
        .success()
        .stdout(predicate::str::contains("ZEBRAFISH"))
        .stdout(predicate::str::contains("QUOKKA"));
}

#[test]
fn search_finds_seeded_message() {
    let sb = SeededStore::new();
    sb.cmd()
        .args(["search", "ZEBRAFISH"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ZEBRAFISH"))
        .stdout(predicate::str::contains("QUOKKA").not());
}

#[test]
fn conversations_lists_seeded_conversations() {
    let sb = SeededStore::new();
    sb.cmd()
        .arg("conversations")
        .assert()
        .success()
        .stdout(predicate::str::contains("Direct With Alice"))
        .stdout(predicate::str::contains("general-announcements"));
}

#[test]
fn messages_shows_messages_for_conversation() {
    let sb = SeededStore::new();
    sb.cmd()
        .args(["messages", "c-dm"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ZEBRAFISH"));
}

#[test]
fn contacts_lists_seeded_senders() {
    let sb = SeededStore::new();
    sb.cmd()
        .arg("contacts")
        .assert()
        .success()
        .stdout(predicate::str::contains("alice@example.com"));
}

#[test]
fn channels_lists_only_channel_conversations() {
    let sb = SeededStore::new();
    // `channels` excludes DMs (kind = dm), includes group/channel.
    sb.cmd()
        .arg("channels")
        .assert()
        .success()
        .stdout(predicate::str::contains("general-announcements"))
        .stdout(predicate::str::contains("Direct With Alice").not());
}
