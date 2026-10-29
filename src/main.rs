#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

pub mod google_calendar;

use chrono::{Datelike, Local, NaiveDate, Utc};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use rusqlite::{params, Connection};
use slint::{ModelRc, SharedString, VecModel};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct Event {
    pub id: i64,
    pub start: Option<String>,
    pub end: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub notified: Option<String>,
    pub google_event_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GoogleAuthInfo {
    pub client_id: String,
    pub client_secret: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
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
    let _ = conn.execute("ALTER TABLE events ADD COLUMN google_event_id TEXT", []);

    conn.execute(
        "CREATE TABLE IF NOT EXISTS google_auth (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            client_id TEXT NOT NULL,
            client_secret TEXT NOT NULL,
            access_token TEXT,
            refresh_token TEXT,
            expires_at INTEGER
        )",
        [],
    )
    .expect("Failed to create google_auth table");

    conn
}

pub fn get_google_auth(conn: &Connection) -> Option<GoogleAuthInfo> {
    let mut stmt = conn
        .prepare("SELECT client_id, client_secret, access_token, refresh_token, expires_at FROM google_auth WHERE id = 1")
        .ok()?;
    stmt.query_row([], |row| {
        Ok(GoogleAuthInfo {
            client_id: row.get(0)?,
            client_secret: row.get(1)?,
            access_token: row.get(2)?,
            refresh_token: row.get(3)?,
            expires_at: row.get(4)?,
        })
    })
    .ok()
}

pub fn save_google_credentials(conn: &Connection, client_id: &str, client_secret: &str) {
    let _ = conn.execute(
        "INSERT INTO google_auth (id, client_id, client_secret) VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET client_id = ?1, client_secret = ?2",
        params![client_id, client_secret],
    );
}

pub fn save_google_tokens(conn: &Connection, access_token: &str, refresh_token: Option<&str>, expires_in_secs: i64) {
    let now = Utc::now().timestamp();
    let expires_at = now + expires_in_secs;
    if let Some(rf) = refresh_token {
        let _ = conn.execute(
            "UPDATE google_auth SET access_token = ?1, refresh_token = ?2, expires_at = ?3 WHERE id = 1",
            params![access_token, rf, expires_at],
        );
    } else {
        let _ = conn.execute(
            "UPDATE google_auth SET access_token = ?1, expires_at = ?2 WHERE id = 1",
            params![access_token, expires_at],
        );
    }
}

pub fn clear_google_auth(conn: &Connection) {
    let _ = conn.execute("DELETE FROM google_auth WHERE id = 1", []);
}

