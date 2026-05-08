use crate::hooks::execute::extract_error_from_stream;
use crate::hooks::hook_fs::{
    delete_hook, find_hook, load_hooks, save_hook, slugify, update_hook_enabled,
};
use crate::hooks::model::{ActiveWindow, Hook, PromptConfig, Trigger, Weekday};
use crate::hooks::placeholders::expand_placeholders;
use crate::models::Message;

#[test]
fn load_hooks_returns_empty_for_nonexistent_dir() {
    let dir = std::env::temp_dir().join(format!("void-hooks-nonexistent-{}", uuid::Uuid::new_v4()));
    assert!(!dir.exists(), "dir should not exist");
    let hooks = load_hooks(&dir);
    assert!(hooks.is_empty());
}

#[test]
fn slugify_basic() {
    assert_eq!(slugify("Gmail Auto-Archive"), "gmail-auto-archive");
    assert_eq!(slugify("  Daily  Digest  "), "daily-digest");
    assert_eq!(slugify("foo_bar__baz"), "foo-bar-baz");
}

#[test]
fn hook_roundtrip() {
    let hook = Hook {
        name: "Test Hook".into(),
        enabled: true,
        max_turns: 5,
        agent: "claude".into(),
        extra_args: Vec::new(),
        active_window: None,
        trigger: Trigger::NewMessage {
            connector: Some("gmail".into()),
        },
        prompt: PromptConfig {
            text: "Hello {message_id}".into(),
        },
    };
    let toml_str = toml::to_string_pretty(&hook).unwrap();
    let parsed: Hook = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.name, "Test Hook");
    assert_eq!(parsed.max_turns, 5);
    assert!(
        matches!(parsed.trigger, Trigger::NewMessage { connector: Some(ref c) } if c == "gmail")
    );
    assert!(parsed.extra_args.is_empty());
}

#[test]
fn schedule_hook_roundtrip() {
    let hook = Hook {
        name: "Daily Digest".into(),
        enabled: true,
        max_turns: 10,
        agent: "claude".into(),
        extra_args: Vec::new(),
        active_window: None,
        trigger: Trigger::Schedule {
            cron: "0 9 * * 1-5".into(),
        },
        prompt: PromptConfig {
            text: "Run digest for {today}".into(),
        },
    };
    let toml_str = toml::to_string_pretty(&hook).unwrap();
    let parsed: Hook = toml::from_str(&toml_str).unwrap();
    assert!(matches!(parsed.trigger, Trigger::Schedule { ref cron } if cron == "0 9 * * 1-5"));
}

#[test]
fn hook_extra_args_roundtrip() {
    let hook = Hook {
        name: "WithArgs".into(),
        enabled: true,
        max_turns: 3,
        agent: "claude".into(),
        extra_args: vec![
            "--model".into(),
            "sonnet".into(),
            "--dangerously-skip-permissions".into(),
        ],
        active_window: None,
        trigger: Trigger::NewMessage { connector: None },
        prompt: PromptConfig { text: "x".into() },
    };
    let toml_str = toml::to_string_pretty(&hook).unwrap();
    assert!(
        toml_str.contains("extra_args"),
        "extra_args must be present:\n{toml_str}"
    );
    let parsed: Hook = toml::from_str(&toml_str).unwrap();
    assert_eq!(
        parsed.extra_args,
        vec![
            "--model".to_string(),
            "sonnet".to_string(),
            "--dangerously-skip-permissions".to_string(),
        ]
    );
}

#[test]
fn extract_error_from_stream_rate_limit_result() {
    let stream = r#"{"type":"system","subtype":"init"}
{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour"}}
{"type":"result","subtype":"success","is_error":true,"api_error_status":429,"result":"You've hit your limit · resets 6:20pm","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour"}}
"#;
    let err = extract_error_from_stream(stream).expect("should extract error");
    assert!(err.contains("HTTP 429"), "missing status tag: {err}");
    assert!(
        err.contains("rate_limit=five_hour"),
        "missing rate_limit tag: {err}"
    );
    assert!(err.contains("resets 6:20pm"), "missing body: {err}");
}

