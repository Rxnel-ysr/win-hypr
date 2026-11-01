//! The Windows 11 taskbar: centred Start / Search / Task View / app icons,
//! tray cluster, clock + notification bell, "show desktop" sliver.

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;
use w11_common::config::Config;
use w11_common::popup::TASKBAR_HEIGHT;
use w11_common::{apps, hypr, sys, theme, thumbs};
mod tray;

struct State {
    apps_box: gtk::Box,
    clients: Vec<hypr::Client>,
    active: String,
    cfg: Config,
    /// Fingerprint of what the app strip currently shows; we only rebuild when it changes.
    sig: String,
}

type Shared = Rc<RefCell<State>>;

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("dev.win11hypr.Taskbar")
        .build();
    app.connect_startup(|_| theme::apply());
    app.connect_activate(build);
    app.run()
}

fn build(app: &gtk::Application) {
    if !app.windows().is_empty() {
        return;
    }
    // Keep the process alive and re-scan installed apps when something is (un)installed.
    let app_monitor = gio::AppInfoMonitor::get();
    app_monitor.connect_changed(|_| apps::invalidate());
    std::mem::forget(app_monitor);

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

    let cfg = Config::load();
    let root = gtk::CenterBox::new();
    root.add_css_class("taskbar");
    root.set_height_request(TASKBAR_HEIGHT);

    // ---------------------------------------------------------- left: weather
    let left = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    left.set_valign(gtk::Align::Center);
    if cfg.weather {
        let weather = gtk::Button::new();
        weather.add_css_class("tray-button");
        let wb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let wicon = gtk::Image::from_icon_name("weather-few-clouds-symbolic");
        wicon.set_pixel_size(22);
        let wtext = gtk::Label::new(Some("Widgets"));
        wtext.add_css_class("tb-weather-temp");
        wb.append(&wicon);
        wb.append(&wtext);
        weather.set_child(Some(&wb));
        weather.connect_clicked(|_| sys::spawn("w11-flyout calendar"));
        left.append(&weather);
        refresh_weather(wtext.clone(), cfg.weather_location.clone());
        let loc = cfg.weather_location.clone();
        glib::timeout_add_seconds_local(1800, move || {
            refresh_weather(wtext.clone(), loc.clone());
            glib::ControlFlow::Continue
        });
    }
    root.set_start_widget(Some(&left));

    // -------------------------------------------------------------- center
    let center = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    center.set_valign(gtk::Align::Center);

    let start = icon_button(&windows_logo(), "Start");
    start.connect_clicked(|_| sys::spawn("w11-start"));
    center.append(&start);

    let search = icon_button(
        &gtk::Image::builder()
            .icon_name("system-search-symbolic")
            .pixel_size(20)
            .build(),
        "Search",
    );
    search.connect_clicked(|_| sys::spawn("w11-start"));
    center.append(&search);

    let taskview = icon_button(
        &gtk::Image::builder()
            .icon_name("view-grid-symbolic")
            .pixel_size(20)
            .build(),
        "Task View",
    );
    let tv_cmd = cfg.task_view_cmd.clone();
    taskview.connect_clicked(move |_| sys::spawn(&tv_cmd));
    center.append(&taskview);

    let apps_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    center.append(&apps_box);
    root.set_center_widget(Some(&center));

    // ----------------------------------------------------------------- end
    let end = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    end.set_valign(gtk::Align::Center);

    let tray_items = tray::start();
    let chevron = gtk::Button::new();
    chevron.add_css_class("tray-button");
    chevron.set_child(Some(&gtk::Image::from_icon_name("pan-up-symbolic")));
    let tray_pop = gtk::Popover::new();
    tray_pop.set_has_arrow(false);
    tray_pop.set_position(gtk::PositionType::Top);
    tray_pop.set_parent(&chevron);
    let tray_flow = gtk::FlowBox::new();
    tray_flow.set_selection_mode(gtk::SelectionMode::None);
    tray_flow.set_max_children_per_line(5);
    tray_pop.set_child(Some(&tray_flow));
    tray::attach(tray_items, &tray_flow, &tray_pop);
    {
        let p = tray_pop.clone();
        chevron.connect_clicked(move |_| p.popup());
    }
    end.append(&chevron);

    // network / volume / battery cluster -> quick settings
    let quick = gtk::Button::new();
    quick.add_css_class("tray-button");
    let qbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let net_icon = gtk::Image::from_icon_name("network-wireless-symbolic");
    let vol_icon = gtk::Image::from_icon_name("audio-volume-high-symbolic");
    let bat_icon = gtk::Image::from_icon_name("battery-full-symbolic");
    bat_icon.set_visible(false);
    qbox.append(&net_icon);
    qbox.append(&vol_icon);
    qbox.append(&bat_icon);
    quick.set_child(Some(&qbox));
    quick.connect_clicked(|_| sys::spawn("w11-flyout quick"));
    end.append(&quick);

    // clock + date + bell -> calendar & notification center
    let clock_btn = gtk::Button::new();
    clock_btn.add_css_class("tray-button");
    let cbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let ctext = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let time_l = gtk::Label::new(None);
    time_l.add_css_class("tb-clock-time");
    time_l.set_halign(gtk::Align::End);
    let date_l = gtk::Label::new(None);
    date_l.add_css_class("tb-clock-date");
    date_l.set_halign(gtk::Align::End);
    ctext.append(&time_l);
    ctext.append(&date_l);
    let bell = gtk::Image::from_icon_name("preferences-system-notifications-symbolic");
    cbox.append(&ctext);
    cbox.append(&bell);
    clock_btn.set_child(Some(&cbox));
    clock_btn.connect_clicked(|_| sys::spawn("w11-flyout calendar"));
    end.append(&clock_btn);

    let sliver = gtk::Button::new();
    sliver.add_css_class("tb-show-desktop");
    sliver.set_tooltip_text(Some("Show desktop"));
    sliver.connect_clicked(|_| {
        std::thread::spawn(hypr::toggle_show_desktop);
    });
    end.append(&sliver);
    root.set_end_widget(Some(&end));

    window.set_child(Some(&root));
    window.present();
    thumbs::spawn_daemon();

    // ------------------------------------------------------------- state
    let state: Shared = Rc::new(RefCell::new(State {
        apps_box,
        clients: Vec::new(),
        active: String::new(),
        cfg,
        sig: String::new(),
    }));
    refresh_clients(&state);

    // Hyprland events -> refresh the app strip (debounced: bursts of events = one refresh)
    let (tx, rx) = async_channel::unbounded::<String>();
    hypr::listen_events(tx);
    let st = state.clone();
    let pending = Rc::new(Cell::new(false));
    glib::spawn_future_local(async move {
        while let Ok(ev) = rx.recv().await {
            let relevant = matches!(
                ev.as_str(),
                "openwindow"
                    | "closewindow"
                    | "activewindow"
                    | "activewindowv2"
                    | "movewindow"
                    | "movewindowv2"
                    | "windowtitle"
                    | "windowtitlev2"
                    | "workspace"
                    | "workspacev2"
                    | "focusedmon"
            );
            if relevant && !pending.replace(true) {
                let (st, pending) = (st.clone(), pending.clone());
                glib::timeout_add_local_once(Duration::from_millis(40), move || {
                    pending.set(false);
                    refresh_clients(&st);
                });
            }
        }
    });
    // Safety net in case an event is missed; cheap now because unchanged state is ignored.
    let st = state.clone();
    glib::timeout_add_seconds_local(5, move || {
        refresh_clients(&st);
        glib::ControlFlow::Continue
    });

    // clock
    let update_clock = {
        let (time_l, date_l) = (time_l.clone(), date_l.clone());
        move || {
            if let Ok(now) = glib::DateTime::now_local() {
                if let Ok(t) = now.format("%-I:%M %p") {
                    time_l.set_text(&t);
                }
                if let Ok(d) = now.format("%-m/%-d/%Y") {
                    date_l.set_text(&d);
                }
            }
        }
    };
    update_clock();
    glib::timeout_add_seconds_local(1, move || {
        update_clock();
        glib::ControlFlow::Continue
    });

    // tray status (polls CLI tools off the main thread)
    let poll_tray = move || {
        let (net_icon, vol_icon, bat_icon) = (net_icon.clone(), vol_icon.clone(), bat_icon.clone());
        glib::spawn_future_local(async move {
            let res = gio::spawn_blocking(|| {
                let (v, m) = sys::volume();
                (sys::network_icon(), sys::volume_icon(v, m), sys::battery())
            })
            .await;
            if let Ok((net, vol, bat)) = res {
                net_icon.set_icon_name(Some(net));
                vol_icon.set_icon_name(Some(vol));
                match bat {
                    Some(b) => {
                        bat_icon.set_icon_name(Some(&sys::battery_icon(b)));
                        bat_icon.set_tooltip_text(Some(&format!("{}%", b.percent)));
                        bat_icon.set_visible(true);
                    }
                    None => bat_icon.set_visible(false),
                }
            }
        });
    };
    poll_tray();
    glib::timeout_add_seconds_local(5, move || {
        poll_tray();
        glib::ControlFlow::Continue
    });
}

