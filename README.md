# Taskie

Taskie is a lightweight desktop calendar and event management application built with **Rust** and **Slint UI**. It offers a clean graphical interface to organize schedule events, desktop notifications, and system tray integration.

## Features

- **Interactive Calendar Grid**: View monthly calendar layouts with automatic date and day cell calculations.
- **Event Management**: Create, view, and delete events with customizable titles, start/end times, descriptions, and color tags.
- **Desktop Notifications**: Background worker checks scheduled events and triggers native desktop notifications when events occur.
- **System Tray Support**: Runs in the background and provides system tray access (Show, Hide, Quit) via cross-platform tray integration.
- **SQLite Persistence**: Stores calendar events locally in `~/.calendar.db`.

## Tech Stack

- **GUI**: [Slint](https://slint.dev/) (v1.8 / v1.18)
- **Database**: [rusqlite](https://crates.io/crates/rusqlite) (SQLite with bundled feature)
- **Notifications**: [notify-rust](https://crates.io/crates/notify-rust)
- **System Tray**: [tray-icon](https://crates.io/crates/tray-icon)
- **Time/Date handling**: [chrono](https://crates.io/crates/chrono)

## Prerequisites

On Linux distributions (e.g. Ubuntu/Debian), ensure required system packages for font configuration and graphics building are installed:

```bash
sudo apt-get install libfontconfig1-dev pkg-config
```

## Building and Running

### Build

To compile Taskie in debug mode:

```bash
cargo build
```

To build an optimized release binary:

```bash
cargo build --release
```

### Windows Installer (Inno Setup)

To build the Windows installer locally on Windows using [Inno Setup](https://jrsoftware.org/isinfo.php):

1. Compile the 64-bit release binary:
   ```powershell
   cargo build --release --target x86_64-pc-windows-msvc
   ```
2. Compile the installer with `iscc`:
   ```powershell
   iscc installer\windows\taskie.iss
   ```
The installer executable will be created at `artifacts/taskie-windows-setup.exe`. In GitHub Actions, the installer is built automatically on new releases or manual workflow dispatch.

### Run

To launch Taskie:

```bash
cargo run
```

### Running Tests

To run the unit test suite:

```bash
cargo test
```

## Database Schema & Storage

Taskie creates and maintains a local SQLite database at `~/.calendar.db`. The `events` table includes:

- `id` (INTEGER PRIMARY KEY)
- `title` (TEXT)
- `start_date` / `start` (TEXT)
- `end_date` / `end` (TEXT)
- `description` (TEXT)
- `color` (TEXT)
- `notified` (TEXT timestamp ISO-8601)

## License

This project is licensed under standard open-source terms.
