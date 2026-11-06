//! Shared helper for loading per-crate stylesheets on top of fluent.css.

use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    /// Addresses of stylesheets already installed, so repeated calls are no-ops.
    static LOADED: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
}

/// Install `css` for the whole display at APPLICATION priority.
/// Safe to call repeatedly: each stylesheet is installed once per process.
pub fn load(css: &'static str) {
    load_with_priority(css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

pub fn load_with_priority(css: &'static str, priority: u32) {
    let key = css.as_ptr() as usize;
    if !LOADED.with(|s| s.borrow_mut().insert(key)) {
        return;
    }
    let Some(display) = gtk::gdk::Display::default() else {
        // No display yet: allow a later retry.
        LOADED.with(|s| s.borrow_mut().remove(&key));
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_data(css);
    gtk::style_context_add_provider_for_display(&display, &provider, priority);
}

/// `w11_common::load_css!("css/start.css")`
/// The path is relative to the file where the macro is called, like `include_str!`.
#[macro_export]
macro_rules! load_css {
    ($path:literal) => {
        $crate::css::load(include_str!($path))
    };
}