pub fn check_events(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let current_time = Utc::now();

    let mut stmt = conn.prepare(
        "SELECT id, COALESCE(start_date, start) as start_val, COALESCE(end_date, end) as end_val, title, description, color, notified, google_event_id
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
            google_event_id: row.get(7)?,
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

fn fetch_all_events(conn: &Connection) -> Vec<Event> {
    let mut stmt = match conn.prepare(
        "SELECT id, COALESCE(start_date, start) as start_val, COALESCE(end_date, end) as end_val, title, description, color, notified, google_event_id FROM events"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let event_iter = stmt.query_map([], |row| {
        Ok(Event {
            id: row.get(0)?,
            start: row.get(1)?,
            end: row.get(2)?,
            title: row.get(3)?,
            description: row.get(4)?,
            color: row.get(5)?,
            notified: row.get(6)?,
            google_event_id: row.get(7)?,
        })
    });

    match event_iter {
        Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

fn parse_date_from_str(s: &str) -> Option<NaiveDate> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.date_naive());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ") {
        return Some(dt.date());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(dt.date());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M") {
        return Some(dt.date());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    if s.len() >= 10 {
        if let Ok(d) = NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d") {
            return Some(d);
        }
    }
    None
}

fn format_time_from_str(s: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return dt.format("%H:%M").to_string();
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ") {
        return dt.format("%H:%M").to_string();
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return dt.format("%H:%M").to_string();
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M") {
        return dt.format("%H:%M").to_string();
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() > 1 {
        return parts[1].to_string();
    }
    "".to_string()
}

fn get_events_for_date(db_events: &[Event], target_date: NaiveDate) -> Vec<CalendarEvent> {
    db_events
        .iter()
        .filter(|e| {
            if let Some(ref start_str) = e.start {
                if let Some(start_date) = parse_date_from_str(start_str) {
                    let end_date = e
                        .end
                        .as_deref()
                        .and_then(parse_date_from_str)
                        .unwrap_or(start_date);
                    let effective_end = if end_date < start_date { start_date } else { end_date };
                    target_date >= start_date && target_date <= effective_end
                } else {
                    false
                }
            } else {
                false
            }
        })
        .map(|e| {
            let start_time = e.start.as_deref().map(format_time_from_str).unwrap_or_default();
            let end_time = e.end.as_deref().map(format_time_from_str).unwrap_or_default();
            CalendarEvent {
                id: e.id as i32,
                title: SharedString::from(&e.title),
                description: SharedString::from(e.description.as_deref().unwrap_or("")),
                start_time: SharedString::from(start_time),
                end_time: SharedString::from(end_time),
                color: SharedString::from(e.color.as_deref().unwrap_or("#3b82f6")),
            }
        })
        .collect()
}

fn create_tray_icon() -> Result<tray_icon::Icon, Box<dyn std::error::Error>> {
    const WIDTH: u32 = 32;
    const HEIGHT: u32 = 32;
    const ICON_RGBA: &[u8] = include_bytes!("../assets/icon_32.rgba");
    Ok(tray_icon::Icon::from_rgba(ICON_RGBA.to_vec(), WIDTH, HEIGHT)?)
}

fn setup_tray(ui: &MainWindow) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let tray_menu = Menu::new();
    let show_item = MenuItem::new("Show", true, None);
    let hide_item = MenuItem::new("Hide", true, None);
    let quit_item = MenuItem::new("Quit", true, None);

    tray_menu.append(&show_item)?;
    tray_menu.append(&hide_item)?;
    tray_menu.append(&quit_item)?;

    let show_id = show_item.id().clone();
    let hide_id = hide_item.id().clone();
    let quit_id = quit_item.id().clone();

    let ui_weak = ui.as_weak();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == show_id {
            let ui_handle = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_handle.upgrade() {
                    let _ = ui.show();
                }
            });
        } else if event.id == hide_id {
            let ui_handle = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_handle.upgrade() {
                    let _ = ui.hide();
                }
            });
        } else if event.id == quit_id {
            let _ = slint::invoke_from_event_loop(|| {
                let _ = slint::quit_event_loop();
            });
        }
    }));

    let ui_weak_click = ui.as_weak();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let ui_handle = ui_weak_click.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_handle.upgrade() {
                    let _ = ui.show();
                }
            });
        }
    }));

    let mut builder = TrayIconBuilder::new()
        .with_tooltip("Taskie")
        .with_title("Taskie")
        .with_menu(Box::new(tray_menu));

    if let Ok(icon) = create_tray_icon() {
        builder = builder.with_icon(icon);
    }

    let tray = builder.build()?;
    Ok(tray)
}

