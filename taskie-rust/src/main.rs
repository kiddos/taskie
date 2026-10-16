slint::include_modules!();

use chrono::{Datelike, NaiveDate, Utc};
use rusqlite::{Connection, params};
use slint::{ModelRc, SharedString, VecModel};
use std::rc::Rc;

fn init_db() -> Connection {
    let conn = Connection::open("taskie.db").expect("Failed to open SQLite database");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            start_date TEXT NOT NULL,
            end_date TEXT NOT NULL,
            description TEXT,
            color TEXT
        )",
        [],
    )
    .expect("Failed to create table");
    conn
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

    let today = Utc::now().naive_utc().date();
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
            let now = Utc::now().naive_utc().date();
            let year  = now.year();
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
