use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpListener;
use url::Url;

use crate::{get_google_auth, save_google_tokens};

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_in: i64,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleEventDateTime {
    pub date: Option<String>,
    pub date_time: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleCalendarItem {
    pub id: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub start: Option<GoogleEventDateTime>,
    pub end: Option<GoogleEventDateTime>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleCalendarListResponse {
    pub items: Option<Vec<GoogleCalendarItem>>,
}

#[derive(Debug, Serialize)]
pub struct GoogleEventCreateTime {
    #[serde(rename = "dateTime")]
    pub date_time: String,
}

#[derive(Debug, Serialize)]
pub struct GoogleEventCreatePayload {
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub start: GoogleEventCreateTime,
    pub end: GoogleEventCreateTime,
}

#[derive(Debug, Deserialize)]
pub struct GoogleEventCreateResponse {
    pub id: String,
}

const REDIRECT_URI: &str = "http://127.0.0.1:8080";

pub fn get_valid_access_token(conn: &Connection) -> Result<String, String> {
    let auth = get_google_auth(conn).ok_or_else(|| "Google Calendar is not connected".to_string())?;

    let access_token = auth.access_token.clone().unwrap_or_default();
    let expires_at = auth.expires_at.unwrap_or(0);
    let now = Utc::now().timestamp();

    // If token is still valid (with 60 second buffer), return it
    if !access_token.is_empty() && expires_at > now + 60 {
        return Ok(access_token);
    }

    // Refresh token if available
    let refresh_token = auth
        .refresh_token
        .ok_or_else(|| "Access token expired and no refresh token available. Please reconnect.".to_string())?;

    let client = Client::new();
    let res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", auth.client_id.as_str()),
            ("client_secret", auth.client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .map_err(|e| format!("Failed to send token refresh request: {}", e))?;

    if !res.status().is_success() {
        let err_body = res.text().unwrap_or_default();
        return Err(format!("Token refresh failed: {}", err_body));
    }

    let token_resp: TokenResponse = res
        .json()
        .map_err(|e| format!("Failed to parse refresh token response: {}", e))?;

    save_google_tokens(
        conn,
        &token_resp.access_token,
        token_resp.refresh_token.as_deref(),
        token_resp.expires_in,
    );

    Ok(token_resp.access_token)
}

pub fn start_oauth_flow(client_id: &str, client_secret: &str) -> Result<(String, String, i64), String> {
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=select_account%20consent",
        urlencoding::encode(client_id),
        urlencoding::encode(REDIRECT_URI),
        urlencoding::encode("https://www.googleapis.com/auth/calendar")
    );

    if let Err(e) = webbrowser::open(&auth_url) {
        eprintln!("Failed to open browser automatically: {}", e);
    }

    let listener = TcpListener::bind("127.0.0.1:8080")
        .map_err(|e| format!("Failed to bind local OAuth server on 127.0.0.1:8080: {}", e))?;

    let code = match listener.accept() {
        Ok((mut stream, _)) => {
            let mut buffer = [0; 2048];
            let n = stream.read(&mut buffer).unwrap_or(0);
            let req = String::from_utf8_lossy(&buffer[..n]);

            let code = if let Some(first_line) = req.lines().next() {
                if let Some(path_start) = first_line.find("GET ") {
                    let rest = &first_line[path_start + 4..];
                    if let Some(path_end) = rest.find(" HTTP/") {
                        let path = &rest[..path_end];
                        let dummy_url = format!("http://localhost{}", path);
                        if let Ok(parsed) = Url::parse(&dummy_url) {
                            parsed
                                .query_pairs()
                                .find(|(k, _)| k == "code")
                                .map(|(_, v)| v.to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let response_body = if code.is_some() {
                "<html><body style='font-family:sans-serif;text-align:center;padding-top:50px;'><h2>Taskie Google Calendar Authentication Successful!</h2><p>You can close this window now.</p></body></html>"
            } else {
                "<html><body style='font-family:sans-serif;text-align:center;padding-top:50px;'><h2>Authentication Failed or Cancelled.</h2></body></html>"
            };

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();

            code.ok_or_else(|| "Failed to extract authorization code from OAuth callback".to_string())?
        }
        Err(e) => return Err(format!("Failed to receive OAuth callback: {}", e)),
    };

    let client = Client::new();
    let res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", &code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", REDIRECT_URI),
        ])
        .send()
        .map_err(|e| format!("Failed to exchange authorization code: {}", e))?;

    if !res.status().is_success() {
        let err = res.text().unwrap_or_default();
        return Err(format!("Token exchange failed: {}", err));
    }

    let token_resp: TokenResponse = res
        .json()
        .map_err(|e| format!("Failed to parse token response: {}", e))?;

    let refresh = token_resp
        .refresh_token
        .unwrap_or_default();

    Ok((token_resp.access_token, refresh, token_resp.expires_in))
}

pub fn sync_from_google_calendar(conn: &Connection) -> Result<usize, String> {
    let token = get_valid_access_token(conn)?;
    let client = Client::new();

    let res = client
        .get("https://www.googleapis.com/calendar/v3/calendars/primary/events")
        .bearer_auth(&token)
        .send()
        .map_err(|e| format!("Failed to fetch Google Calendar events: {}", e))?;

    if !res.status().is_success() {
        let err = res.text().unwrap_or_default();
        return Err(format!("Google Calendar API error: {}", err));
    }

    let calendar_resp: GoogleCalendarListResponse = res
        .json()
        .map_err(|e| format!("Failed to parse Google Calendar items: {}", e))?;

    let items = calendar_resp.items.unwrap_or_default();
    let mut synced_count = 0;

    for item in items {
        if item.status.as_deref() == Some("cancelled") {
            // Delete local event if cancelled on Google Calendar
            let _ = conn.execute("DELETE FROM events WHERE google_event_id = ?1", params![item.id]);
            continue;
        }

        let summary = match item.summary {
            Some(s) if !s.trim().is_empty() => s,
            _ => "(No title)".to_string(),
        };

        let description = item.description.unwrap_or_default();

        let start_str = item
            .start
            .as_ref()
            .and_then(|s| s.date_time.clone().or_else(|| s.date.clone()))
            .unwrap_or_default();

        let end_str = item
            .end
            .as_ref()
            .and_then(|e| e.date_time.clone().or_else(|| e.date.clone()))
            .unwrap_or_default();

        if start_str.is_empty() {
            continue;
        }

        let formatted_start = format_google_time_to_db(&start_str);
        let formatted_end = if !end_str.is_empty() {
            format_google_time_to_db(&end_str)
        } else {
            formatted_start.clone()
        };

        // Check if event already exists locally by google_event_id
        let mut stmt = conn
            .prepare("SELECT id FROM events WHERE google_event_id = ?1")
            .map_err(|e| e.to_string())?;

        let exists: Option<i64> = stmt
            .query_row(params![item.id], |row| row.get(0))
            .ok();

        if let Some(local_id) = exists {
            let _ = conn.execute(
                "UPDATE events SET title = ?1, start_date = ?2, end_date = ?3, start = ?2, end = ?3, description = ?4 WHERE id = ?5",
                params![summary, formatted_start, formatted_end, description, local_id],
            );
        } else {
            let _ = conn.execute(
                "INSERT INTO events (title, start_date, end_date, start, end, description, color, google_event_id)
                 VALUES (?1, ?2, ?3, ?2, ?3, ?4, ?5, ?6)",
                params![summary, formatted_start, formatted_end, description, "#3b82f6", item.id],
            );
        }
        synced_count += 1;
    }

    Ok(synced_count)
}

pub fn push_event_to_google_calendar(
    conn: &Connection,
    title: &str,
    start_str: &str,
    end_str: &str,
    description: &str,
) -> Result<Option<String>, String> {
    if get_google_auth(conn).is_none() {
        return Ok(None);
    }

    let token = get_valid_access_token(conn)?;

    let rfc_start = format_db_time_to_rfc3339(start_str);
    let rfc_end = format_db_time_to_rfc3339(end_str);

    let payload = GoogleEventCreatePayload {
        summary: title.to_string(),
        description: if description.is_empty() { None } else { Some(description.to_string()) },
        start: GoogleEventCreateTime { date_time: rfc_start },
        end: GoogleEventCreateTime { date_time: rfc_end },
    };

    let client = Client::new();
    let res = client
        .post("https://www.googleapis.com/calendar/v3/calendars/primary/events")
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .map_err(|e| format!("Failed to push event to Google Calendar: {}", e))?;

    if !res.status().is_success() {
        let err = res.text().unwrap_or_default();
        return Err(format!("Google Calendar API error creating event: {}", err));
    }

    let created: GoogleEventCreateResponse = res
        .json()
        .map_err(|e| format!("Failed to parse Google response: {}", e))?;

    Ok(Some(created.id))
}

fn format_google_time_to_db(gtime: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(gtime) {
        return dt.format("%Y-%m-%d %H:%M").to_string();
    }
    gtime.to_string()
}

fn format_db_time_to_rfc3339(db_time: &str) -> String {
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(db_time, "%Y-%m-%d %H:%M") {
        return dt.and_utc().to_rfc3339();
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(db_time) {
        return dt.to_rfc3339();
    }
    format!("{}:00Z", db_time.replace(' ', "T"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_db_time_to_rfc3339() {
        let db_t = "2026-03-29 09:00";
        let rfc = format_db_time_to_rfc3339(db_t);
        assert_eq!(rfc, "2026-03-29T09:00:00+00:00");
    }

    #[test]
    fn test_format_google_time_to_db() {
        let gtime = "2026-03-29T09:00:00Z";
        let db_t = format_google_time_to_db(gtime);
        assert_eq!(db_t, "2026-03-29 09:00");
    }
}
