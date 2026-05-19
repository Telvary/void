//! Unipile LinkedIn posts and comments API.
//! https://developer.unipile.com/docs/posts-and-comments

use serde::Deserialize;
use serde_json::json;

use super::{ListResponse, UnipileClient};
use crate::error::LinkedInError;

fn encode_path_segment(segment: &str) -> String {
    urlencoding::encode(segment).into_owned()
}

/// Account owner profile (`GET /users/me`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AccountOwnerProfile {
    #[serde(default)]
    pub provider_id: String,
    #[serde(default)]
    pub public_identifier: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
}

impl AccountOwnerProfile {
    pub fn display_name(&self) -> String {
        match (&self.first_name, &self.last_name) {
            (Some(a), Some(b)) if !a.is_empty() || !b.is_empty() => {
                format!("{a} {b}").trim().to_string()
            }
            (Some(a), _) if !a.is_empty() => a.clone(),
            (_, Some(b)) if !b.is_empty() => b.clone(),
            _ => self.provider_id.clone(),
        }
    }
}

/// LinkedIn post from `GET /users/{id}/posts` or `GET /posts/{id}`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UnipilePost {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub social_id: String,
    #[serde(default)]
    pub share_url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub parsed_datetime: Option<String>,
    #[serde(default)]
    pub comment_counter: Option<i64>,
    #[serde(default)]
    pub reaction_counter: Option<i64>,
}

impl UnipilePost {
    pub fn display_label(&self) -> String {
        if let Some(title) = self.title.as_ref().filter(|t| !t.is_empty()) {
            return title.clone();
        }
        self.text
            .as_ref()
            .map(|t: &String| t.chars().take(80).collect::<String>())
            .filter(|t: &String| !t.is_empty())
            .unwrap_or_else(|| format!("Post {}", self.id))
    }
}

/// Comment on a post (`GET /posts/{social_id}/comments`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UnipileComment {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub post_id: Option<String>,
    #[serde(default)]
    pub post_urn: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub reply_counter: Option<i64>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub author_details: Option<UnipileCommentAuthor>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UnipileCommentAuthor {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub headline: Option<String>,
    #[serde(default)]
    pub profile_url: Option<String>,
    #[serde(default)]
    pub profile_picture_url: Option<String>,
}

impl UnipileComment {
    pub fn author_provider_id(&self) -> &str {
        self.author_details
            .as_ref()
            .and_then(|d| d.id.as_deref())
            .filter(|s| !s.is_empty())
            .or(self.author.as_deref().filter(|s| !s.is_empty()))
            .unwrap_or("unknown")
    }

    pub fn author_display_name(&self) -> String {
        self.author
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.author_provider_id().to_string())
    }
}

impl UnipileClient {
    pub async fn get_account_owner_profile(
        &self,
        account_id: &str,
    ) -> Result<AccountOwnerProfile, LinkedInError> {
        let value = self
            .get_json("users/me", &[("account_id", account_id.to_string())])
            .await?;
        serde_json::from_value(value).map_err(|e| LinkedInError::Decode(e.to_string()))
    }

    pub async fn get_post(
        &self,
        account_id: &str,
        post_id: &str,
    ) -> Result<UnipilePost, LinkedInError> {
        let path = format!("posts/{}", encode_path_segment(post_id));
        let value = self
            .get_json(&path, &[("account_id", account_id.to_string())])
            .await?;
        serde_json::from_value(value).map_err(|e| LinkedInError::Decode(e.to_string()))
    }