#[test]
fn extract_error_from_stream_no_error() {
    let stream = r#"{"type":"system","subtype":"init"}
{"type":"result","subtype":"success","is_error":false,"result":"all good"}
"#;
    assert!(extract_error_from_stream(stream).is_none());
}

#[test]
fn extract_error_from_stream_rate_limit_event_fallback() {
    let stream = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour"}}
"#;
    let err = extract_error_from_stream(stream).expect("should extract fallback");
    assert!(err.contains("rate limited"), "missing prefix: {err}");
    assert!(err.contains("five_hour"), "missing type: {err}");
}

#[test]
fn extract_error_from_stream_empty() {
    assert!(extract_error_from_stream("").is_none());
    assert!(extract_error_from_stream("not json\n").is_none());
}

#[test]
fn hook_extra_args_omitted_when_empty() {
    let hook = Hook {
        name: "Default".into(),
        enabled: true,
        max_turns: 1,
        agent: "claude".into(),
        extra_args: Vec::new(),
        active_window: None,
        trigger: Trigger::NewMessage { connector: None },
        prompt: PromptConfig { text: "x".into() },
    };
    let toml_str = toml::to_string_pretty(&hook).unwrap();
    assert!(
        !toml_str.contains("extra_args"),
        "expected extra_args to be omitted when empty, got:\n{toml_str}"
    );
}

#[test]
fn expand_placeholders_no_message() {
    let result = expand_placeholders("Today is {today}, now is {now}", None);
    assert!(!result.contains("{today}"));
    assert!(!result.contains("{now}"));
}

#[test]
fn expand_placeholders_keeps_message_tokens_when_no_message() {
    let result = expand_placeholders(
        "before {message_id} after {connector} {connection_id}",
        None,
    );
    assert_eq!(
        result, "before {message_id} after {connector} {connection_id}",
        "message placeholders must remain literal when no Message is supplied"
    );
}

#[test]
fn expand_placeholders_with_message() {
    let msg = Message {
        id: "msg-123".into(),
        conversation_id: "c1".into(),
        connection_id: "acc1".into(),
        connector: "gmail".into(),
        external_id: "ext1".into(),
        sender: "alice@example.com".into(),
        sender_name: None,
        sender_avatar_url: None,
        body: Some("Hello".into()),
        timestamp: 1_700_000_000,
        synced_at: None,
        is_archived: false,
        reply_to_id: None,
        media_type: None,
        metadata: None,
        context_id: None,
        context: None,
    };
    let result = expand_placeholders("ID={message_id} CONN={connector}", Some(&msg));
    assert_eq!(result, "ID=msg-123 CONN=gmail");
}

