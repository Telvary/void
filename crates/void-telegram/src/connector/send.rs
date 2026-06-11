use grammers_client::client::Client;
use grammers_client::message::InputMessage;
use grammers_client::peer::Peer;
use void_core::models::MessageContent;

fn text_for_message_content(content: &MessageContent) -> &str {
    match content {
        MessageContent::Text(text) => text.as_str(),
        MessageContent::File { caption, .. } => caption.as_deref().unwrap_or(""),
    }
}

pub(crate) fn build_input_message(content: &MessageContent) -> InputMessage {
    InputMessage::new().text(text_for_message_content(content))
}

pub(crate) fn parse_reply_id(message_id: &str) -> anyhow::Result<(String, String)> {
    let (conv, msg) = message_id
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid reply ID format: {message_id}"))?;
    Ok((conv.to_string(), msg.to_string()))
}

/// Strip the `telegram_<connection_id>_` prefix from an external ID, returning
/// the bare numeric portion. If the prefix is absent, the input is returned
/// unchanged (callers then attempt to parse it directly).
pub(crate) fn strip_telegram_prefix<'a>(raw: &'a str, connection_id: &str) -> &'a str {
    let prefix = format!("telegram_{connection_id}_");
    raw.strip_prefix(&prefix).unwrap_or(raw)
}

/// Resolve a user-provided recipient string to a Telegram peer.
/// Accepts: @username, username, phone number, or numeric chat ID.
pub(crate) async fn resolve_peer(client: &Client, input: &str) -> anyhow::Result<Peer> {
    let input = input.trim();

    if let Ok(id) = input.parse::<i64>() {
        let results = client.search_peer(&id.to_string(), 1).await?;
        if let Some(item) = results.into_iter().next() {
            return Ok(item.into_peer());
        }
        anyhow::bail!("could not resolve numeric peer ID: {input}");
    }

    let username = input.strip_prefix('@').unwrap_or(input);
    match client.resolve_username(username).await {
        Ok(Some(peer)) => return Ok(peer),
        Ok(None) => {}
        Err(e) => {
            tracing::debug!(input, error = %e, "resolve_username failed, falling back to search");
        }
    }

    let results = client.search_peer(input, 5).await?;
    if let Some(item) = results.into_iter().next() {
        return Ok(item.into_peer());
    }

    anyhow::bail!("could not resolve peer: {input}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reply_id_valid() {
        let (conv, msg) = parse_reply_id("telegram_conn_1_-100123:telegram_conn_1_42").unwrap();
        assert_eq!(conv, "telegram_conn_1_-100123");
        assert_eq!(msg, "telegram_conn_1_42");
    }

    #[test]
    fn parse_reply_id_splits_on_first_colon_only() {
        let (a, b) = parse_reply_id("left:mid:right").unwrap();
        assert_eq!(a, "left");
        assert_eq!(b, "mid:right");
    }

    #[test]
    fn parse_reply_id_invalid_no_colon() {
        let err = parse_reply_id("no-separator-here").unwrap_err();
        assert!(
            err.to_string().contains("invalid reply ID format"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_reply_id_empty_string_is_error() {
        let err = parse_reply_id("").unwrap_err();
        assert!(
            err.to_string().contains("invalid reply ID format"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_reply_id_leading_colon_yields_empty_conv() {
        let (conv, msg) = parse_reply_id(":42").unwrap();
        assert_eq!(conv, "");
        assert_eq!(msg, "42");
    }

    #[test]
    fn parse_reply_id_trailing_colon_yields_empty_msg() {
        let (conv, msg) = parse_reply_id("chat:").unwrap();
        assert_eq!(conv, "chat");
        assert_eq!(msg, "");
    }

    #[test]
    fn parse_reply_id_error_message_includes_input() {
        let err = parse_reply_id("xyz").unwrap_err();
        assert!(err.to_string().contains("xyz"), "unexpected error: {err}");
    }

    #[test]
    fn text_for_message_content_text_empty() {
        assert_eq!(
            text_for_message_content(&MessageContent::Text(String::new())),
            ""
        );
    }

    #[test]
    fn text_for_message_content_file_empty_caption() {
        let path = std::env::temp_dir().join("y.bin");
        assert_eq!(
            text_for_message_content(&MessageContent::File {
                path,
                caption: Some(String::new()),
                mime_type: Some("application/octet-stream".into()),
            }),
            ""
        );
    }

    #[test]
    fn strip_telegram_prefix_removes_matching_prefix() {
        assert_eq!(strip_telegram_prefix("telegram_conn1_42", "conn1"), "42");
        assert_eq!(
            strip_telegram_prefix("telegram_conn1_-100987654321", "conn1"),
            "-100987654321"
        );
    }

    #[test]
    fn strip_telegram_prefix_passthrough_when_absent() {
        // Raw numeric IDs with no prefix are returned unchanged.
        assert_eq!(strip_telegram_prefix("42", "conn1"), "42");
        assert_eq!(strip_telegram_prefix("-100123", "conn1"), "-100123");
    }

    #[test]
    fn strip_telegram_prefix_wrong_connection_id_not_stripped() {
        // Prefix for a different connection id must not be stripped.
        assert_eq!(
            strip_telegram_prefix("telegram_other_42", "conn1"),
            "telegram_other_42"
        );
    }

    #[test]
    fn strip_telegram_prefix_roundtrip_with_sync_format() {
        // Mirrors the external_id format built in sync.rs:
        // format!("telegram_{connection_id}_{msg_id}")
        let connection_id = "abc";
        let msg_id: i32 = 12345;
        let external_id = format!("telegram_{connection_id}_{msg_id}");
        let stripped = strip_telegram_prefix(&external_id, connection_id);
        assert_eq!(stripped.parse::<i32>().unwrap(), msg_id);

        let chat_id: i64 = -1001234567890;
        let conv_external_id = format!("telegram_{connection_id}_{chat_id}");
        let stripped_chat = strip_telegram_prefix(&conv_external_id, connection_id);
        assert_eq!(stripped_chat.parse::<i64>().unwrap(), chat_id);
    }

    #[test]
    fn strip_telegram_prefix_only_strips_once() {
        // Only the leading prefix is removed; an embedded repeat stays.
        assert_eq!(
            strip_telegram_prefix("telegram_c_telegram_c_5", "c"),
            "telegram_c_5"
        );
    }

    #[test]
    fn text_for_message_content_text() {
        assert_eq!(
            text_for_message_content(&MessageContent::Text("hello".into())),
            "hello"
        );
    }

    #[test]
    fn text_for_message_content_file_with_caption() {
        let path = std::env::temp_dir().join("x.png");
        assert_eq!(
            text_for_message_content(&MessageContent::File {
                path,
                caption: Some("see this".into()),
                mime_type: None,
            }),
            "see this"
        );
    }

    #[test]
    fn text_for_message_content_file_without_caption() {
        let path = std::env::temp_dir().join("x.png");
        assert_eq!(
            text_for_message_content(&MessageContent::File {
                path,
                caption: None,
                mime_type: None,
            }),
            ""
        );
    }
}