    pub async fn list_user_posts(
        &self,
        account_id: &str,
        user_identifier: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<ListResponse<UnipilePost>, LinkedInError> {
        let path = format!("users/{}/posts", encode_path_segment(user_identifier));
        let mut query = vec![
            ("account_id", account_id.to_string()),
            ("limit", limit.to_string()),
        ];
        if let Some(c) = cursor {
            query.push(("cursor", c.to_string()));
        }
        let value = self.get_json(&path, &query).await?;
        serde_json::from_value(value).map_err(|e| LinkedInError::Decode(e.to_string()))
    }

    pub async fn list_post_comments(
        &self,
        account_id: &str,
        post_social_id: &str,
        cursor: Option<&str>,
        comment_id: Option<&str>,
        limit: u32,
    ) -> Result<ListResponse<UnipileComment>, LinkedInError> {
        let path = format!("posts/{}/comments", encode_path_segment(post_social_id));
        let mut query = vec![
            ("account_id", account_id.to_string()),
            ("limit", limit.to_string()),
        ];
        if let Some(c) = cursor {
            query.push(("cursor", c.to_string()));
        }
        if let Some(parent) = comment_id {
            query.push(("comment_id", parent.to_string()));
        }
        let value = self.get_json(&path, &query).await?;
        serde_json::from_value(value).map_err(|e| LinkedInError::Decode(e.to_string()))
    }

    pub async fn send_post_comment(
        &self,
        account_id: &str,
        post_social_id: &str,
        text: &str,
        reply_to_comment_id: Option<&str>,
    ) -> Result<String, LinkedInError> {
        let path = format!("posts/{}/comments", encode_path_segment(post_social_id));
        let mut body = json!({
            "account_id": account_id,
            "text": text,
        });
        if let Some(cid) = reply_to_comment_id {
            body["comment_id"] = json!(cid);
        }

        let resp = self
            .http
            .post(self.url(&path))
            .header("X-API-KEY", &self.api_key)
            .header("accept", "application/json")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LinkedInError::Connection(e.to_string()))?;

        super::UnipileClient::parse_send_response(resp, "send post comment").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_POST_JSON: &str = r#"{
        "object": "PostList",
        "items": [{
            "provider": "LINKEDIN",
            "id": "7332661864792854528",
            "social_id": "urn:li:activity:7332661864792854528",
            "share_url": "https://www.linkedin.com/posts/example",
            "text": "Hello from my post",
            "parsed_datetime": "2026-05-26T19:01:02.468Z",
            "comment_counter": 3,
            "reaction_counter": 6
        }]
    }"#;

    const SAMPLE_COMMENT_JSON: &str = r#"{
        "object": "CommentList",
        "items": [{
            "object": "Comment",
            "id": "7335000001439513601",
            "post_id": "7332661864792854528",
            "post_urn": "urn:li:activity:7332661864792854528",
            "text": "Great post!",
            "date": "2026-05-27T10:00:00.000Z",
            "author": "Jane Doe",
            "author_details": {
                "id": "ACoAABFBQBcBtnr0Y6FNrtQpItSVnTX8Sxzl7Jg",
                "profile_url": "https://www.linkedin.com/in/jane"
            },
            "reply_counter": 1
        }]
    }"#;

    #[test]
    fn deserialize_post_list() {
        let list: ListResponse<UnipilePost> = serde_json::from_str(SAMPLE_POST_JSON).unwrap();
        assert_eq!(
            list.items[0].social_id,
            "urn:li:activity:7332661864792854528"
        );
        assert_eq!(list.items[0].comment_counter, Some(3));
    }

    #[test]
    fn deserialize_comment_list() {
        let list: ListResponse<UnipileComment> = serde_json::from_str(SAMPLE_COMMENT_JSON).unwrap();
        let c = &list.items[0];
        assert_eq!(c.id, "7335000001439513601");
        assert_eq!(c.author_display_name(), "Jane Doe");
        assert_eq!(
            c.author_provider_id(),
            "ACoAABFBQBcBtnr0Y6FNrtQpItSVnTX8Sxzl7Jg"
        );
    }

    #[test]
    fn encode_social_id_for_path() {
        assert_eq!(
            encode_path_segment("urn:li:activity:123"),
            "urn%3Ali%3Aactivity%3A123"
        );
    }
}
