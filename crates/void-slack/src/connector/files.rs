//! Slack file metadata and local download helpers.

use crate::api::SlackFile;

/// Build the `metadata.files[]` entry stored on messages.
pub(crate) fn file_metadata_entry(f: &SlackFile) -> serde_json::Value {
    serde_json::json!({
        "id": f.id,
        "name": f.name,
        "title": f.title,
        "mimetype": f.mimetype,
        "filetype": f.filetype,
        "size": f.size,
        "url_private": f.url_private,
        "url_private_download": f.url_private_download,
        "permalink": f.permalink,
        "is_external": f.is_external,
        "external_type": f.external_type,
    })
}

/// Build a `metadata.files[]` entry from a raw Slack event file object.
pub(crate) fn file_metadata_entry_from_event(f: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": f.get("id").and_then(|v| v.as_str()).unwrap_or(""),
        "name": f.get("name").and_then(|v| v.as_str()),
        "title": f.get("title").and_then(|v| v.as_str()),
        "mimetype": f.get("mimetype").and_then(|v| v.as_str()),
        "filetype": f.get("filetype").and_then(|v| v.as_str()),
        "size": f.get("size").and_then(|v| v.as_u64()),
        "url_private": f.get("url_private").and_then(|v| v.as_str()),
        "url_private_download": f.get("url_private_download").and_then(|v| v.as_str()),
        "permalink": f.get("permalink").and_then(|v| v.as_str()),
        "is_external": f.get("is_external").and_then(|v| v.as_bool()),
        "external_type": f.get("external_type").and_then(|v| v.as_str()),
    })
}

pub(crate) fn mark_download_skipped(file: &mut serde_json::Value, reason: &str) {
    file["download_skipped"] = serde_json::Value::Bool(true);
    file["download_skip_reason"] = serde_json::Value::String(reason.to_string());
}

/// Whether this URL points at a full file on Slack's CDN (not a thumbnail or third-party host).
pub(crate) fn is_slack_hosted_download_url(url: &str) -> bool {
    url.starts_with("https://files.slack.com/") && !url.contains("/files-tmb/")
}

fn is_external_file_metadata(file: &serde_json::Value) -> bool {
    if file
        .get("is_external")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return true;
    }
    let external_type = file
        .get("external_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !external_type.is_empty() {
        return true;
    }
    matches!(
        file.get("filetype").and_then(|v| v.as_str()),
        Some("gdoc" | "gslide" | "gsheet" | "gdrive" | "dropbox" | "box" | "remote")
    )
}

/// Pick the best download URL for a cached file entry, if any.
pub(crate) fn resolve_download_url(file: &serde_json::Value) -> Option<&str> {
    if file
        .get("download_skipped")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return None;
    }
    if is_external_file_metadata(file) {
        return None;
    }

    let private_download = file.get("url_private_download").and_then(|v| v.as_str());
    if let Some(url) = private_download.filter(|u| is_slack_hosted_download_url(u)) {
        return Some(url);
    }

    let private = file.get("url_private").and_then(|v| v.as_str());
    if let Some(url) = private.filter(|u| is_slack_hosted_download_url(u)) {
        return Some(url);
    }

    None
}

/// Why a file cannot be downloaded locally (external host, thumbnail-only, etc.).
pub(crate) fn skip_download_reason(file: &serde_json::Value) -> Option<&'static str> {
    if file
        .get("download_skipped")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return None;
    }
    if is_external_file_metadata(file) {
        return Some("external");
    }

    let private_download = file.get("url_private_download").and_then(|v| v.as_str());
    let private = file.get("url_private").and_then(|v| v.as_str());

    let has_slack_url = [private_download, private]
        .into_iter()
        .flatten()
        .any(is_slack_hosted_download_url);
    if has_slack_url {
        return None;
    }

    if [private_download, private]
        .into_iter()
        .flatten()
        .any(|u| !u.starts_with("https://files.slack.com/"))
    {
        return Some("external_url");
    }

    if [private_download, private]
        .into_iter()
        .flatten()
        .any(|u| u.contains("/files-tmb/"))
    {
        return Some("thumbnail_only");
    }

    if private_download.is_some() || private.is_some() {
        return Some("no_download_url");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_url_private_download_over_private() {
        let file = serde_json::json!({
            "url_private": "https://files.slack.com/files-tmb/T-F-thumb.mp4",
            "url_private_download": "https://files.slack.com/files-pri/T-F-F/full.mp4",
        });
        assert_eq!(
            resolve_download_url(&file),
            Some("https://files.slack.com/files-pri/T-F-F/full.mp4")
        );
    }

    #[test]
    fn rejects_thumbnail_url_without_download_link() {
        let file = serde_json::json!({
            "url_private": "https://files.slack.com/files-tmb/T-F-thumb.mp4",
        });
        assert!(resolve_download_url(&file).is_none());
        assert_eq!(skip_download_reason(&file), Some("thumbnail_only"));
    }

    #[test]
    fn skips_google_docs_external_files() {
        let file = serde_json::json!({
            "is_external": true,
            "external_type": "gdoc",
            "url_private": "https://docs.google.com/presentation/d/abc/edit",
        });
        assert!(resolve_download_url(&file).is_none());
        assert_eq!(skip_download_reason(&file), Some("external"));
    }

    #[test]
    fn skips_non_slack_urls_without_is_external_flag() {
        let file = serde_json::json!({
            "url_private": "https://docs.google.com/presentation/d/abc/edit",
        });
        assert!(resolve_download_url(&file).is_none());
        assert_eq!(skip_download_reason(&file), Some("external_url"));
    }
}
