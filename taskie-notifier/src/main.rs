use chrono::{DateTime, Duration, Utc};
use notify_rust::Notification;
use rusqlite::{params, Connection, Result};
use std::path::PathBuf;
use std::thread;
use std::time::Duration as StdDuration;

#[derive(Debug)]
pub struct Event {
    pub id: i64,
    pub start: Option<String>,
    pub end: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub notified: Option<String>,
}

pub fn check_events(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let current_time = Utc::now();
    let thirty_minutes_ago = (current_time - Duration::minutes(30)).to_rfc3339();

    let mut stmt = conn.prepare(
        "SELECT id, start, end, title, description, color, notified
         FROM events
         WHERE start <= ? AND notified IS NULL",
    )?;

    let event_iter = stmt.query_map(params![thirty_minutes_ago], |row| {
        Ok(Event {
            id: row.get(0)?,
            start: row.get(1)?,
            end: row.get(2)?,
            title: row.get(3)?,
            description: row.get(4)?,
            color: row.get(5)?,
            notified: row.get(6)?,
        })
    })?;

    let mut events_to_notify = Vec::new();

    for event in event_iter {
        let event = event?;
        if let Some(ref start_str) = event.start {
            // Parse start timestamp using DateTime::parse_from_rfc3339 or fallback ISO formats
            let is_before_current = if let Ok(start_dt) = DateTime::parse_from_rfc3339(start_str) {
                start_dt.with_timezone(&Utc) < current_time
            } else if let Ok(start_dt) = chrono::NaiveDateTime::parse_from_str(start_str, "%Y-%m-%dT%H:%M:%S%.fZ") {
                start_dt.and_utc() < current_time
            } else if let Ok(start_dt) = chrono::NaiveDateTime::parse_from_str(start_str, "%Y-%m-%d %H:%M:%S") {
                start_dt.and_utc() < current_time
            } else {
                false
            };

            if is_before_current {
                events_to_notify.push(event);
            }
        }
    }

    let current_iso = current_time.to_rfc3339();

    for event in events_to_notify {
        let body = event
            .description
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&event.title);

        if let Err(e) = Notification::new()
            .summary(&event.title)
            .body(body)
            .sound_name("default")
            .show()
        {
            eprintln!("Failed to send notification for event {}: {}", event.id, e);
        }

        conn.execute(
            "UPDATE events SET notified = ? WHERE id = ?",
            params![current_iso, event.id],
        )?;
    }

    Ok(())
}

fn get_db_path() -> PathBuf {
    let home = dirs::home_dir().expect("Could not find home directory");
    home.join(".calendar.db")
}

fn main() {
    let db_path = get_db_path();
    println!("Connecting to database at {:?}", db_path);

    loop {
        match Connection::open(&db_path) {
            Ok(conn) => {
                if let Err(e) = check_events(&conn) {
                    eprintln!("Error checking events: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Failed to open database: {}", e);
            }
        }

        thread::sleep(StdDuration::from_secs(60));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                start TEXT NOT NULL,
                end TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                color TEXT,
                notified TEXT
            )",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_check_events_notifies_past_unnotified_events() {
        let conn = setup_test_db();
        let past_time = (Utc::now() - Duration::minutes(35)).to_rfc3339();
        let end_time = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO events (start, end, title, description, color) VALUES (?, ?, ?, ?, ?)",
            params![past_time, end_time, "Test Meeting", "Test Description", "blue"],
        )
        .unwrap();

        check_events(&conn).unwrap();

        let notified: Option<String> = conn
            .query_row("SELECT notified FROM events WHERE id = 1", [], |row| row.get(0))
            .unwrap();

        assert!(notified.is_some());
    }

    #[test]
    fn test_check_events_skips_future_events() {
        let conn = setup_test_db();
        let future_time = (Utc::now() + Duration::hours(1)).to_rfc3339();
        let end_time = (Utc::now() + Duration::hours(2)).to_rfc3339();

        conn.execute(
            "INSERT INTO events (start, end, title, description, color) VALUES (?, ?, ?, ?, ?)",
            params![future_time, end_time, "Future Meeting", "Description", "blue"],
        )
        .unwrap();

        check_events(&conn).unwrap();

        let notified: Option<String> = conn
            .query_row("SELECT notified FROM events WHERE id = 1", [], |row| row.get(0))
            .unwrap();

        assert!(notified.is_none());
    }

    #[test]
    fn test_check_events_skips_already_notified_events() {
        let conn = setup_test_db();
        let past_time = (Utc::now() - Duration::minutes(40)).to_rfc3339();
        let end_time = Utc::now().to_rfc3339();
        let notified_time = (Utc::now() - Duration::minutes(10)).to_rfc3339();

        conn.execute(
            "INSERT INTO events (start, end, title, description, color, notified) VALUES (?, ?, ?, ?, ?, ?)",
            params![past_time, end_time, "Notified Meeting", "Description", "blue", notified_time],
        )
        .unwrap();

        check_events(&conn).unwrap();

        let notified: Option<String> = conn
            .query_row("SELECT notified FROM events WHERE id = 1", [], |row| row.get(0))
            .unwrap();

        assert_eq!(notified, Some(notified_time));
    }

    #[test]
    fn test_check_events_fallback_description_to_title() {
        let conn = setup_test_db();
        let past_time = (Utc::now() - Duration::minutes(35)).to_rfc3339();
        let end_time = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO events (start, end, title, description, color) VALUES (?, ?, ?, NULL, ?)",
            params![past_time, end_time, "Meeting without description", "blue"],
        )
        .unwrap();

        check_events(&conn).unwrap();

        let notified: Option<String> = conn
            .query_row("SELECT notified FROM events WHERE id = 1", [], |row| row.get(0))
            .unwrap();

        assert!(notified.is_some());
    }
}
