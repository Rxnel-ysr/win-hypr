//! Small AppInfo cache. `w11_common::apps` keeps the *entire* installed-app list alive after
//! the first `for_class` call; we keep only the handful of apps actually shown on the bar and
//! drop the big list again right after resolving.

use gtk::gio;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use w11_common::apps;

const MAX_CLASSES: usize = 64;

type Map = RefCell<HashMap<String, Option<gio::AppInfo>>>;

#[derive(Default)]
pub struct Resolver {
    ids: Map,
    classes: Map,
    dirty: Cell<bool>,
}

impl Resolver {
    pub fn by_id(&self, id: &str) -> Option<gio::AppInfo> {
        if let Some(hit) = self.ids.borrow().get(id) {
            return hit.clone();
        }
        let found = apps::by_id(id);
        self.ids.borrow_mut().insert(id.to_string(), found.clone());
        self.dirty.set(true);
        found
    }

    pub fn for_class(&self, class: &str) -> Option<gio::AppInfo> {
        if let Some(hit) = self.classes.borrow().get(class) {
            return hit.clone();
        }
        let found = apps::for_class(class);
        let mut m = self.classes.borrow_mut();
        if m.len() >= MAX_CLASSES {
            m.clear();
        }
        m.insert(class.to_string(), found.clone());
        self.dirty.set(true);
        found
    }

    /// Free `w11_common::apps` caches if we touched them.
    pub fn release(&self) {
        if self.dirty.take() {
            apps::invalidate();
        }
    }

    /// Installed apps changed: forget everything.
    pub fn clear(&self) {
        self.ids.borrow_mut().clear();
        self.classes.borrow_mut().clear();
        apps::invalidate();
    }
}
