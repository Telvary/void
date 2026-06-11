use std::collections::HashMap;

use crate::error::CalendarError;
use serde::{Deserialize, Serialize};
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://www.googleapis.com";

/// Google Calendar API client.
pub struct CalendarApiClient {
    http: reqwest::Client,
    access_token: String,
    base_url: String,
}

impl CalendarApiClient {
    pub fn new(access_token: &str) -> Self {
        Self {
            http: void_gmail::api::build_http_client(),
            access_token: access_token.to_string(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    #[cfg(test)]
    pub fn with_base_url(access_token: &str, base_url: &str) -> Self {
        Self {
            http: void_gmail::api::build_http_client(),
            access_token: access_token.to_string(),
            base_url: base_url.to_string(),
        }
    }

    pub async fn list_calendars(&self) -> Result<CalendarListResponse, CalendarError> {
        debug!("calendar: list_calendars");
        let resp: CalendarListResponse = self
            .http
            .get(format!(
                "{}/calendar/v3/users/me/calendarList",
                self.base_url
            ))
            .bearer_auth(&self.access_token)
            .send()
            .await?
            .json()
            .await
            .map_err(CalendarError::from)?;
        let count = resp.items.as_ref().map(|i| i.len()).unwrap_or(0);
        debug!(count, "calendar: list_calendars ok");
        Ok(resp)
    }

    pub async fn list_events(
        &self,
        calendar_id: &str,
        time_min: Option<&str>,
        time_max: Option<&str>,
        sync_token: Option<&str>,
        page_token: Option<&str>,
    ) -> Result<EventListResponse, CalendarError> {
        debug!(
            calendar_id,
            time_min = ?time_min,
            time_max = ?time_max,
            "calendar: list_events"
        );
        let mut params: Vec<(&str, String)> = vec![("maxResults", "2500".into())];

        if sync_token.is_some() {
            // syncToken is incompatible with singleEvents, orderBy, timeMin, timeMax
            if let Some(st) = sync_token {
                params.push(("syncToken", st.into()));
            }
            // showDeleted must be true (default) to receive cancelled events
        } else {
            // singleEvents expands recurring instances for display; orderBy must not be
            // set — Google Calendar API does not return nextSyncToken when orderBy is used.
            params.push(("singleEvents", "true".into()));
            if let Some(t) = time_min {
                params.push(("timeMin", t.into()));
            }
            if let Some(t) = time_max {
                params.push(("timeMax", t.into()));
            }
        }
        if let Some(pt) = page_token {
            params.push(("pageToken", pt.into()));
        }

        let url = format!(
            "{}/calendar/v3/calendars/{}/events",
            self.base_url,
            urlencoded(calendar_id)
        );
        let resp: EventListResponse = self
            .http
            .get(&url)
            .bearer_auth(&self.access_token)
            .query(&params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .map_err(CalendarError::from)?;
        let count = resp.items.as_ref().map(|i| i.len()).unwrap_or(0);
        let has_sync_token = resp.next_sync_token.is_some();
        let has_page_token = resp.next_page_token.is_some();
        debug!(
            count,
            has_sync_token, has_page_token, "calendar: list_events ok"
        );
        Ok(resp)
    }

    /// Unfiltered events.list for sync-token bootstrap (no timeMin/timeMax/singleEvents/orderBy).
    pub async fn list_events_sync_bootstrap(
        &self,
        calendar_id: &str,
        page_token: Option<&str>,
    ) -> Result<EventListResponse, CalendarError> {
        debug!(calendar_id, "calendar: list_events_sync_bootstrap");
        let mut params: Vec<(&str, String)> = vec![("maxResults", "2500".into())];
        if let Some(pt) = page_token {
            params.push(("pageToken", pt.into()));
        }

        let url = format!(
            "{}/calendar/v3/calendars/{}/events",
            self.base_url,
            urlencoded(calendar_id)
        );
        let resp: EventListResponse = self
            .http
            .get(&url)
            .bearer_auth(&self.access_token)
            .query(&params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .map_err(CalendarError::from)?;
        debug!(
            count = resp.items.as_ref().map(|i| i.len()).unwrap_or(0),
            has_sync_token = resp.next_sync_token.is_some(),
            has_page_token = resp.next_page_token.is_some(),
            "calendar: list_events_sync_bootstrap ok"
        );
        Ok(resp)
    }

    pub async fn insert_event(
        &self,
        calendar_id: &str,
        event: &InsertEventRequest,
        conference_data_version: Option<u32>,
        send_updates: Option<&str>,
    ) -> Result<GoogleCalendarEvent, CalendarError> {
        debug!(
            calendar_id,
            summary = event.summary.as_str(),
            "calendar: insert_event"
        );
        let url = format!(
            "{}/calendar/v3/calendars/{}/events",
            self.base_url,
            urlencoded(calendar_id)
        );
        let mut req = self
            .http
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(event);

        if let Some(v) = conference_data_version {
            req = req.query(&[("conferenceDataVersion", v.to_string())]);
        }
        if let Some(su) = send_updates {
            req = req.query(&[("sendUpdates", su)]);
        }

        let resp: GoogleCalendarEvent = req
            .send()
            .await?
            .json()
            .await
            .map_err(CalendarError::from)?;
        let event_id = resp.id.as_deref().unwrap_or("(none)");
        debug!(event_id, "calendar: insert_event ok");
        Ok(resp)
    }

    pub async fn get_event(
        &self,
        calendar_id: &str,
        event_id: &str,
    ) -> Result<GoogleCalendarEvent, CalendarError> {
        debug!(calendar_id, event_id, "calendar: get_event");
        let url = format!(
            "{}/calendar/v3/calendars/{}/events/{}",
            self.base_url,
            urlencoded(calendar_id),
            urlencoded(event_id)
        );
        let resp: GoogleCalendarEvent = self
            .http
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .map_err(CalendarError::from)?;
        debug!(event_id, "calendar: get_event ok");
        Ok(resp)
    }

    pub async fn search_events(
        &self,
        calendar_id: &str,
        query: &str,
        time_min: Option<&str>,
        time_max: Option<&str>,
    ) -> Result<EventListResponse, CalendarError> {
        debug!(calendar_id, query, "calendar: search_events");
        let mut params: Vec<(&str, String)> = vec![
            ("singleEvents", "true".into()),
            ("orderBy", "startTime".into()),
            ("maxResults", "2500".into()),
            ("q", query.into()),
        ];
        if let Some(t) = time_min {
            params.push(("timeMin", t.into()));
        }
        if let Some(t) = time_max {
            params.push(("timeMax", t.into()));
        }
        let url = format!(
            "{}/calendar/v3/calendars/{}/events",
            self.base_url,
            urlencoded(calendar_id)
        );
        let resp: EventListResponse = self
            .http
            .get(&url)
            .bearer_auth(&self.access_token)
            .query(&params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .map_err(CalendarError::from)?;
        let count = resp.items.as_ref().map(|i| i.len()).unwrap_or(0);
        debug!(count, "calendar: search_events ok");
        Ok(resp)
    }

    pub async fn update_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        update: &UpdateEventRequest,
        send_updates: Option<&str>,
    ) -> Result<GoogleCalendarEvent, CalendarError> {
        debug!(calendar_id, event_id, "calendar: update_event");
        let url = format!(
            "{}/calendar/v3/calendars/{}/events/{}",
            self.base_url,
            urlencoded(calendar_id),
            urlencoded(event_id)
        );
        let mut req = self
            .http
            .patch(&url)
            .bearer_auth(&self.access_token)
            .json(update);
        if let Some(su) = send_updates {
            req = req.query(&[("sendUpdates", su)]);
        }
        let resp: GoogleCalendarEvent = req.send().await?.error_for_status()?.json().await?;
        debug!(event_id, "calendar: update_event ok");
        Ok(resp)
    }

    pub async fn freebusy(
        &self,
        time_min: &str,
        time_max: &str,
        emails: &[String],
    ) -> Result<FreeBusyResponse, CalendarError> {
        debug!(
            time_min,
            time_max,
            attendees = emails.len(),
            "calendar: freebusy"
        );
        let items: Vec<serde_json::Value> = emails
            .iter()
            .map(|e| serde_json::json!({ "id": e }))
            .collect();
        let body = serde_json::json!({
            "timeMin": time_min,
            "timeMax": time_max,
            "items": items,
        });
        let url = format!("{}/calendar/v3/freeBusy", self.base_url);
        let resp: FreeBusyResponse = self
            .http
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        debug!(calendars = resp.calendars.len(), "calendar: freebusy ok");
        Ok(resp)
    }

    pub async fn delete_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        send_updates: Option<&str>,
    ) -> Result<(), CalendarError> {
        debug!(calendar_id, event_id, "calendar: delete_event");
        let url = format!(
            "{}/calendar/v3/calendars/{}/events/{}",
            self.base_url,
            urlencoded(calendar_id),
            urlencoded(event_id)
        );
        let mut req = self.http.delete(&url).bearer_auth(&self.access_token);
        if let Some(su) = send_updates {
            req = req.query(&[("sendUpdates", su)]);
        }
        req.send().await?.error_for_status()?;
        debug!(event_id, "calendar: delete_event ok");
        Ok(())
    }
}

fn urlencoded(s: &str) -> String {
    s.replace('#', "%23").replace(' ', "%20")
}

// -- Calendar API types --

#[derive(Debug, Deserialize)]
pub struct CalendarListResponse {
    pub items: Option<Vec<CalendarListEntry>>,
}

#[derive(Debug, Deserialize)]
pub struct CalendarListEntry {
    pub id: String,
    pub summary: Option<String>,
    pub primary: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventListResponse {
    pub items: Option<Vec<GoogleCalendarEvent>>,
    pub next_sync_token: Option<String>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleCalendarEvent {
    pub id: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: Option<EventDateTime>,
    pub end: Option<EventDateTime>,
    pub status: Option<String>,
    pub attendees: Option<Vec<EventAttendee>>,
    pub conference_data: Option<ConferenceData>,
    pub html_link: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDateTime {
    pub date_time: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EventAttendee {
    pub email: Option<String>,
    #[serde(rename = "responseStatus")]
    pub response_status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConferenceData {
    pub entry_points: Option<Vec<EntryPoint>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPoint {
    pub entry_point_type: Option<String>,
    pub uri: Option<String>,
}

// -- FreeBusy types --

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeBusyResponse {
    pub time_min: Option<String>,
    pub time_max: Option<String>,
    #[serde(default)]
    pub calendars: HashMap<String, FreeBusyCalendar>,
}

#[derive(Debug, Deserialize)]
pub struct FreeBusyCalendar {
    #[serde(default)]
    pub busy: Vec<FreeBusySlot>,
    #[serde(default)]
    pub errors: Vec<FreeBusyError>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FreeBusySlot {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Deserialize)]
pub struct FreeBusyError {
    pub domain: Option<String>,
    pub reason: Option<String>,
}

// -- Request types --

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertEventRequest {
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub start: EventDateTimeRequest,
    pub end: EventDateTimeRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attendees: Option<Vec<AttendeeRequest>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conference_data: Option<ConferenceDataRequest>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEventRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<EventDateTimeRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<EventDateTimeRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attendees: Option<Vec<AttendeeResponseRequest>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendeeResponseRequest {
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDateTimeRequest {
    pub date_time: String,
    pub time_zone: String,
}

#[derive(Debug, Serialize)]
pub struct AttendeeRequest {
    pub email: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConferenceDataRequest {
    pub create_request: CreateConferenceRequest,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConferenceRequest {
    pub request_id: String,
    pub conference_solution_key: ConferenceSolutionKey,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConferenceSolutionKey {
    #[serde(rename = "type")]
    pub key_type: String,
}

#[cfg(test)]
mod api_tests {
    use super::*;
    use crate::error::CalendarError;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // -- Happy-path parsing --

    #[tokio::test]
    async fn list_events_parses_attendees_and_page_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendar/v3/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "ev1",
                        "summary": "Standup",
                        "location": "Room A",
                        "status": "confirmed",
                        "start": {"dateTime": "2026-06-11T09:00:00Z"},
                        "end": {"dateTime": "2026-06-11T09:30:00Z"},
                        "attendees": [
                            {"email": "a@example.com", "responseStatus": "accepted"},
                            {"email": "b@example.com", "responseStatus": "needsAction"}
                        ],
                        "htmlLink": "https://cal/ev1"
                    },
                    {
                        "id": "ev2",
                        "summary": "All day",
                        "start": {"date": "2026-06-12"},
                        "end": {"date": "2026-06-13"}
                    }
                ],
                "nextPageToken": "page2"
            })))
            .mount(&server)
            .await;

        let api = CalendarApiClient::with_base_url("test-token", &server.uri());
        let resp = api
            .list_events("primary", Some("2026-06-11T00:00:00Z"), None, None, None)
            .await
            .unwrap();
        let items = resp.items.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id.as_deref(), Some("ev1"));
        assert_eq!(items[0].summary.as_deref(), Some("Standup"));
        assert_eq!(
            items[0].start.as_ref().unwrap().date_time.as_deref(),
            Some("2026-06-11T09:00:00Z")
        );
        let attendees = items[0].attendees.as_ref().unwrap();
        assert_eq!(attendees.len(), 2);
        assert_eq!(attendees[0].email.as_deref(), Some("a@example.com"));
        assert_eq!(attendees[0].response_status.as_deref(), Some("accepted"));
        // All-day event uses `date` not `dateTime`.
        assert_eq!(
            items[1].start.as_ref().unwrap().date.as_deref(),
            Some("2026-06-12")
        );
        assert_eq!(resp.next_page_token.as_deref(), Some("page2"));
    }

    #[tokio::test]
    async fn list_events_second_page_consumed_via_page_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendar/v3/calendars/primary/events"))
            .and(query_param("pageToken", "page2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{"id": "ev3", "summary": "Followup"}],
                "nextSyncToken": "sync-final"
            })))
            .mount(&server)
            .await;

        let api = CalendarApiClient::with_base_url("test-token", &server.uri());
        let resp = api
            .list_events("primary", None, None, None, Some("page2"))
            .await
            .unwrap();
        let items = resp.items.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id.as_deref(), Some("ev3"));
        assert_eq!(resp.next_sync_token.as_deref(), Some("sync-final"));
        assert!(resp.next_page_token.is_none());
    }

    #[tokio::test]
    async fn list_calendars_parses_two_entries() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendar/v3/users/me/calendarList"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {"id": "primary", "summary": "Me", "primary": true},
                    {"id": "team@example.com", "summary": "Team"}
                ]
            })))
            .mount(&server)
            .await;

        let api = CalendarApiClient::with_base_url("test-token", &server.uri());
        let resp = api.list_calendars().await.unwrap();
        let items = resp.items.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "primary");
        assert_eq!(items[0].primary, Some(true));
        assert_eq!(items[1].id, "team@example.com");
    }

    #[tokio::test]
    async fn get_event_parses_conference_data() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendar/v3/calendars/primary/events/ev1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "ev1",
                "summary": "Sync",
                "conferenceData": {
                    "entryPoints": [
                        {"entryPointType": "video", "uri": "https://meet.example/ev1"}
                    ]
                }
            })))
            .mount(&server)
            .await;

        let api = CalendarApiClient::with_base_url("test-token", &server.uri());
        let event = api.get_event("primary", "ev1").await.unwrap();
        assert_eq!(event.id.as_deref(), Some("ev1"));
        let ep = event.conference_data.unwrap().entry_points.unwrap();
        assert_eq!(ep[0].uri.as_deref(), Some("https://meet.example/ev1"));
    }

    // -- Error paths --

    /// `list_events` calls `.error_for_status()`, so a 401 preserves the status.
    #[tokio::test]
    async fn list_events_401_preserves_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendar/v3/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let api = CalendarApiClient::with_base_url("test-token", &server.uri());
        let err = api
            .list_events("primary", None, None, None, None)
            .await
            .expect_err("expected error");
        match err {
            CalendarError::Http(e) => {
                assert_eq!(e.status(), Some(reqwest::StatusCode::UNAUTHORIZED))
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }

    /// `get_event` 429 is preserved via `.error_for_status()`.
    #[tokio::test]
    async fn get_event_429_preserves_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendar/v3/calendars/primary/events/ev1"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
            .mount(&server)
            .await;

        let api = CalendarApiClient::with_base_url("test-token", &server.uri());
        let err = api
            .get_event("primary", "ev1")
            .await
            .expect_err("expected error");
        match err {
            CalendarError::Http(e) => {
                assert_eq!(e.status(), Some(reqwest::StatusCode::TOO_MANY_REQUESTS))
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }

    /// `search_events` 5xx is preserved via `.error_for_status()`.
    #[tokio::test]
    async fn search_events_500_preserves_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendar/v3/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
            .mount(&server)
            .await;

        let api = CalendarApiClient::with_base_url("test-token", &server.uri());
        let err = api
            .search_events("primary", "lunch", None, None)
            .await
            .expect_err("expected error");
        match err {
            CalendarError::Http(e) => {
                assert_eq!(e.status(), Some(reqwest::StatusCode::INTERNAL_SERVER_ERROR))
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }

    /// `list_calendars` decodes directly (no `error_for_status`); a non-JSON 500
    /// body surfaces as an Http decode error rather than a panic.
    #[tokio::test]
    async fn list_calendars_5xx_surfaces_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendar/v3/users/me/calendarList"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let api = CalendarApiClient::with_base_url("test-token", &server.uri());
        let err = api.list_calendars().await.expect_err("expected error");
        assert!(matches!(err, CalendarError::Http(_)), "got {err:?}");
    }

    /// Malformed JSON: a CalendarListEntry missing required `id` -> clean Err.
    #[tokio::test]
    async fn list_calendars_malformed_json_is_clean_err() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendar/v3/users/me/calendarList"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{"summary": "no id here"}]
            })))
            .mount(&server)
            .await;

        let api = CalendarApiClient::with_base_url("test-token", &server.uri());
        let err = api
            .list_calendars()
            .await
            .expect_err("expected decode error");
        assert!(matches!(err, CalendarError::Http(_)), "got {err:?}");
    }
}
