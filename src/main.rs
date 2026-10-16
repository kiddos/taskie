slint::include_modules!();

use chrono::{Datelike, NaiveDate, Utc, Local};
use ksni::blocking::TrayMethods;
use ksni::menu::*;
use ksni::{MenuItem, Tray};
use rusqlite::{params, Connection};
use slint::{ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::rc::Rc;

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

fn get_db_path() -> PathBuf {
    let home = dirs::home_dir().expect("Could not find home directory");
    home.join(".calendar.db")
}

fn init_db() -> Connection {
    let db_path = get_db_path();
    let conn = Connection::open(&db_path).expect("Failed to open SQLite database");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            start_date TEXT,
            end_date TEXT,
            start TEXT,
            end TEXT,
            description TEXT,
            color TEXT,
            notified TEXT
        )",
        [],
    )
    .expect("Failed to create table");

    let _ = conn.execute("ALTER TABLE events ADD COLUMN notified TEXT", []);
    let _ = conn.execute("ALTER TABLE events ADD COLUMN start_date TEXT", []);
    let _ = conn.execute("ALTER TABLE events ADD COLUMN end_date TEXT", []);
    let _ = conn.execute("ALTER TABLE events ADD COLUMN start TEXT", []);
    let _ = conn.execute("ALTER TABLE events ADD COLUMN end TEXT", []);

    conn
}