fn refresh_weather(label: gtk::Label, loc: String) {
    glib::spawn_future_local(async move {
        let res = gio::spawn_blocking(move || {
            sys::sh(&format!(
                "curl -s --max-time 6 'https://wttr.in/{}?format=%t'",
                loc.replace(' ', "+")
            ))
        })
        .await;
        if let Ok(t) = res {
            // wttr returns e.g. "+23°C"; ignore HTML/error responses
            if !t.is_empty() && t.len() < 12 && !t.contains('<') {
                label.set_text(t.trim_start_matches('+'));
            }
        }
    });
}

fn windows_logo() -> gtk::Grid {
    let g = gtk::Grid::new();
    g.set_row_spacing(2);
    g.set_column_spacing(2);
    g.set_halign(gtk::Align::Center);
    g.set_valign(gtk::Align::Center);
    for (c, r) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let p = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        p.add_css_class("win-logo-pane");
        g.attach(&p, c, r, 1, 1);
    }
    g
}

fn icon_button(child: &impl IsA<gtk::Widget>, tip: &str) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("tb-button");
    b.set_child(Some(child));
    b.set_tooltip_text(Some(tip));
    b.set_width_request(44);
    b
}

/// What the app strip depends on. Titles are deliberately left out: terminals and browsers
/// change them constantly and that must not tear down the buttons under the cursor.
fn signature(clients: &[hypr::Client], active: &str) -> String {
    let mut s = String::from(active);
    for w in clients {
        s.push_str(&format!(
            "|{}:{}:{}:{}",
            w.address, w.class, w.workspace.id, w.focus_history_id
        ));
    }
    s
}

