//! Memory hygiene for the flyout processes.

use gtk::glib;
use std::time::Duration;

/// Ask glibc to return freed heap pages to the kernel.
///
/// GTK/GDK free a lot of small blocks (thumbnails, widgets, CSS nodes) when a flyout closes, but
/// glibc keeps them in its arenas, so RSS stays high until the process exits. A short delay lets
/// widget destruction finish first.
pub fn trim_soon() {
    glib::timeout_add_local_once(Duration::from_millis(300), || unsafe {
        libc::malloc_trim(0);
    });
}
