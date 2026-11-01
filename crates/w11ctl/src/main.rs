//! `w11ctl` – window-management verbs that Windows has and Hyprland doesn't.
//!
//!   w11ctl snap <left|right|top-left|top-right|bottom-left|bottom-right|maximize>
//!   w11ctl restore | minimize | show-desktop | alt-tab | close | maximize-toggle
//!   w11ctl workspace <n|next|prev>
//!   w11ctl lock | sleep | shutdown | restart | sign-out

use w11_common::{hypr, sys};

fn usage() -> ! {
    eprintln!(
        "usage: w11ctl <snap DIR | restore | minimize | maximize-toggle | show-desktop | alt-tab | close | workspace N|next|prev | lock | sleep | shutdown | restart | sign-out>"
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("snap") => {
            let Some(mode) = args.get(1).and_then(|s| hypr::Snap::parse(s)) else { usage() };
            hypr::snap_active(mode);
        }
        Some("restore") => {
            // Win+Down: restore if snapped/maximised, otherwise minimise.
            if !hypr::restore_active() {
                hypr::minimize_active();
            }
        }
        Some("minimize") => hypr::minimize_active(),
        Some("maximize-toggle") => hypr::toggle_maximize_active(),
        Some("show-desktop") => hypr::toggle_show_desktop(),
        Some("alt-tab") => hypr::alt_tab(),
        Some("close") => {
            if let Some(c) = hypr::active_window() {
                hypr::close(&c.address);
            }
        }
        Some("workspace") => match args.get(1).map(String::as_str) {
            Some("next") => hypr::switch_workspace_relative(1),
            Some("prev") => hypr::switch_workspace_relative(-1),
            Some(s) => match s.parse::<i64>() {
                Ok(n) => {
                    hypr::switch_workspace(n);
                }
                Err(_) => usage(),
            },
            None => usage(),
        },
        Some("lock") => sys::lock(),
        Some("sleep") => sys::sleep(),
        Some("shutdown") => sys::shutdown(),
        Some("restart") => sys::restart(),
        Some("sign-out") => sys::sign_out(),
        _ => usage(),
    }
    // give spawned helper threads a moment to reap their children
    std::thread::sleep(std::time::Duration::from_millis(30));
}