/// Query Hyprland off the UI thread, and only rebuild the strip if something changed.
fn refresh_clients(state: &Shared) {
    let st = state.clone();
    glib::spawn_future_local(async move {
        let Ok((clients, active)) = gio::spawn_blocking(|| {
            (
                hypr::clients(),
                hypr::active_window().map(|c| c.address).unwrap_or_default(),
            )
        })
        .await
        else {
            return;
        };
        let sig = signature(&clients, &active);
        {
            let mut s = st.borrow_mut();
            if s.sig == sig {
                return;
            }
            s.sig = sig;
            s.clients = clients;
            s.active = active;
        }
        rebuild(&st);
    });
}

fn rebuild(state: &Shared) {
    let (apps_box, clients, active, cfg) = {
        let s = state.borrow();
        (
            s.apps_box.clone(),
            s.clients.clone(),
            s.active.clone(),
            s.cfg.clone(),
        )
    };
    while let Some(c) = apps_box.first_child() {
        apps_box.remove(&c);
    }

    let mut used: Vec<String> = Vec::new();

    // pinned apps first
    for id in &cfg.pinned {
        let Some(info) = apps::by_id(id) else {
            continue;
        };
        let wins: Vec<hypr::Client> = clients
            .iter()
            .filter(|c| apps::class_matches(&info, &c.class))
            .cloned()
            .collect();
        used.extend(wins.iter().map(|w| w.address.clone()));
        apps_box.append(&app_button(
            state,
            Some(info),
            String::new(),
            wins,
            &active,
            true,
        ));
    }

    // then running apps that aren't pinned, grouped by class (oldest focus first)
    let mut classes: Vec<String> = Vec::new();
    for c in &clients {
        if !used.contains(&c.address) && !classes.contains(&c.class) {
            classes.push(c.class.clone());
        }
    }
    for class in classes {
        let wins: Vec<hypr::Client> = clients
            .iter()
            .filter(|c| c.class == class && !used.contains(&c.address))
            .cloned()
            .collect();
        let info = apps::for_class(&class);
        apps_box.append(&app_button(state, info, class, wins, &active, false));
    }
}

