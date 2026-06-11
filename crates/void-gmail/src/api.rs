use std::time::Duration;

use crate::error::GmailError;
use serde::Deserialize;
use tracing::{debug, info};

const DEFAULT_BASE_URL: &str = "https://gmail.googleapis.com";
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .expect("failed to build HTTP client")
}

/// Low-level Gmail API client.
pub struct GmailApiClient {
    http: reqwest::Client,
    access_token: String,
    base_url: String,
}

impl GmailApiClient {
    pub fn new(access_token: &str) -> Self {
        Self {
            http: build_http_client(),
            access_token: access_token.to_string(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    #[cfg(test)]
    pub fn with_base_url(access_token: &str, base_url: &str) -> Self {
        Self {
            http: build_http_client(),
            access_token: access_token.to_string(),
            base_url: base_url.to_string(),
        }
    }

    pub fn set_token(&mut self, token: &str) {
        self.access_token = token.to_string();
    }

    pub async fn get_profile(&self) -> Result<GmailProfile, GmailError> {
        debug!("gmail: get_profile");
        let resp: GmailProfile = self
            .http
            .get(format!("{}/gmail/v1/users/me/profile", self.base_url))
            .bearer_auth(&self.access_token)
            .send()
            .await?
            .json()
            .await?;
        debug!(email = ?resp.email_address, "gmail: got profile");
        Ok(resp)
    }

    pub async fn list_messages(
        &self,
        max_results: u32,
        page_token: Option<&str>,
        label_ids: Option<&[&str]>,
        query: Option<&str>,
    ) -> Result<MessageListResponse, GmailError> {
        debug!(
            max_results,
            has_page_token = page_token.is_some(),
            query,
            "gmail: list_messages"
        );
        let mut params = vec![("maxResults", max_results.to_string())];
        if let Some(pt) = page_token {
            params.push(("pageToken", pt.to_string()));
        }
        if let Some(labels) = label_ids {
            for label in labels {
                params.push(("labelIds", label.to_string()));
            }
        }
        if let Some(q) = query {
            params.push(("q", q.to_string()));
        }
        let resp: MessageListResponse = self
            .http
            .get(format!("{}/gmail/v1/users/me/messages", self.base_url))
            .bearer_auth(&self.access_token)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        let count = resp.messages.as_ref().map(|m| m.len()).unwrap_or(0);
        debug!(
            message_count = count,
            has_more = resp.next_page_token.is_some(),
            "gmail: listed messages"
        );
        Ok(resp)
    }

    pub async fn get_message(&self, message_id: &str) -> Result<GmailMessage, GmailError> {
        debug!(message_id, "gmail: get_message");
        let resp: GmailMessage = self
            .http
            .get(format!(
                "{}/gmail/v1/users/me/messages/{message_id}",
                self.base_url
            ))
            .bearer_auth(&self.access_token)
            .query(&[("format", "full")])
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    pub async fn list_history(
        &self,
        start_history_id: &str,
        label_id: Option<&str>,
    ) -> Result<HistoryListResponse, GmailError> {
        debug!(start_history_id, ?label_id, "gmail: list_history");
        let mut all_records: Vec<HistoryRecord> = Vec::new();
        let mut page_token: Option<String> = None;
        let mut latest_history_id: Option<String> = None;
        let max_pages = 10u32;

        for page in 0..max_pages {
            let mut params = vec![("startHistoryId", start_history_id.to_string())];
            if let Some(label) = label_id {
                params.push(("labelId", label.to_string()));
            }
            if let Some(pt) = &page_token {
                params.push(("pageToken", pt.clone()));
            }
            let resp: HistoryListResponse = self
                .http
                .get(format!("{}/gmail/v1/users/me/history", self.base_url))
                .bearer_auth(&self.access_token)
                .query(&params)
                .send()
                .await?
                .json()
                .await?;

            if let Some(records) = resp.history {
                let count = records.len();
                all_records.extend(records);
                debug!(page, record_count = count, "gmail: listed history page");
            }
            latest_history_id = resp.history_id.or(latest_history_id);
            page_token = resp.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        debug!(
            total_records = all_records.len(),
            "gmail: listed history (all pages)"
        );
        Ok(HistoryListResponse {
            history: if all_records.is_empty() {
                None
            } else {
                Some(all_records)
            },
            history_id: latest_history_id,
            next_page_token: None,
        })
    }

    pub async fn modify_message(
        &self,
        message_id: &str,
        add_labels: &[&str],
        remove_labels: &[&str],
    ) -> Result<GmailMessage, GmailError> {
        debug!(
            message_id,
            ?add_labels,
            ?remove_labels,
            "gmail: modify_message"
        );
        let body = serde_json::json!({
            "addLabelIds": add_labels,
            "removeLabelIds": remove_labels,
        });
        let resp: GmailMessage = self
            .http
            .post(format!(
                "{}/gmail/v1/users/me/messages/{message_id}/modify",
                self.base_url
            ))
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        debug!(message_id, "gmail: message modified");
        Ok(resp)
    }

    pub async fn send_message(&self, raw: &str) -> Result<GmailMessage, GmailError> {
        info!("gmail: send_message");
        let body = serde_json::json!({ "raw": raw });
        let resp: GmailMessage = self
            .http
            .post(format!("{}/gmail/v1/users/me/messages/send", self.base_url))
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        debug!(message_id = ?resp.id, "gmail: sent message");
        Ok(resp)
    }

    pub async fn get_thread(&self, thread_id: &str) -> Result<GmailThread, GmailError> {
        debug!(thread_id, "gmail: get_thread");
        let resp: GmailThread = self
            .http
            .get(format!(
                "{}/gmail/v1/users/me/threads/{thread_id}",
                self.base_url
            ))
            .bearer_auth(&self.access_token)
            .query(&[("format", "full")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let msg_count = resp.messages.as_ref().map(|m| m.len()).unwrap_or(0);
        debug!(thread_id, msg_count, "gmail: get_thread ok");
        Ok(resp)
    }

    pub async fn get_attachment(
        &self,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<AttachmentResponse, GmailError> {
        debug!(message_id, attachment_id, "gmail: get_attachment");
        let resp: AttachmentResponse = self
            .http
            .get(format!(
                "{}/gmail/v1/users/me/messages/{message_id}/attachments/{attachment_id}",
                self.base_url
            ))
            .bearer_auth(&self.access_token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        debug!(message_id, attachment_id, "gmail: get_attachment ok");
        Ok(resp)
    }

    pub async fn list_labels(&self) -> Result<LabelListResponse, GmailError> {
        debug!("gmail: list_labels");
        let resp: LabelListResponse = self
            .http
            .get(format!("{}/gmail/v1/users/me/labels", self.base_url))
            .bearer_auth(&self.access_token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let count = resp.labels.as_ref().map(|l| l.len()).unwrap_or(0);
        debug!(count, "gmail: list_labels ok");
        Ok(resp)
    }

    pub async fn modify_thread(
        &self,
        thread_id: &str,
        add_labels: &[&str],
        remove_labels: &[&str],
    ) -> Result<GmailThread, GmailError> {
        debug!(
            thread_id,
            ?add_labels,
            ?remove_labels,
            "gmail: modify_thread"
        );
        let body = serde_json::json!({
            "addLabelIds": add_labels,
            "removeLabelIds": remove_labels,
        });
        let resp: GmailThread = self
            .http
            .post(format!(
                "{}/gmail/v1/users/me/threads/{thread_id}/modify",
                self.base_url
            ))
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        debug!(thread_id, "gmail: modify_thread ok");
        Ok(resp)
    }

    pub async fn batch_modify_messages(
        &self,
        message_ids: &[&str],
        add_labels: &[&str],
        remove_labels: &[&str],
    ) -> Result<(), GmailError> {
        debug!(
            ?message_ids,
            ?add_labels,
            ?remove_labels,
            "gmail: batch_modify"
        );
        let body = serde_json::json!({
            "ids": message_ids,
            "addLabelIds": add_labels,
            "removeLabelIds": remove_labels,
        });
        self.http
            .post(format!(
                "{}/gmail/v1/users/me/messages/batchModify",
                self.base_url
            ))
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        debug!("gmail: batch_modify ok");
        Ok(())
    }

    pub async fn list_drafts(&self, max_results: u32) -> Result<DraftListResponse, GmailError> {
        debug!(max_results, "gmail: list_drafts");
        let resp: DraftListResponse = self
            .http
            .get(format!("{}/gmail/v1/users/me/drafts", self.base_url))
            .bearer_auth(&self.access_token)
            .query(&[("maxResults", max_results.to_string())])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let count = resp.drafts.as_ref().map(|d| d.len()).unwrap_or(0);
        debug!(count, "gmail: list_drafts ok");
        Ok(resp)
    }

    pub async fn get_draft(&self, draft_id: &str) -> Result<GmailDraft, GmailError> {
        debug!(draft_id, "gmail: get_draft");
        let resp: GmailDraft = self
            .http
            .get(format!(
                "{}/gmail/v1/users/me/drafts/{draft_id}",
                self.base_url
            ))
            .bearer_auth(&self.access_token)
            .query(&[("format", "full")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        debug!(draft_id, "gmail: get_draft ok");
        Ok(resp)
    }

    pub async fn create_draft(
        &self,
        raw: &str,
        thread_id: Option<&str>,
    ) -> Result<GmailDraft, GmailError> {
        info!("gmail: create_draft");
        let mut message = serde_json::json!({ "raw": raw });
        if let Some(tid) = thread_id {
            message["threadId"] = serde_json::Value::String(tid.to_string());
        }
        let body = serde_json::json!({ "message": message });
        let resp: GmailDraft = self
            .http
            .post(format!("{}/gmail/v1/users/me/drafts", self.base_url))
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        debug!(draft_id = ?resp.id, "gmail: create_draft ok");
        Ok(resp)
    }

    pub async fn update_draft(&self, draft_id: &str, raw: &str) -> Result<GmailDraft, GmailError> {
        debug!(draft_id, "gmail: update_draft");
        let body = serde_json::json!({
            "message": { "raw": raw }
        });
        let resp: GmailDraft = self
            .http
            .put(format!(
                "{}/gmail/v1/users/me/drafts/{draft_id}",
                self.base_url
            ))
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        debug!(draft_id, "gmail: update_draft ok");
        Ok(resp)
    }

    pub async fn delete_draft(&self, draft_id: &str) -> Result<(), GmailError> {
        debug!(draft_id, "gmail: delete_draft");
        self.http
            .delete(format!(
                "{}/gmail/v1/users/me/drafts/{draft_id}",
                self.base_url
            ))
            .bearer_auth(&self.access_token)
            .send()
            .await?
            .error_for_status()?;
        debug!(draft_id, "gmail: delete_draft ok");
        Ok(())
    }
}

// -- Gmail API types --

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailProfile {
    pub email_address: Option<String>,
    pub history_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageListResponse {
    pub messages: Option<Vec<MessageRef>>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessageRef {
    pub id: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailMessage {
    pub id: Option<String>,
    pub thread_id: Option<String>,
    pub snippet: Option<String>,
    pub internal_date: Option<String>,
    pub label_ids: Option<Vec<String>>,
    pub payload: Option<MessagePayload>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePayload {
    pub mime_type: Option<String>,
    pub headers: Option<Vec<MessageHeader>>,
    pub body: Option<MessagePartBody>,
    pub parts: Option<Vec<MessagePart>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePart {
    pub mime_type: Option<String>,
    pub filename: Option<String>,
    pub headers: Option<Vec<MessageHeader>>,
    pub body: Option<MessagePartBody>,
    pub parts: Option<Vec<MessagePart>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePartBody {
    pub data: Option<String>,
    pub size: Option<u64>,
    pub attachment_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessageHeader {
    pub name: String,
    pub value: String,
}

impl GmailMessage {
    pub fn get_header(&self, name: &str) -> Option<String> {
        self.payload
            .as_ref()?
            .headers
            .as_ref()?
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.clone())
    }

    /// Extract the plain text body by walking the MIME tree.
    pub fn text_body(&self) -> Option<String> {
        self.payload
            .as_ref()
            .and_then(|p| extract_body_by_mime(p, "text/plain"))
    }

    /// Extract the HTML body by walking the MIME tree.
    pub fn html_body(&self) -> Option<String> {
        self.payload
            .as_ref()
            .and_then(|p| extract_body_by_mime(p, "text/html"))
    }

    /// Return the attachment_id for the text/plain part when data is absent (large body).
    pub fn text_body_attachment_id(&self) -> Option<String> {
        self.payload
            .as_ref()
            .and_then(|p| find_attachment_id_by_mime(p, "text/plain"))
    }

    /// Return the attachment_id for the text/html part when data is absent (large body).
    pub fn html_body_attachment_id(&self) -> Option<String> {
        self.payload
            .as_ref()
            .and_then(|p| find_attachment_id_by_mime(p, "text/html"))
    }

    /// Extract all file attachments (parts with a non-empty filename and an attachment_id).
    pub fn file_attachments(&self) -> Vec<FileAttachment> {
        let mut result = Vec::new();
        if let Some(payload) = &self.payload {
            if let Some(parts) = &payload.parts {
                for part in parts {
                    collect_file_attachments(part, &mut result);
                }
            }
        }
        result
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileAttachment {
    pub filename: String,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
    pub attachment_id: String,
}

fn extract_body_by_mime(payload: &MessagePayload, target_mime: &str) -> Option<String> {
    if let Some(mime) = &payload.mime_type {
        if mime == target_mime {
            return decode_body_data(&payload.body);
        }
    }

    if let Some(parts) = &payload.parts {
        for part in parts {
            if let Some(result) = extract_body_from_part(part, target_mime) {
                return Some(result);
            }
        }
    }

    None
}

fn extract_body_from_part(part: &MessagePart, target_mime: &str) -> Option<String> {
    if let Some(mime) = &part.mime_type {
        if mime == target_mime {
            return decode_body_data(&part.body);
        }
    }

    if let Some(sub_parts) = &part.parts {
        for sub in sub_parts {
            if let Some(result) = extract_body_from_part(sub, target_mime) {
                return Some(result);
            }
        }
    }

    None
}

fn decode_body_data(body: &Option<MessagePartBody>) -> Option<String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    let data = body.as_ref()?.data.as_deref()?;
    let bytes = URL_SAFE_NO_PAD.decode(data.trim_end_matches('=')).ok()?;
    String::from_utf8(bytes).ok()
}

pub fn decode_attachment_data(data: &str) -> Option<String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    let bytes = URL_SAFE_NO_PAD.decode(data.trim_end_matches('=')).ok()?;
    String::from_utf8(bytes).ok()
}

fn collect_file_attachments(part: &MessagePart, out: &mut Vec<FileAttachment>) {
    if let Some(filename) = &part.filename {
        if !filename.is_empty() {
            if let Some(aid) = part.body.as_ref().and_then(|b| b.attachment_id.as_ref()) {
                out.push(FileAttachment {
                    filename: filename.clone(),
                    mime_type: part.mime_type.clone(),
                    size: part.body.as_ref().and_then(|b| b.size),
                    attachment_id: aid.clone(),
                });
            }
        }
    }
    if let Some(sub_parts) = &part.parts {
        for sub in sub_parts {
            collect_file_attachments(sub, out);
        }
    }
}

fn find_attachment_id_by_mime(payload: &MessagePayload, target_mime: &str) -> Option<String> {
    if let Some(mime) = &payload.mime_type {
        if mime == target_mime {
            if let Some(body) = &payload.body {
                if body.data.is_none() {
                    return body.attachment_id.clone();
                }
            }
        }
    }
    if let Some(parts) = &payload.parts {
        for part in parts {
            if let Some(id) = find_attachment_id_in_part(part, target_mime) {
                return Some(id);
            }
        }
    }
    None
}

fn find_attachment_id_in_part(part: &MessagePart, target_mime: &str) -> Option<String> {
    if let Some(mime) = &part.mime_type {
        if mime == target_mime {
            if let Some(body) = &part.body {
                if body.data.is_none() {
                    return body.attachment_id.clone();
                }
            }
        }
    }
    if let Some(sub_parts) = &part.parts {
        for sub in sub_parts {
            if let Some(id) = find_attachment_id_in_part(sub, target_mime) {
                return Some(id);
            }
        }
    }
    None
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryListResponse {
    pub history: Option<Vec<HistoryRecord>>,
    pub history_id: Option<String>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub messages_added: Option<Vec<HistoryMessageAdded>>,
    pub labels_added: Option<Vec<HistoryLabelChange>>,
    pub labels_removed: Option<Vec<HistoryLabelChange>>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryMessageAdded {
    pub message: MessageRef,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryLabelChange {
    pub message: MessageRef,
    pub label_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailThread {
    pub id: Option<String>,
    pub snippet: Option<String>,
    pub messages: Option<Vec<GmailMessage>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentResponse {
    pub data: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct LabelListResponse {
    pub labels: Option<Vec<GmailLabel>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailLabel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub label_type: Option<String>,
    pub messages_total: Option<u64>,
    pub messages_unread: Option<u64>,
    pub threads_total: Option<u64>,
    pub threads_unread: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct DraftListResponse {
    pub drafts: Option<Vec<DraftRef>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftRef {
    pub id: String,
    pub message: Option<MessageRef>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailDraft {
    pub id: Option<String>,
    pub message: Option<GmailMessage>,
}

#[cfg(test)]
mod api_tests {
    use super::*;
    use crate::error::GmailError;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // -- Happy-path parsing --

    #[tokio::test]
    async fn get_message_parses_threading_and_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/messages/m1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "m1",
                "threadId": "t1",
                "snippet": "Hello there",
                "internalDate": "1741700000000",
                "labelIds": ["INBOX", "UNREAD"],
                "payload": {
                    "mimeType": "text/plain",
                    "headers": [
                        {"name": "From", "value": "sender@example.com"},
                        {"name": "Subject", "value": "Greetings"}
                    ],
                    "body": {"data": "SGVsbG8gV29ybGQ", "size": 11}
                }
            })))
            .mount(&server)
            .await;

        let api = GmailApiClient::with_base_url("test-token", &server.uri());
        let msg = api.get_message("m1").await.unwrap();
        assert_eq!(msg.id.as_deref(), Some("m1"));
        assert_eq!(msg.thread_id.as_deref(), Some("t1"));
        assert_eq!(msg.snippet.as_deref(), Some("Hello there"));
        assert_eq!(
            msg.label_ids.as_ref().unwrap(),
            &vec!["INBOX".to_string(), "UNREAD".to_string()]
        );
        assert_eq!(
            msg.get_header("from").as_deref(),
            Some("sender@example.com")
        );
        assert_eq!(msg.get_header("Subject").as_deref(), Some("Greetings"));
        assert_eq!(msg.text_body().as_deref(), Some("Hello World"));
    }

    #[tokio::test]
    async fn get_thread_parses_messages() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/threads/t1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "t1",
                "snippet": "Conversation",
                "messages": [
                    {"id": "m1", "threadId": "t1", "snippet": "first"},
                    {"id": "m2", "threadId": "t1", "snippet": "second"}
                ]
            })))
            .mount(&server)
            .await;

        let api = GmailApiClient::with_base_url("test-token", &server.uri());
        let thread = api.get_thread("t1").await.unwrap();
        assert_eq!(thread.id.as_deref(), Some("t1"));
        let msgs = thread.messages.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].id.as_deref(), Some("m1"));
        assert_eq!(msgs[1].id.as_deref(), Some("m2"));
    }

    #[tokio::test]
    async fn list_labels_parses_two_labels() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/labels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "labels": [
                    {"id": "INBOX", "name": "INBOX", "type": "system"},
                    {"id": "Label_1", "name": "Work", "type": "user"}
                ]
            })))
            .mount(&server)
            .await;

        let api = GmailApiClient::with_base_url("test-token", &server.uri());
        let resp = api.list_labels().await.unwrap();
        let labels = resp.labels.unwrap();
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].id, "INBOX");
        assert_eq!(labels[1].name, "Work");
    }

    /// Regression: `list_history` must consume all internal pages (was a real bug).
    #[tokio::test]
    async fn list_history_consumes_two_pages() {
        let server = MockServer::start().await;
        // Page 1: has nextPageToken -> must trigger a second request.
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/history"))
            .and(query_param_is_missing("pageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "history": [
                    {"messagesAdded": [{"message": {"id": "m1", "threadId": "t1"}}]}
                ],
                "historyId": "100",
                "nextPageToken": "page2"
            })))
            .mount(&server)
            .await;
        // Page 2: terminal (no nextPageToken).
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/history"))
            .and(query_param("pageToken", "page2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "history": [
                    {"messagesAdded": [{"message": {"id": "m2", "threadId": "t2"}}]}
                ],
                "historyId": "200"
            })))
            .mount(&server)
            .await;

        let api = GmailApiClient::with_base_url("test-token", &server.uri());
        let resp = api.list_history("50", None).await.unwrap();
        let records = resp.history.unwrap();
        // Both pages must be present.
        assert_eq!(records.len(), 2);
        let ids: Vec<&str> = records
            .iter()
            .filter_map(|r| r.messages_added.as_ref())
            .flat_map(|ma| ma.iter().map(|m| m.message.id.as_str()))
            .collect();
        assert_eq!(ids, vec!["m1", "m2"]);
        // Latest history id is from the last page; aggregated token cleared.
        assert_eq!(resp.history_id.as_deref(), Some("200"));
        assert!(resp.next_page_token.is_none());
    }

    // -- Error paths --

    /// `list_messages` goes straight to `.json()`, so an error body is a DECODE error.
    #[tokio::test]
    async fn list_messages_401_surfaces_decode_error_not_panic() {
        let server = MockServer::start().await;
        // A real Gmail 401 returns an error body whose `messages` (if present) is not
        // an array; here the top-level is an array, which cannot decode into the
        // struct -> reqwest decode error (never a panic).
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/messages"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(serde_json::json!(["invalid", "credentials"])),
            )
            .mount(&server)
            .await;

        let api = GmailApiClient::with_base_url("test-token", &server.uri());
        let err = api
            .list_messages(10, None, None, None)
            .await
            .expect_err("expected error");
        // Error body does not match MessageListResponse -> reqwest decode error.
        assert!(matches!(err, GmailError::Http(_)), "got {err:?}");
    }

    /// `get_message` also decodes directly; 5xx with non-matching body -> decode error.
    #[tokio::test]
    async fn get_message_5xx_surfaces_decode_error() {
        let server = MockServer::start().await;
        // Non-JSON / non-object body cannot decode into GmailMessage -> decode error.
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/messages/m1"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let api = GmailApiClient::with_base_url("test-token", &server.uri());
        let err = api.get_message("m1").await.expect_err("expected error");
        assert!(matches!(err, GmailError::Http(_)), "got {err:?}");
    }

    /// `list_labels` calls `.error_for_status()`, so HTTP status is preserved.
    #[tokio::test]
    async fn list_labels_401_preserves_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/labels"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": {"code": 401, "message": "Invalid Credentials"}
            })))
            .mount(&server)
            .await;

        let api = GmailApiClient::with_base_url("test-token", &server.uri());
        let err = api.list_labels().await.expect_err("expected error");
        match err {
            GmailError::Http(e) => assert_eq!(e.status(), Some(reqwest::StatusCode::UNAUTHORIZED)),
            other => panic!("expected Http error, got {other:?}"),
        }
    }

    /// `get_thread` preserves status via `.error_for_status()`.
    #[tokio::test]
    async fn get_thread_500_preserves_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/threads/t1"))
            .respond_with(ResponseTemplate::new(500).set_body_string("oops"))
            .mount(&server)
            .await;

        let api = GmailApiClient::with_base_url("test-token", &server.uri());
        let err = api.get_thread("t1").await.expect_err("expected error");
        match err {
            GmailError::Http(e) => {
                assert_eq!(e.status(), Some(reqwest::StatusCode::INTERNAL_SERVER_ERROR))
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }

    /// `create_draft` preserves status (e.g. 429 rate-limit) via `.error_for_status()`.
    #[tokio::test]
    async fn create_draft_429_preserves_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gmail/v1/users/me/drafts"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
            .mount(&server)
            .await;

        let api = GmailApiClient::with_base_url("test-token", &server.uri());
        let err = api
            .create_draft("cmF3", None)
            .await
            .expect_err("expected error");
        match err {
            GmailError::Http(e) => {
                assert_eq!(e.status(), Some(reqwest::StatusCode::TOO_MANY_REQUESTS))
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }

    /// Malformed JSON (missing required `id` on a MessageRef) -> clean Err, no panic.
    #[tokio::test]
    async fn list_messages_malformed_json_is_clean_err() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gmail/v1/users/me/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "messages": [{"threadId": "t1"}]
            })))
            .mount(&server)
            .await;

        let api = GmailApiClient::with_base_url("test-token", &server.uri());
        let err = api
            .list_messages(10, None, None, None)
            .await
            .expect_err("expected decode error for missing id");
        assert!(matches!(err, GmailError::Http(_)), "got {err:?}");
    }
}