#[test]
fn save_and_load_hook() {
    let dir = std::env::temp_dir().join(format!("void-hooks-test-{}", uuid::Uuid::new_v4()));
    let hook = Hook {
        name: "My Test Hook".into(),
        enabled: true,
        max_turns: 3,
        agent: "claude".into(),
        extra_args: Vec::new(),
        active_window: None,
        trigger: Trigger::NewMessage { connector: None },
        prompt: PromptConfig {
            text: "test".into(),
        },
    };
    save_hook(&dir, &hook).unwrap();
    let loaded = load_hooks(&dir);
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "My Test Hook");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn delete_hook_works() {
    let dir = std::env::temp_dir().join(format!("void-hooks-test-{}", uuid::Uuid::new_v4()));
    let hook = Hook {
        name: "To Delete".into(),
        enabled: true,
        max_turns: 3,
        agent: "claude".into(),
        extra_args: Vec::new(),
        active_window: None,
        trigger: Trigger::NewMessage { connector: None },
        prompt: PromptConfig {
            text: "test".into(),
        },
    };
    save_hook(&dir, &hook).unwrap();
    assert!(delete_hook(&dir, "To Delete").unwrap());
    assert!(!delete_hook(&dir, "To Delete").unwrap());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn find_hook_works() {
    let dir = std::env::temp_dir().join(format!("void-hooks-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hook = Hook {
        name: "Find Me".into(),
        enabled: true,
        max_turns: 2,
        agent: "claude".into(),
        extra_args: Vec::new(),
        active_window: None,
        trigger: Trigger::NewMessage { connector: None },
        prompt: PromptConfig {
            text: "prompt".into(),
        },
    };
    save_hook(&dir, &hook).unwrap();
    let found = find_hook(&dir, "Find Me").expect("hook should exist");
    assert_eq!(found.name, "Find Me");
    assert_eq!(found.max_turns, 2);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn active_window_roundtrip() {
    let hook = Hook {
        name: "Windowed Hook".into(),
        enabled: true,
        max_turns: 3,
        agent: "claude".into(),
        extra_args: Vec::new(),
        active_window: Some(ActiveWindow {
            days: vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ],
            start: "08:00".into(),
            end: "21:00".into(),
            utc_offset_hours: Some(2),
        }),
        trigger: Trigger::NewMessage { connector: None },
        prompt: PromptConfig { text: "x".into() },
    };
    let toml_str = toml::to_string_pretty(&hook).unwrap();
    assert!(
        toml_str.contains("[active_window]"),
        "expected active_window section:\n{toml_str}"
    );
    assert!(
        toml_str.contains("08:00"),
        "expected start time:\n{toml_str}"
    );
    let parsed: Hook = toml::from_str(&toml_str).unwrap();
    let window = parsed
        .active_window
        .expect("active_window should be present");
    assert_eq!(window.days.len(), 5);
    assert_eq!(window.start, "08:00");
    assert_eq!(window.end, "21:00");
    assert_eq!(window.utc_offset_hours, Some(2));
}

#[test]
fn active_window_omitted_when_none() {
    let hook = Hook {
        name: "NoWindow".into(),
        enabled: true,
        max_turns: 1,
        agent: "claude".into(),
        extra_args: Vec::new(),
        active_window: None,
        trigger: Trigger::NewMessage { connector: None },
        prompt: PromptConfig { text: "x".into() },
    };
    let toml_str = toml::to_string_pretty(&hook).unwrap();
    assert!(
        !toml_str.contains("active_window"),
        "expected active_window to be omitted:\n{toml_str}"
    );
}

#[test]
fn active_window_is_active_checks_time_range() {
    let window = ActiveWindow {
        days: vec![
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ],
        start: "00:00".into(),
        end: "23:59".into(),
        utc_offset_hours: None,
    };
    assert!(window.is_active_now(), "all-day window should be active");

    let window_never = ActiveWindow {
        days: vec![],
        start: "00:00".into(),
        end: "23:59".into(),
        utc_offset_hours: None,
    };
    assert!(
        !window_never.is_active_now(),
        "no-days window should never be active"
    );
}

#[test]
fn weekday_from_str_variants() {
    assert_eq!(Weekday::parse("mon"), Some(Weekday::Mon));
    assert_eq!(Weekday::parse("Monday"), Some(Weekday::Mon));
    assert_eq!(Weekday::parse("FRI"), Some(Weekday::Fri));
    assert_eq!(Weekday::parse("invalid"), None);
}

#[test]
fn update_hook_enabled_toggles() {
    let dir = std::env::temp_dir().join(format!("void-hooks-test-{}", uuid::Uuid::new_v4()));
    let hook = Hook {
        name: "Toggle Test".into(),
        enabled: true,
        max_turns: 1,
        agent: "claude".into(),
        extra_args: Vec::new(),
        active_window: None,
        trigger: Trigger::NewMessage { connector: None },
        prompt: PromptConfig { text: "x".into() },
    };
    save_hook(&dir, &hook).unwrap();
    assert!(update_hook_enabled(&dir, "Toggle Test", false).unwrap());
    let loaded = find_hook(&dir, "Toggle Test").unwrap();
    assert!(!loaded.enabled);
    assert!(update_hook_enabled(&dir, "Toggle Test", true).unwrap());
    let loaded = find_hook(&dir, "Toggle Test").unwrap();
    assert!(loaded.enabled);
    assert!(!update_hook_enabled(&dir, "Nonexistent", true).unwrap());
    std::fs::remove_dir_all(&dir).ok();
}