fn app_button(
    state: &Shared,
    info: Option<gio::AppInfo>,
    class: String,
    wins: Vec<hypr::Client>,
    active: &str,
    pinned: bool,
) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    let button = gtk::Button::new();
    button.add_css_class("tb-button");
    button.set_width_request(44);

    let image = match &info {
        Some(i) => gtk::Image::from_gicon(&apps::icon_of(i)),
        None => gtk::Image::from_icon_name(&class.to_lowercase()),
    };
    image.set_pixel_size(24);
    if info.is_none() && image.icon_name().is_none() {
        image.set_icon_name(Some("application-x-executable"));
    }
    button.set_child(Some(&image));
    let name = info
        .as_ref()
        .map(|i| i.name().to_string())
        .unwrap_or_else(|| class.clone());
    let tip = match wins.first() {
        Some(w) if wins.len() == 1 && !w.title.is_empty() => w.title.clone(),
        Some(_) => format!("{name} ({} windows)", wins.len()),
        None => name.clone(),
    };
    button.set_tooltip_text(Some(&tip));
    overlay.set_child(Some(&button));

    if !wins.is_empty() {
        let ind = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        ind.add_css_class("tb-indicator");
        if wins.iter().any(|w| w.address == active) {
            ind.add_css_class("active");
        }
        ind.set_halign(gtk::Align::Center);
        ind.set_valign(gtk::Align::End);
        ind.set_can_target(false);
        overlay.add_overlay(&ind);
    }

    // left click: launch / focus / minimise / cycle
    {
        let wins = wins.clone();
        let info = info.clone();
        let active = active.to_string();
        button.connect_clicked(move |_| {
            if wins.is_empty() {
                if let Some(i) = &info {
                    apps::launch(i);
                }
                return;
            }
            let wins = wins.clone();
            let active = active.clone();
            std::thread::spawn(move || click_windows(&wins, &active));
        });
    }

    // middle click: new window
    {
        let info = info.clone();
        let mid = gtk::GestureClick::builder().button(2).build();
        mid.connect_pressed(move |_, _, _, _| {
            if let Some(i) = &info {
                apps::launch(i);
            }
        });
        button.add_controller(mid);
    }

    // right click: jump-list-ish menu
    {
        let popover = gtk::Popover::new();
        popover.set_position(gtk::PositionType::Top);
        popover.set_has_arrow(false);
        popover.set_parent(&overlay);
        let menu = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let entry = |label: &str| {
            let b = gtk::Button::with_label(label);
            b.add_css_class("menu-item");
            if let Some(l) = b.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
                l.set_xalign(0.0);
            }
            b
        };

        if let Some(i) = info.clone() {
            let b = entry(&name);
            let p = popover.clone();
            b.connect_clicked(move |_| {
                p.popdown();
                apps::launch(&i);
            });
            menu.append(&b);
        }
        if let Some(i) = &info {
            let id = apps::desktop_id(i);
            let b = entry(if pinned {
                "Unpin from taskbar"
            } else {
                "Pin to taskbar"
            });
            let (p, st) = (popover.clone(), state.clone());
            b.connect_clicked(move |_| {
                p.popdown();
                {
                    let mut s = st.borrow_mut();
                    if let Some(pos) = s.cfg.pinned.iter().position(|x| *x == id) {
                        s.cfg.pinned.remove(pos);
                    } else {
                        s.cfg.pinned.push(id.clone());
                    }
                    s.cfg.save();
                }
                let st = st.clone();
                glib::idle_add_local_once(move || rebuild(&st));
            });
            menu.append(&b);
        }
        if !wins.is_empty() {
            let b = entry(if wins.len() > 1 {
                "Close all windows"
            } else {
                "Close window"
            });
            let (p, addrs) = (
                popover.clone(),
                wins.iter().map(|w| w.address.clone()).collect::<Vec<_>>(),
            );
            b.connect_clicked(move |_| {
                p.popdown();
                let addrs = addrs.clone();
                std::thread::spawn(move || {
                    for a in addrs {
                        hypr::close(&a);
                    }
                });
            });
            menu.append(&b);
        }
        popover.set_child(Some(&menu));

        let pop = popover.clone();
        let right = gtk::GestureClick::builder().button(3).build();
        right.connect_pressed(move |_, _, _, _| pop.popup());
        button.add_controller(right);
        overlay.connect_destroy(move |_| popover.unparent());
    }

    overlay
}

/// Windows semantics for clicking a taskbar button that has open windows.
fn click_windows(wins: &[hypr::Client], active: &str) {
    let mut sorted = wins.to_vec();
    sorted.sort_by_key(|w| w.focus_history_id);
    match sorted.iter().position(|w| w.address == active) {
        // one window and it's focused -> minimise; several -> cycle to the next
        Some(_) if sorted.len() == 1 => {
            hypr::minimize(&sorted[0].address);
        }
        Some(i) => {
            hypr::restore_and_focus(&sorted[(i + 1) % sorted.len()]);
        }
        None => {
            hypr::restore_and_focus(&sorted[0]);
        }
    }
}
