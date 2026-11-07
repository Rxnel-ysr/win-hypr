//! Windows 11 style toast notifications.
//!
//! Owns `org.freedesktop.Notifications` on the session bus, shows toasts above the taskbar
//! (bottom-right) and appends everything to the history shown by `w11-flyout calendar`.
//!
//! * server.rs   D-Bus side (own thread, no GTK)
//! * history.rs  persistent history (in-memory copy, written on change)
//! * toasts.rs   toast window; it only exists while a toast is on screen
//! * card.rs     a single toast card
//! * text.rs     markup stripping + clipping

mod card;
mod history;
mod server;
mod text;
mod toasts;

use gtk::glib;
use gtk::prelude::*;
use w11_common::{config, mem, theme};

use history::History;
use server::Msg;
use toasts::Toasts;

/// Bounded so a notification storm applies back-pressure to D-Bus instead of piling up RAM.
const QUEUE: usize = 64;

fn main() -> glib::ExitCode {
    mem::tune(mem::Profile::Resident);
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "cairo");
    }
    let app = gtk::Application::builder()
        .application_id("dev.win11hypr.Notify")
        .build();
    app.connect_startup(|_| theme::apply());
    app.connect_activate(build);
    app.run()
}

fn build(app: &gtk::Application) {
    if !app.windows().is_empty() {
        return;
    }
    // Keep the daemon alive even though no window exists between toasts.
    std::mem::forget(app.hold());
    w11_common::load_css!("css/toast.css");

    let (tx, rx) = async_channel::bounded::<Msg>(QUEUE);
    if let Err(e) = server::spawn(tx) {
        eprintln!("w11-notify: cannot start D-Bus thread: {e}");
        std::process::exit(1);
    }

    let toasts = Toasts::new(app);
    let mut history = History::default();

    glib::spawn_future_local(async move {
        while let Ok(msg) = rx.recv().await {
            match msg {
                Msg::Show(n, timeout) => {
                    // toast first so it appears immediately, then persist
                    if !config::dnd_enabled() {
                        toasts.show(&n, timeout);
                    }
                    history.push(n); // history is kept even in do-not-disturb
                }
                Msg::Close(id) => toasts.close(id),
            }
        }
    });

    // Resident daemon: give freed heap back only when RSS actually grew.
    mem::keep_trimmed(60, 4096);
}
