//! Allocator hygiene for every w11 process (glibc only; a no-op elsewhere).
//!
//! * `tune(Profile)`        – call first thing in `main`
//! * `trim_soon()`          – debounced trim, callable from any thread
//! * `trim_on_hide(&win)`   – flyouts: trim whenever the window hides
//! * `keep_trimmed(..)`     – resident daemons: trim only when RSS has actually grown
//! * `rss_kb()`             – cheap RSS probe; set W11_MEM_DEBUG=1 to log every trim

use gtk::glib;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod ffi {
    extern "C" {
        pub fn malloc_trim(pad: usize) -> i32;
        pub fn mallopt(param: i32, value: i32) -> i32;
    }
}

const M_TRIM_THRESHOLD: i32 = -1;
const M_TOP_PAD: i32 = -2;
const M_MMAP_THRESHOLD: i32 = -3;
const M_ARENA_MAX: i32 = -8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// taskbar, notify: live for the whole session, so be strict.
    Resident,
    /// start, flyouts, settings, explorer: short-lived, bursts of big allocations.
    Transient,
}

/// Call before any thread or GTK object exists. Safe to call twice.
pub fn tune(profile: Profile) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let (trim, mmap, pad) = match profile {
            Profile::Resident => (64 * 1024, 64 * 1024, 32 * 1024),
            Profile::Transient => (128 * 1024, 128 * 1024, 64 * 1024),
        };
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        unsafe {
            ffi::mallopt(M_ARENA_MAX, 1);
            ffi::mallopt(M_TRIM_THRESHOLD, trim);
            ffi::mallopt(M_MMAP_THRESHOLD, mmap);
            ffi::mallopt(M_TOP_PAD, pad);
        }
        #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
        let _ = (trim, mmap, pad, M_ARENA_MAX, M_TRIM_THRESHOLD, M_MMAP_THRESHOLD, M_TOP_PAD);
    });
}

/// Resident set size in KiB (assumes 4 KiB pages, true on x86_64).
pub fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map_or(0, |pages| pages * 4)
}

fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| std::env::var_os("W11_MEM_DEBUG").is_some())
}

/// Return freed heap pages to the kernel right now (walks every arena).
pub fn trim_now() {
    let before = if debug() { rss_kb() } else { 0 };
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        ffi::malloc_trim(0);
    }
    if debug() {
        eprintln!("w11-mem: trim {} -> {} KiB", before, rss_kb());
    }
}

static PENDING: AtomicBool = AtomicBool::new(false);

/// Debounced trim `secs` seconds from now. Any thread; bursts collapse into one trim.
pub fn trim_in(secs: u32) {
    if PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    glib::timeout_add_seconds_once(secs, || {
        PENDING.store(false, Ordering::Release);
        trim_now();
    });
}

pub fn trim_soon() {
    trim_in(2);
}

/// Flyouts: let widget teardown finish, then trim.
pub fn trim_on_hide(w: &impl IsA<gtk::Widget>) {
    w.connect_hide(|_| trim_soon());
}

/// Resident daemons: every `every_secs`, trim only if RSS grew more than `grow_kb`
/// above the lowest value seen. Idle cost is one tiny /proc read per tick, and the
/// baseline follows RSS downward, so slow leaks still get trimmed eventually.
pub fn keep_trimmed(every_secs: u32, grow_kb: u64) {
    let floor = Rc::new(Cell::new(rss_kb()));
    glib::timeout_add_seconds_local(every_secs, move || {
        let now = rss_kb();
        if now > floor.get() + grow_kb {
            trim_now();
            floor.set(rss_kb());
        } else {
            floor.set(floor.get().min(now));
        }
        glib::ControlFlow::Continue
    });
}