fn generate_calendar_days(year: i32, month: u32, db_events: &[Event]) -> Vec<DayCell> {
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

    let build_cell = |date: NaiveDate, is_current_month: bool| -> DayCell {
        let events = get_events_for_date(db_events, date);
        let event_count = events.len() as i32;
        let has_events = event_count > 0;
        let events_model = Rc::new(VecModel::from(events));

        DayCell {
            day_number: SharedString::from(format!("{:02}", date.day())),
            is_current_month,
            is_today: date == today,
            date_str: date.to_string().into(),
            has_events,
            event_count,
            events: ModelRc::from(events_model),
        }
    };

    // Previous month trailing days
    let start_day_num = days_in_prev_month - weekday_of_first + 1;
    for d in start_day_num..=days_in_prev_month {
        let date = NaiveDate::from_ymd_opt(prev_year, prev_month, d as u32).unwrap();
        cells.push(build_cell(date, false));
    }

    // Current month days
    for d in 1..=days_in_current_month {
        let date = NaiveDate::from_ymd_opt(year, month, d as u32).unwrap();
        cells.push(build_cell(date, true));
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
        cells.push(build_cell(date, false));
    }

    cells
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = init_db();
    let db_rc = std::sync::Arc::new(std::sync::Mutex::new(db));

    let ui = MainWindow::new()?;

    // Spawn cross-platform tray icon service
    let _tray = match setup_tray(&ui) {
        Ok(tray) => Some(tray),
        Err(e) => {
            eprintln!("Warning: Failed to initialize system tray: {e}");
            None
        }
    };

    // Minimize / hide to tray when user closes the window, keeping background notifier running
    if _tray.is_some() {
        ui.window().on_close_requested(move || {
            slint::CloseRequestResponse::HideWindow
        });
    }

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
    let current_year = Rc::new(Cell::new(now.year()));
    let current_month = Rc::new(Cell::new(now.month()));

    let update_calendar = {
        let ui_handle = ui.as_weak();
        let db_rc = db_rc.clone();
        let current_year = current_year.clone();
        let current_month = current_month.clone();
        move |year: i32, month: u32| {
            current_year.set(year);
            current_month.set(month);
            if let Some(ui) = ui_handle.upgrade() {
                let db = db_rc.lock().unwrap();
                let events = fetch_all_events(&db);

                let month_name = NaiveDate::from_ymd_opt(year, month, 1)
                    .unwrap()
                    .format("%Y %B")
                    .to_string();
                ui.set_current_month_year(month_name.into());

                let days = generate_calendar_days(year, month, &events);
                let model = Rc::new(VecModel::from(days));
                ui.set_days(ModelRc::from(model));
            }
        }
    };

    // Initial check for Google connection status & perform auto sync if connected
    {
        let db = db_rc.lock().unwrap();
        if let Some(auth) = get_google_auth(&db) {
            ui.set_google_client_id(auth.client_id.into());
            ui.set_google_client_secret(auth.client_secret.into());
            if auth.access_token.is_some() {
                ui.set_is_google_connected(true);
                // Perform initial sync in background thread so app startup isn't blocked
                let db_rc_sync = db_rc.clone();
                let ui_handle_sync = ui.as_weak();
                let cur_y = current_year.get();
                let cur_m = current_month.get();
                std::thread::spawn(move || {
                    {
                        let db = db_rc_sync.lock().unwrap();
                        let _ = google_calendar::sync_from_google_calendar(&db);
                    }
                    let db_rc_inner = db_rc_sync.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_handle_sync.upgrade() {
                            let db = db_rc_inner.lock().unwrap();
                            let events = fetch_all_events(&db);
                            let month_name = NaiveDate::from_ymd_opt(cur_y, cur_m, 1)
                                .unwrap()
                                .format("%Y %B")
                                .to_string();
                            ui.set_current_month_year(month_name.into());

                            let days = generate_calendar_days(cur_y, cur_m, &events);
                            let model = Rc::new(VecModel::from(days));
                            ui.set_days(ModelRc::from(model));
                        }
                    });
                });
            }
        }
    }

    // Google Calendar UI Callbacks
    {
        let ui_handle = ui.as_weak();
        ui.on_open_google_popup(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_google_popup(true);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_close_google_popup(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_google_popup(false);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let db_rc = db_rc.clone();
        let current_year = current_year.clone();
        let current_month = current_month.clone();
        ui.on_connect_google(move |client_id, client_secret| {
            if client_id.trim().is_empty() || client_secret.trim().is_empty() {
                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_google_status_message("Client ID and Client Secret are required.".into());
                }
                return;
            }

            let ui_handle = ui_handle.clone();
            let db_rc = db_rc.clone();
            let cur_y = current_year.get();
            let cur_m = current_month.get();

            let c_id = client_id.to_string();
            let c_sec = client_secret.to_string();

            if let Some(ui) = ui_handle.upgrade() {
                ui.set_google_status_message("Opening browser for OAuth authentication...".into());
            }

            std::thread::spawn(move || {
                match google_calendar::start_oauth_flow(&c_id, &c_sec) {
                    Ok((access_token, refresh_token, expires_in)) => {
                        let sync_res = {
                            let db = db_rc.lock().unwrap();
                            save_google_credentials(&db, &c_id, &c_sec);
                            save_google_tokens(
                                &db,
                                &access_token,
                                if refresh_token.is_empty() { None } else { Some(&refresh_token) },
                                expires_in,
                            );

                            // Trigger sync immediately after authentication
                            google_calendar::sync_from_google_calendar(&db)
                        };

                        let msg = match sync_res {
                            Ok(count) => format!("Connected! Synced {} events.", count),
                            Err(e) => format!("Connected, but sync had warning: {}", e),
                        };

                        let db_rc_inner = db_rc.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = ui_handle.upgrade() {
                                ui.set_is_google_connected(true);
                                ui.set_google_status_message(msg.into());
                                let db = db_rc_inner.lock().unwrap();
                                let events = fetch_all_events(&db);
                                let month_name = NaiveDate::from_ymd_opt(cur_y, cur_m, 1)
                                    .unwrap()
                                    .format("%Y %B")
                                    .to_string();
                                ui.set_current_month_year(month_name.into());

                                let days = generate_calendar_days(cur_y, cur_m, &events);
                                let model = Rc::new(VecModel::from(days));
                                ui.set_days(ModelRc::from(model));
                            }
                        });
                    }
                    Err(e) => {
                        let err_msg = format!("Authentication failed: {}", e);
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = ui_handle.upgrade() {
                                ui.set_google_status_message(err_msg.into());
                            }
                        });
                    }
                }
            });
        });
    }

    {
        let ui_handle = ui.as_weak();
        let db_rc = db_rc.clone();
        ui.on_disconnect_google(move || {
            let db = db_rc.lock().unwrap();
            clear_google_auth(&db);
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_is_google_connected(false);
                ui.set_google_status_message("Disconnected from Google Calendar.".into());
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let db_rc = db_rc.clone();
        let current_year = current_year.clone();
        let current_month = current_month.clone();
        ui.on_sync_google(move || {
            let ui_handle = ui_handle.clone();
            let db_rc = db_rc.clone();
            let cur_y = current_year.get();
            let cur_m = current_month.get();

            std::thread::spawn(move || {
                let result = {
                    let db = db_rc.lock().unwrap();
                    google_calendar::sync_from_google_calendar(&db)
                };
                let msg = match result {
                    Ok(count) => format!("Synced {} events from Google Calendar.", count),
                    Err(e) => format!("Sync error: {}", e),
                };

                let db_rc_inner = db_rc.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_handle.upgrade() {
                        ui.set_google_status_message(msg.into());
                        let db = db_rc_inner.lock().unwrap();
                        let events = fetch_all_events(&db);
                        let month_name = NaiveDate::from_ymd_opt(cur_y, cur_m, 1)
                            .unwrap()
                            .format("%Y %B")
                            .to_string();
                        ui.set_current_month_year(month_name.into());

                        let days = generate_calendar_days(cur_y, cur_m, &events);
                        let model = Rc::new(VecModel::from(days));
                        ui.set_days(ModelRc::from(model));
                    }
                });
            });
        });
    }

    update_calendar(current_year.get(), current_month.get());

    // Navigation callbacks
    {
        let update_calendar = update_calendar.clone();
        let current_year = current_year.clone();
        let current_month = current_month.clone();
        ui.on_next_month(move || {
            let mut y = current_year.get();
            let mut m = current_month.get();
            if m == 12 {
                m = 1;
                y += 1;
            } else {
                m += 1;
            }
            update_calendar(y, m);
        });
    }

    {
        let update_calendar = update_calendar.clone();
        let current_year = current_year.clone();
        let current_month = current_month.clone();
        ui.on_prev_month(move || {
            let mut y = current_year.get();
            let mut m = current_month.get();
            if m == 1 {
                m = 12;
                y -= 1;
            } else {
                m -= 1;
            }
            update_calendar(y, m);
        });
    }

    {
        let update_calendar = update_calendar.clone();
        ui.on_today_month(move || {
            let now = Local::now().date_naive();
            update_calendar(now.year(), now.month());
        });
    }

    // Helper closure to update selected_events for popup view
    let refresh_selected_events = {
        let ui_handle = ui.as_weak();
        let db_rc = db_rc.clone();
        move |date_str: &str| {
            if let Some(ui) = ui_handle.upgrade() {
                if let Ok(target_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    let db = db_rc.lock().unwrap();
                    let events = fetch_all_events(&db);
                    let sel_events = get_events_for_date(&events, target_date);
                    ui.set_selected_events(ModelRc::from(Rc::new(VecModel::from(sel_events))));
                }
            }
        }
    };

    // Date click -> Show popup
    {
        let ui_handle = ui.as_weak();
        let refresh_selected_events = refresh_selected_events.clone();
        ui.on_date_clicked(move |date_str| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_selected_date(date_str.clone());
                refresh_selected_events(&date_str);
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
        let update_calendar = update_calendar.clone();
        let refresh_selected_events = refresh_selected_events.clone();
        let current_year = current_year.clone();
        let current_month = current_month.clone();
        ui.on_save_event(move |title, start, end, desc, color| {
            if let Some(ui) = ui_handle.upgrade() {
                let db = db_rc.lock().unwrap();

                // If Google Calendar is connected, attempt to push event to Google Calendar
                let google_event_id = if get_google_auth(&db).is_some() {
                    match google_calendar::push_event_to_google_calendar(&db, title.as_str(), start.as_str(), end.as_str(), desc.as_str()) {
                        Ok(gid) => gid,
                        Err(e) => {
                            eprintln!("Failed to push event to Google Calendar: {}", e);
                            None
                        }
                    }
                } else {
                    None
                };

                db.execute(
                    "INSERT INTO events (title, start_date, end_date, start, end, description, color, google_event_id) VALUES (?1, ?2, ?3, ?2, ?3, ?4, ?5, ?6)",
                    params![title.as_str(), start.as_str(), end.as_str(), desc.as_str(), color.as_str(), google_event_id],
                ).expect("Failed to insert event into SQLite");

                let selected_date = ui.get_selected_date().to_string();

                drop(db); // release lock before updating calendar

                update_calendar(current_year.get(), current_month.get());
                refresh_selected_events(&selected_date);

                ui.set_event_title("".into());
                ui.set_event_description("".into());
            }
        });
    }

    // Delete event from SQLite database
    {
        let ui_handle = ui.as_weak();
        let db_rc = db_rc.clone();
        let update_calendar = update_calendar.clone();
        let refresh_selected_events = refresh_selected_events.clone();
        let current_year = current_year.clone();
        let current_month = current_month.clone();
        ui.on_delete_event(move |event_id| {
            if let Some(ui) = ui_handle.upgrade() {
                let db = db_rc.lock().unwrap();
                db.execute(
                    "DELETE FROM events WHERE id = ?1",
                    params![event_id],
                ).expect("Failed to delete event from SQLite");

                let selected_date = ui.get_selected_date().to_string();

                drop(db); // release lock before updating calendar

                update_calendar(current_year.get(), current_month.get());
                refresh_selected_events(&selected_date);
            }
        });
    }

    ui.show()?;
    slint::run_event_loop_until_quit()?;
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
                notified TEXT,
                google_event_id TEXT
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS google_auth (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                client_id TEXT NOT NULL,
                client_secret TEXT NOT NULL,
                access_token TEXT,
                refresh_token TEXT,
                expires_at INTEGER
            )",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_google_auth_db_operations() {
        let conn = setup_test_db();
        assert!(get_google_auth(&conn).is_none());

        save_google_credentials(&conn, "test_client_id", "test_client_secret");
        let auth = get_google_auth(&conn).unwrap();
        assert_eq!(auth.client_id, "test_client_id");
        assert_eq!(auth.client_secret, "test_client_secret");
        assert!(auth.access_token.is_none());

        save_google_tokens(&conn, "access_123", Some("refresh_456"), 3600);
        let auth = get_google_auth(&conn).unwrap();
        assert_eq!(auth.access_token.as_deref(), Some("access_123"));
        assert_eq!(auth.refresh_token.as_deref(), Some("refresh_456"));

        clear_google_auth(&conn);
        assert!(get_google_auth(&conn).is_none());
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

    #[test]
    fn test_get_events_for_date() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO events (title, start_date, end_date, description, color) VALUES (?, ?, ?, ?, ?)",
            params!["Team Sync", "2026-03-29 09:00", "2026-03-29 10:00", "Weekly sync", "#10b981"],
        )
        .unwrap();

        let events = fetch_all_events(&conn);
        let date = NaiveDate::from_ymd_opt(2026, 3, 29).unwrap();
        let day_events = get_events_for_date(&events, date);

        assert_eq!(day_events.len(), 1);
        assert_eq!(day_events[0].title.as_str(), "Team Sync");
        assert_eq!(day_events[0].color.as_str(), "#10b981");
    }

    #[test]
    fn test_generate_calendar_days_contains_events() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO events (title, start_date, end_date, description, color) VALUES (?, ?, ?, ?, ?)",
            params!["Conference", "2026-03-15 08:00", "2026-03-17 18:00", "Tech Conf", "#8b5cf6"],
        )
        .unwrap();

        let events = fetch_all_events(&conn);
        let days = generate_calendar_days(2026, 3, &events);

        assert_eq!(days.len(), 42);

        // March 15, 16, and 17 should have the event
        let march_15_cell = days.iter().find(|cell| cell.date_str.as_str() == "2026-03-15").unwrap();
        assert!(march_15_cell.has_events);
        assert_eq!(march_15_cell.event_count, 1);

        let march_16_cell = days.iter().find(|cell| cell.date_str.as_str() == "2026-03-16").unwrap();
        assert!(march_16_cell.has_events);

        let march_17_cell = days.iter().find(|cell| cell.date_str.as_str() == "2026-03-17").unwrap();
        assert!(march_17_cell.has_events);

        let march_18_cell = days.iter().find(|cell| cell.date_str.as_str() == "2026-03-18").unwrap();
        assert!(!march_18_cell.has_events);
    }
}
