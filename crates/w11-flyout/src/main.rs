//! `w11-flyout quick | calendar | taskview`
//!
//! * quick     – Quick Settings (Wi-Fi, Bluetooth, brightness, volume, ...)   -> quick.rs
//! * calendar  – notification center + month calendar                          -> calendar.rs
//! * taskview  – full-screen window switcher with virtual desktops             -> taskview/
//!
//! Every flyout is its own single-instance process (see `w11_common::popup::run_popup`), and each
//! one only loads the stylesheet it needs (`src/css/<flyout>.css`).

mod calendar;
mod mem;
mod quick;
mod taskview;

use gtk::glib;
use w11_common::popup::run_popup;

fn main() -> glib::ExitCode {
    // Static flyouts: cairo avoids creating a GL context (tens of MB RSS on an iGPU).
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "cairo");
    }
    match std::env::args().nth(1).as_deref() {
        Some("quick") => run_popup("dev.win11hypr.QuickSettings", quick::build),
        Some("calendar") => run_popup("dev.win11hypr.Calendar", calendar::build),
        Some("taskview") => run_popup("dev.win11hypr.TaskView", taskview::build),
        _ => {
            eprintln!("usage: w11-flyout <quick|calendar|taskview>");
            glib::ExitCode::FAILURE
        }
    }
}
