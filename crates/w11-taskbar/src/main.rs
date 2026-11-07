//! The Windows 11 taskbar: centred Start / Search / Task View / app strip,
//! tray cluster, clock + notification bell, "show desktop" sliver.
//!
//! Module map:
//!   strip.rs    running + pinned app buttons (the only part that reacts to Hyprland)
//!   menu.rs     right-click jump menu (built on demand, destroyed on close)
//!   resolver.rs window class / desktop-id -> AppInfo cache that frees the big app list
//!   status.rs   network / volume / battery cluster
//!   clock.rs    clock + date + bell
//!   weather.rs  wttr.in widget
//!   widgets.rs  small shared builders
//!   tray.rs     StatusNotifier host
//!   mem.rs      allocator tuning

mod clock;
mod menu;
mod resolver;
mod status;
mod strip;
mod tray;
mod weather;
mod widgets;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::time::Duration;
use w11_common::config::Config;
use w11_common::popup::TASKBAR_HEIGHT;
use w11_common::{sys, theme, thumbs};

fn main() -> glib::ExitCode {
    // Must run before GTK spawns threads.
    w11_common::mem::tune(w11_common::mem::Profile::Transient);
    // Static bar: cairo avoids creating a GL context (tens of MB RSS on an iGPU).
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "cairo");
    }
    let app = gtk::Application::builder()
        .application_id("dev.win11hypr.Taskbar")
        .build();
    app.connect_startup(|_| {
        theme::apply();
        w11_common::load_css!("css/taskbar.css");
    });
    app.connect_activate(build);
    app.run()
}

fn build(app: &gtk::Application) {
    if !app.windows().is_empty() {
        return;
    }

    let cfg = Config::load();
    let strip = strip::Strip::new(cfg.clone());

    // Re-scan installed apps when something is (un)installed.
    {
        let s = strip.clone();
        let monitor = gio::AppInfoMonitor::get();
        monitor.connect_changed(move |_| s.apps_changed());
        std::mem::forget(monitor);
    }

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .decorated(false)
        .default_height(TASKBAR_HEIGHT)
        .build();
    window.add_css_class("w11-window");
    window.init_layer_shell();
    window.set_namespace("w11-taskbar");
    window.set_layer(Layer::Top);
    for e in [Edge::Left, Edge::Right, Edge::Bottom] {
        window.set_anchor(e, true);
    }
    window.auto_exclusive_zone_enable();

    let root = gtk::CenterBox::new();
    root.add_css_class("taskbar");
    root.set_height_request(TASKBAR_HEIGHT);

    // ---- left: weather
    let left = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    left.set_valign(gtk::Align::Center);
    if let Some(w) = weather::build(&cfg) {
        left.append(&w);
    }
    root.set_start_widget(Some(&left));

    // ---- centre: Start, Search, Task View, apps
    let center = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    center.set_valign(gtk::Align::Center);

    let start = widgets::icon_button(&widgets::windows_logo(), "Start");
    widgets::opens(&start, "w11-start");
    center.append(&start);

    let search = widgets::icon_button(&widgets::symbolic("system-search-symbolic", 20), "Search");
    widgets::opens(&search, "w11-start");
    center.append(&search);

    let taskview = widgets::icon_button(&widgets::symbolic("view-grid-symbolic", 20), "Task View");
    let tv_cmd = cfg.task_view_cmd.clone();
    taskview.connect_clicked(move |_| sys::spawn(&tv_cmd));
    center.append(&taskview);

    center.append(&strip.root);
    root.set_center_widget(Some(&center));

    // ---- right: tray overflow, status cluster, clock, show-desktop sliver
    let end = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    end.set_valign(gtk::Align::Center);
    end.append(&widgets::tray_overflow());
    end.append(&status::build());
    end.append(&clock::build());
    end.append(&widgets::show_desktop_sliver());
    root.set_end_widget(Some(&end));

    window.set_child(Some(&root));
    window.present();
    thumbs::spawn_daemon();

    strip.start();

    // Hand freed heap pages back every few minutes.
    glib::timeout_add_seconds_local(300, || {
        w11_common::mem::trim_soon();
        glib::ControlFlow::Continue
    });
    let _ = Duration::from_secs(0); // (keeps `Duration` import obvious if you add timers)
    w11_common::mem::keep_trimmed(60, 4096);
}
