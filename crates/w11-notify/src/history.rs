//! Notification history shared with the notification-center flyout through a JSON file.
//!
//! The daemon keeps one in-memory copy and only re-reads the file when something else changed
//! it (the flyout's "Clear all"), detected by mtime. Previously every single notification
//! parsed and re-allocated the whole file.

use std::time::SystemTime;
use w11_common::config::{self, StoredNotification};

const MAX_HISTORY: usize = 60;

#[derive(Default)]
pub struct History {
    items: Vec<StoredNotification>,
    stamp: Option<SystemTime>,
    loaded: bool,
}

fn mtime() -> Option<SystemTime> {
    std::fs::metadata(config::notifications_path())
        .and_then(|m| m.modified())
        .ok()
}

impl History {
    /// Lazy load, plus reload when the file was changed behind our back.
    fn sync(&mut self) {
        let now = mtime();
        if !self.loaded || now != self.stamp {
            self.items = config::load_notifications();
            self.stamp = now;
            self.loaded = true;
        }
    }

    pub fn push(&mut self, n: StoredNotification) {
        self.sync();
        self.items.retain(|h| h.id != n.id); // a replaced notification moves to the end
        self.items.push(n);
        if self.items.len() > MAX_HISTORY {
            let excess = self.items.len() - MAX_HISTORY;
            self.items.drain(..excess);
        }
        config::save_notifications(&self.items);
        self.stamp = mtime();
    }
}
