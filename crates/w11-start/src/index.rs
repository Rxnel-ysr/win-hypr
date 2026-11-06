//! Lazy, droppable app index. Loaded on first use, released when the menu hides.

use gtk::gio;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use w11_common::apps;

struct Entry {
    info: gio::AppInfo,
    name_lc: String,
}

pub struct Index {
    entries: Vec<Entry>,
}

impl Index {
    fn load() -> Self {
        let entries = apps::all()
            .into_iter()
            .map(|info| {
                let name_lc = info.name().to_lowercase();
                Entry { info, name_lc }
            })
            .collect();
        Self { entries }
    }

    pub fn apps(&self) -> impl Iterator<Item = &gio::AppInfo> {
        self.entries.iter().map(|e| &e.info)
    }

    /// `q` must already be lowercase. Ranked: prefix, name contains, description contains.
    pub fn search(&self, q: &str, limit: usize) -> Vec<&gio::AppInfo> {
        let mut hits: Vec<(u8, &gio::AppInfo)> = Vec::new();
        for e in &self.entries {
            let rank = if e.name_lc.starts_with(q) {
                0
            } else if e.name_lc.contains(q) {
                1
            } else if e
                .info
                .description()
                .map_or(false, |d| d.to_lowercase().contains(q))
            {
                2
            } else {
                continue;
            };
            hits.push((rank, &e.info));
        }
        hits.sort_by_key(|h| h.0);
        hits.into_iter().take(limit).map(|h| h.1).collect()
    }
}

#[derive(Default)]
pub struct AppCache(RefCell<Option<Rc<Index>>>);

impl AppCache {
    pub fn get(&self) -> Rc<Index> {
        self.0
            .borrow_mut()
            .get_or_insert_with(|| Rc::new(Index::load()))
            .clone()
    }

    pub fn clear(&self) {
        self.0.borrow_mut().take();
    }
}