pub fn check_events(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let current_time = Utc::now();

    let mut stmt = conn.prepare(
        "SELECT id, COALESCE(start_date, start) as start_val, COALESCE(end_date, end) as end_val, title, description, color, notified
         FROM events
         WHERE notified IS NULL",
    )?;

    let event_iter = stmt.query_map([], |row| {
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
            let is_before_current = if let Ok(start_dt) = chrono::DateTime::parse_from_rfc3339(start_str) {
                start_dt.with_timezone(&Utc) <= current_time
            } else if let Ok(start_dt) = chrono::NaiveDateTime::parse_from_str(start_str, "%Y-%m-%dT%H:%M:%S%.fZ") {
                start_dt.and_utc() <= current_time
            } else if let Ok(start_dt) = chrono::NaiveDateTime::parse_from_str(start_str, "%Y-%m-%d %H:%M:%S") {
                start_dt.and_utc() <= current_time
            } else if let Ok(start_dt) = chrono::NaiveDateTime::parse_from_str(start_str, "%Y-%m-%d %H:%M") {
                start_dt.and_utc() <= current_time
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

        if let Err(e) = notify_rust::Notification::new()
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

struct TaskieTray {
    ui_handle: slint::Weak<MainWindow>,
}

impl Tray for TaskieTray {
    fn id(&self) -> String {
        "taskie".into()
    }

    fn title(&self) -> String {
        "Taskie".into()
    }

    fn icon_name(&self) -> String {
        "calendar".into()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "show".into(),
                activate: Box::new(|tray: &mut Self| {
                    let ui_handle = tray.ui_handle.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_handle.upgrade() {
                            let _ = ui.show();
                        }
                    });
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "hide".into(),
                activate: Box::new(|tray: &mut Self| {
                    let ui_handle = tray.ui_handle.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_handle.upgrade() {
                            let _ = ui.hide();
                        }
                    });
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|_| {
                    std::process::exit(0);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn generate_calendar_days(year: i32, month: u32) -> Vec<DayCell> {
    let first_of_month = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let weekday_of_first = first_of_month.weekday().num_days_from_sunday() as i32;

    let (prev_year, prev_month) = if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    };
    let days_in_prev_month = NaiveDate::from_ymd_opt(
        if prev_month == 12 {
            prev_year + 1
        } else {
            prev_year
        },
        if prev_month == 12 { 1 } else { prev_month + 1 },
        1,
    )
    .unwrap()
    .pred_opt()
    .unwrap()
    .day() as i32;

    let days_in_current_month = NaiveDate::from_ymd_opt(
        if month == 12 { year + 1 } else { year },
        if month == 12 { 1 } else { month + 1 },
        1,
    )
    .unwrap()
    .pred_opt()
    .unwrap()
    .day() as i32;

    let today = Local::now().date_naive();
    let mut cells = Vec::new();

    // Previous month trailing days
    let start_day_num = days_in_prev_month - weekday_of_first + 1;
    for d in start_day_num..=days_in_prev_month {
        let date = NaiveDate::from_ymd_opt(prev_year, prev_month, d as u32).unwrap();
        cells.push(DayCell {
            day_number: SharedString::from(format!("{:02}", d)),
            is_current_month: false,
            is_today: date == today,
            date_str: date.to_string().into(),
        });
    }

    // Current month days
    for d in 1..=days_in_current_month {
        let date = NaiveDate::from_ymd_opt(year, month, d as u32).unwrap();
        cells.push(DayCell {
            day_number: SharedString::from(format!("{:02}", d)),
            is_current_month: true,
            is_today: date == today,
            date_str: date.to_string().into(),
        });
    }

    // Next month leading days to complete 42 cells (6 rows * 7 cols)
    let total_filled = cells.len();
    let remaining = 42 - total_filled;
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    for d in 1..=remaining as u32 {
        let date = NaiveDate::from_ymd_opt(next_year, next_month, d).unwrap();
        cells.push(DayCell {
            day_number: SharedString::from(format!("{:02}", d)),
            is_current_month: false,
            is_today: date == today,
            date_str: date.to_string().into(),
        });
    }

    cells
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = init_db();
    let db_rc = Rc::new(std::cell::RefCell::new(db));

    let ui = MainWindow::new()?;

    // Allow window to be hidden when closed, remaining in the background
    ui.window().on_close_requested(move || {
        slint::CloseRequestResponse::HideWindow
    });

    // Spawn tray icon service
    let _tray_handle = TaskieTray {
        ui_handle: ui.as_weak(),
    }
    .spawn();

    // Spawn background notifier thread
    std::thread::spawn(|| {
        loop {
            let db_path = get_db_path();
            if let Ok(conn) = Connection::open(&db_path) {
                if let Err(e) = check_events(&conn) {
                    eprintln!("Error checking events in background thread: {}", e);
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    });

    let now = Utc::now().naive_utc().date();
    let mut current_year = now.year();
    let mut current_month = now.month();

    let update_calendar = {
        let ui_handle = ui.as_weak();
        move |year: i32, month: u32| {
            if let Some(ui) = ui_handle.upgrade() {
                let month_name = NaiveDate::from_ymd_opt(year, month, 1)
                    .unwrap()
                    .format("%Y %B")
                    .to_string();
                ui.set_current_month_year(month_name.into());

                let days = generate_calendar_days(year, month);
                let model = Rc::new(VecModel::from(days));
                ui.set_days(ModelRc::from(model));
            }
        }
    };

    update_calendar(current_year, current_month);

    // Navigation callbacks
    {
        let update_calendar = update_calendar.clone();
        ui.on_next_month(move || {
            if current_month == 12 {
                current_month = 1;
                current_year += 1;
            } else {
                current_month += 1;
            }
            update_calendar(current_year, current_month);
        });
    }

    {
        let update_calendar = update_calendar.clone();
        ui.on_prev_month(move || {
            if current_month == 1 {
                current_month = 12;
                current_year -= 1;
            } else {
                current_month -= 1;
            }
            update_calendar(current_year, current_month);
        });
    }

    {
        let update_calendar = update_calendar.clone();
        ui.on_today_month(move || {
            let now = Local::now().date_naive();
            let year = now.year();
            let month = now.month();
            update_calendar(year, month);
        });
    }

    // Date click -> Show popup
    {
        let ui_handle = ui.as_weak();
        ui.on_date_clicked(move |date_str| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_selected_date(date_str);
                ui.set_show_popup(true);
            }
        });
    }

    // Close popup
    {
        let ui_handle = ui.as_weak();
        ui.on_close_popup(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_popup(false);
            }
        });
    }

    // Save event to SQLite database
    {
        let ui_handle = ui.as_weak();
        let db_rc = db_rc.clone();
        ui.on_save_event(move |title, start, end, desc| {
            if let Some(ui) = ui_handle.upgrade() {
                let db = db_rc.borrow();
                db.execute(
                    "INSERT INTO events (title, start_date, end_date, description, color) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![title.as_str(), start.as_str(), end.as_str(), desc.as_str(), "#0078d4"],
                ).expect("Failed to insert event into SQLite");

                ui.set_show_popup(false);
                ui.set_event_title("".into());
                ui.set_event_description("".into());
            }
        });
    }

    ui.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                start TEXT,
                end TEXT,
                start_date TEXT,
                end_date TEXT,
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

    #[test]
    fn test_check_events_with_start_date_format() {
        let conn = setup_test_db();
        let past_date = (Utc::now() - Duration::minutes(10)).format("%Y-%m-%d %H:%M").to_string();
        let end_date = Utc::now().format("%Y-%m-%d %H:%M").to_string();

        conn.execute(
            "INSERT INTO events (start_date, end_date, title, description, color) VALUES (?, ?, ?, ?, ?)",
            params![past_date, end_date, "Date Format Meeting", "Desc", "blue"],
        )
        .unwrap();

        check_events(&conn).unwrap();

        let notified: Option<String> = conn
            .query_row("SELECT notified FROM events WHERE id = 1", [], |row| row.get(0))
            .unwrap();

        assert!(notified.is_some());
    }
}
