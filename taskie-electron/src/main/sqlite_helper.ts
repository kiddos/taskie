import path from 'path';
import Database from 'better-sqlite3';
import os from 'os';
import { Event } from '../types/event';

// Database initialization
const dbPath = path.join(os.homedir(), '.calendar.db');
let db: any = null;
try {
  db = new Database(dbPath, { verbose: console.log });
  db.prepare(
    `CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    start TEXT NOT NULL,
    end TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    color TEXT,
    notified TEXT
  )`
  ).run();
} catch (err) {
  console.error('fail to initialize sqlite db', err);
}

export function createEvent(event: Event): number {
  const stmt = db.prepare(`
    INSERT INTO events (start, end, title, description, color)
    VALUES (?, ?, ?, ?, ?)
  `);
  const result = stmt.run(
    event.start,
    event.end,
    event.title,
    event.description,
    event.color
  );
  return result.lastInsertRowid as number; // Return the ID of the inserted event
}

export function getEventById(id: number): Event {
  const stmt = db.prepare('SELECT * FROM events WHERE id = ?');
  const event = stmt.get(id);
  return event;
}

export function getAllEvents(): Event[] {
  const stmt = db.prepare('SELECT * FROM events');
  return stmt.all() as Event[];
}

export function updateEvent(event: Event): boolean {
  const { id, start, end, title, description, color } = event;
  const stmt = db.prepare(`
    UPDATE events
    SET start= ?, end = ?, title = ?, description = ?, color = ?, notified = NULL
    WHERE id = ?
  `);
  const result = stmt.run(start, end, title, description, color, id);
  return result.changes > 0;
}

export function deleteEvent(id: number): boolean {
  const stmt = db.prepare('DELETE FROM events WHERE id = ?');
  const result = stmt.run(id);
  return result.changes > 0;
}
