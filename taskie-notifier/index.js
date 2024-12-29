const Database = require('better-sqlite3');
const notifier = require('node-notifier');
const moment = require('moment');
const os = require('os');
const path = require('path');

// Initialize the SQLite database connection
const dbPath = path.join(os.homedir(), '.calendar.db');
const db = new Database(dbPath, { verbose: console.log });

// Function to check for events and send notifications
function checkEvents() {
  const currentTime = moment();
  const thirtyMinutesAgo = currentTime.clone().subtract(30, 'minutes').toISOString();

  // Query the database for events that should be notified
  const events = db.prepare(`
    SELECT * FROM events
    WHERE start <= ? AND notified IS NULL
  `).all(thirtyMinutesAgo);

  // Check each event and send a notification
  events.forEach(event => {
    if (event.start && moment(event.start).isBefore(currentTime)) {
      notifier.notify({
        title: event.title,
        message: event.description || event.title,
        sound: true,
      });

      // Mark the event as notified
      db.prepare(`
        UPDATE events
        SET notified = ?
        WHERE id = ?
      `).run(currentTime.toISOString(), event.id);
    }
  });
}

// Schedule the task to run every minute
checkEvents();
setInterval(checkEvents, 60000)
