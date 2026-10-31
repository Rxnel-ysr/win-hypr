//! `w11-flyout quick | calendar | taskview`
//!
//! * quick     – Quick Settings (Wi-Fi, Bluetooth, brightness, volume, ...)
//! * calendar  – notification center + month calendar
//! * taskview  – full-screen window switcher with virtual desktops

use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use w11_common::config::{self, StoredNotification};
use w11_common::popup::{run_popup, Placement, Popup};
use w11_common::{apps, hypr, sys};

fn main() -> glib::ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("quick") => run_popup("dev.win11hypr.QuickSettings", quick),
        Some("calendar") => run_popup("dev.win11hypr.Calendar", calendar),
        Some("taskview") => run_popup("dev.win11hypr.TaskView", taskview),
        _ => {
            eprintln!("usage: w11-flyout <quick|calendar|taskview>");
            glib::ExitCode::FAILURE
        }
    }
}

// =====================================================================
// Quick settings
// =====================================================================

struct Tile {
    button: gtk::Button,
    caption: gtk::Label,
    get: fn() -> bool,
    caption_fn: Option<fn() -> String>,
}

fn make_tile(
    icon: &str,
    label: &str,
    get: fn() -> bool,
    set: fn(bool),
    caption_fn: Option<fn() -> String>,
) -> Rc<Tile> {
    let b = gtk::Button::new();
    b.add_css_class("quick-tile");
    let h = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let img = gtk::Image::from_icon_name(icon);
    img.set_pixel_size(18);
    let v = gtk::Box::new(gtk::Orientation::Vertical, 0);
    v.set_valign(gtk::Align::Center);
    let l = gtk::Label::new(Some(label));
    l.add_css_class("quick-tile-label");
    l.set_xalign(0.0);
    let cap = gtk::Label::new(None);
    cap.add_css_class("quick-tile-caption");
    cap.set_xalign(0.0);
    v.append(&l);
    v.append(&cap);
    h.append(&img);
    h.append(&v);
    b.set_child(Some(&h));
    b.set_hexpand(true);
    let tile = Rc::new(Tile { button: b.clone(), caption: cap, get, caption_fn });
    let t = tile.clone();
    b.connect_clicked(move |btn| {
        let on = !btn.has_css_class("on");
        set(on);
        // optimistic UI; real state is re-read shortly after
        if on {
            btn.add_css_class("on");
        } else {
            btn.remove_css_class("on");
        }
        let t = t.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(900), move || t.refresh());
    });
    tile
}

impl Tile {
    fn refresh(&self) {
        let on = (self.get)();
        if on {
            self.button.add_css_class("on");
        } else {
            self.button.remove_css_class("on");
        }
        let cap = match self.caption_fn {
            Some(f) => f(),
            None => (if on { "On" } else { "Off" }).to_string(),
        };
        self.caption.set_text(&cap);
    }
}

fn slider_row(icon: &gtk::Image, max: f64, value: f64, on_change: impl Fn(f64) + 'static) -> (gtk::Box, gtk::Scale) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    icon.set_pixel_size(18);
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, max, 1.0);
    scale.set_hexpand(true);
    scale.set_draw_value(false);
    scale.set_value(value);
    // debounce so dragging doesn't fork a process per pixel
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let on_change = Rc::new(on_change);
    scale.connect_value_changed(move |s| {
        if let Some(id) = pending.borrow_mut().take() {
            id.remove();
        }
        let v = s.value();
        let (p2, f) = (pending.clone(), on_change.clone());
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || {
            p2.borrow_mut().take();
            f(v);
        });
        *pending.borrow_mut() = Some(id);
    });
    row.append(icon);
    row.append(&scale);
    (row, scale)
}

fn quick(app: &gtk::Application) -> Rc<Popup> {
    let popup = Popup::new(app, "w11-quick", 360, 420, Placement::BottomRight);
    let flyout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    flyout.add_css_class("flyout");
    flyout.set_size_request(360, -1);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
    body.add_css_class("quick-body");

    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(8);
    grid.set_column_homogeneous(true);

    let tiles: Vec<Rc<Tile>> = vec![
        make_tile(
            "network-wireless-symbolic",
            "Wi-Fi",
            sys::wifi_enabled,
            sys::set_wifi,
            Some(|| {
                if !sys::wifi_enabled() {
                    "Off".into()
                } else {
                    sys::wifi_ssid().unwrap_or_else(|| "Not connected".into())
                }
            }),
        ),
        make_tile(
            "bluetooth-symbolic",
            "Bluetooth",
            sys::bluetooth_enabled,
            sys::set_bluetooth,
            None,
        ),
        make_tile(
            "airplane-mode-symbolic",
            "Airplane mode",
            sys::airplane_mode,
            sys::set_airplane_mode,
            None,
        ),
        make_tile(
            "battery-level-100-symbolic",
            "Battery saver",
            sys::battery_saver,
            sys::set_battery_saver,
            None,
        ),
        make_tile(
            "night-light-symbolic",
            "Night light",
            sys::night_light,
            sys::set_night_light,
            None,
        ),
        make_tile(
            "notifications-disabled-symbolic",
            "Focus assist",
            config::dnd_enabled,
            config::set_dnd,
            Some(|| {
                if config::dnd_enabled() { "Do not disturb".into() } else { "Off".into() }
            }),
        ),
    ];
    for (i, t) in tiles.iter().enumerate() {
        grid.attach(&t.button, (i % 2) as i32, (i / 2) as i32, 1, 1);
    }
    body.append(&grid);

    // brightness
    let bright_icon = gtk::Image::from_icon_name("display-brightness-symbolic");
    let bright = sys::brightness();
    let (brow, bscale) = slider_row(&bright_icon, 100.0, bright.unwrap_or(100.0), sys::set_brightness);
    brow.set_visible(bright.is_some());
    body.append(&brow);

    // volume
    let (v, muted) = sys::volume();
    let vol_icon = gtk::Image::from_icon_name(sys::volume_icon(v, muted));
    let vol_btn = gtk::Button::new();
    vol_btn.add_css_class("nav-button");
    vol_btn.set_child(Some(&vol_icon));
    let vi = vol_icon.clone();
    let vimg = gtk::Image::new();
    let (vrow, vscale) = slider_row(&vimg, 100.0, v * 100.0, move |val| {
        sys::set_volume(val / 100.0);
        vi.set_icon_name(Some(sys::volume_icon(val / 100.0, false)));
    });
    // replace the plain icon with a clickable mute button
    if let Some(first) = vrow.first_child() {
        vrow.remove(&first);
    }
    vrow.prepend(&vol_btn);
    vol_btn.connect_clicked({
        let vol_icon = vol_icon.clone();
        move |_| {
            sys::toggle_mute();
            let m = !sys::volume().1;
            let vv = sys::volume().0;
            vol_icon.set_icon_name(Some(sys::volume_icon(vv, m)));
        }
    });
    body.append(&vrow);
    flyout.append(&body);

    // footer
    let footer = gtk::CenterBox::new();
    footer.add_css_class("start-footer");
    let battery_label = gtk::Label::new(None);
    let bat_icon = gtk::Image::from_icon_name("battery-full-symbolic");
    let bat = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bat.append(&bat_icon);
    bat.append(&battery_label);
    footer.set_start_widget(Some(&bat));
    let settings = gtk::Button::new();
    settings.add_css_class("power-button");
    settings.set_icon_name("emblem-system-symbolic");
    settings.set_tooltip_text(Some("All settings"));
    settings.connect_clicked({
        let p = popup.clone();
        move |_| {
            sys::spawn("gnome-control-center || systemsettings || true");
            p.hide();
        }
    });
    footer.set_end_widget(Some(&settings));
    flyout.append(&footer);
    popup.window.set_child(Some(&flyout));

    popup.window.connect_show(move |_| {
        for t in &tiles {
            t.refresh();
        }
        let (v, m) = sys::volume();
        vscale.set_value(v * 100.0);
        vol_icon.set_icon_name(Some(sys::volume_icon(v, m)));
        if let Some(b) = sys::brightness() {
            bscale.set_value(b);
        }
        match sys::battery() {
            Some(b) => {
                bat.set_visible(true);
                bat_icon.set_icon_name(Some(&sys::battery_icon(b)));
                battery_label.set_text(&format!("{}%", b.percent));
            }
            None => bat.set_visible(false),
        }
    });
    popup
}

// =====================================================================
// Calendar + notification center
// =====================================================================

fn calendar(app: &gtk::Application) -> Rc<Popup> {
    let popup = Popup::new(app, "w11-calendar", 380, 700, Placement::BottomRight);
    let flyout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    flyout.add_css_class("flyout");
    flyout.set_size_request(380, 680);

    // header
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    head.set_margin_top(16);
    head.set_margin_start(20);
    head.set_margin_end(20);
    head.set_margin_bottom(8);
    let title = gtk::Label::new(Some("Notifications"));
    title.add_css_class("section-title");
    title.set_hexpand(true);
    title.set_xalign(0.0);
    let clear = gtk::Button::with_label("Clear all");
    clear.add_css_class("link-button");
    head.append(&title);
    head.append(&clear);
    flyout.append(&head);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    list.set_margin_start(16);
    list.set_margin_end(16);
    scroll.set_child(Some(&list));
    flyout.append(&scroll);

    // calendar panel
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 8);
    panel.add_css_class("calendar-panel");
    let date_title = gtk::Label::new(None);
    date_title.add_css_class("calendar-title");
    date_title.set_xalign(0.0);
    let cal = gtk::Calendar::new();
    panel.append(&date_title);
    panel.append(&cal);
    flyout.append(&panel);
    popup.window.set_child(Some(&flyout));

    let fill = {
        let list = list.clone();
        let title = title.clone();
        Rc::new(move || {
            while let Some(c) = list.first_child() {
                list.remove(&c);
            }
            let items = config::load_notifications();
            title.set_text(if config::dnd_enabled() {
                "Notifications (Do not disturb)"
            } else {
                "Notifications"
            });
            if items.is_empty() {
                let l = gtk::Label::new(Some("No new notifications"));
                l.add_css_class("notif-app");
                l.set_margin_top(60);
                l.set_margin_bottom(60);
                list.append(&l);
                return;
            }
            for n in items.iter().rev() {
                list.append(&notif_card(n));
            }
        })
    };

    {
        let fill = fill.clone();
        clear.connect_clicked(move |_| {
            config::save_notifications(&[]);
            fill();
        });
    }
    {
        let (cal, date_title, fill) = (cal.clone(), date_title.clone(), fill.clone());
        popup.window.connect_show(move |_| {
            fill();
            if let Ok(now) = glib::DateTime::now_local() {
                cal.select_day(&now);
                if let Ok(s) = now.format("%A, %B %-d") {
                    date_title.set_text(&s);
                }
            }
        });
    }
    popup
}

fn notif_card(n: &StoredNotification) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    card.add_css_class("notif-card");
    let icon = if n.icon.is_empty() {
        gtk::Image::from_icon_name("dialog-information-symbolic")
    } else {
        gtk::Image::from_icon_name(&n.icon)
    };
    icon.set_pixel_size(28);
    icon.set_valign(gtk::Align::Start);
    let v = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let when = glib::DateTime::from_unix_local(n.time)
        .ok()
        .and_then(|d| d.format("%-I:%M %p").ok())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let app = gtk::Label::new(Some(&format!("{}  ·  {}", n.app, when)));
    app.add_css_class("notif-app");
    app.set_xalign(0.0);
    let sum = gtk::Label::new(Some(&n.summary));
    sum.add_css_class("notif-summary");
    sum.set_xalign(0.0);
    sum.set_wrap(true);
    v.append(&app);
    v.append(&sum);
    if !n.body.is_empty() {
        let b = gtk::Label::new(Some(&n.body));
        b.add_css_class("notif-body");
        b.set_xalign(0.0);
        b.set_wrap(true);
        b.set_lines(3);
        b.set_ellipsize(gtk::pango::EllipsizeMode::End);
        v.append(&b);
    }
    card.append(&icon);
    card.append(&v);
    card
}

// =====================================================================
// Task View
// =====================================================================

fn taskview(app: &gtk::Application) -> Rc<Popup> {
    let popup = Popup::new(app, "w11-taskview", 0, 0, Placement::Fullscreen);
    popup.window.add_css_class("taskview-bg");

    let root = gtk::Box::new(gtk::Orientation::Vertical, 24);
    root.set_margin_top(48);
    root.set_margin_bottom(48);
    root.set_margin_start(64);
    root.set_margin_end(64);

    let chips = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    chips.set_halign(gtk::Align::Center);
    root.append(&chips);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(true);
    flow.set_column_spacing(20);
    flow.set_row_spacing(20);
    flow.set_halign(gtk::Align::Center);
    flow.set_valign(gtk::Align::Start);
    flow.set_max_children_per_line(5);
    scroll.set_child(Some(&flow));
    root.append(&scroll);

    // clicking empty space closes
    let bg = gtk::GestureClick::new();
    let p = popup.clone();
    bg.connect_released(move |g, _, x, y| {
        if let Some(w) = g.widget() {
            if w.pick(x, y, gtk::PickFlags::DEFAULT).map(|p| p == w).unwrap_or(false) {
                p.hide();
            }
        }
    });
    root.add_controller(bg);
    popup.window.set_child(Some(&root));

    let populate = {
        let (flow, chips, popup) = (flow.clone(), chips.clone(), popup.clone());
        move || {
            while let Some(c) = flow.first_child() {
                flow.remove(&c);
            }
            while let Some(c) = chips.first_child() {
                chips.remove(&c);
            }
            let current = hypr::active_workspace_id().unwrap_or(1);
            for id in 1..=4i64 {
                let b = gtk::Button::with_label(&format!("Desktop {id}"));
                b.add_css_class("desktop-chip");
                if id == current {
                    b.add_css_class("current");
                }
                let p = popup.clone();
                b.connect_clicked(move |_| {
                    p.hide();
                    std::thread::spawn(move || {
                        hypr::switch_workspace(id);
                    });
                });
                chips.append(&b);
            }
            let mut clients = hypr::clients();
            clients.sort_by_key(|c| c.focus_history_id);
            for c in clients {
                let b = gtk::Button::new();
                b.add_css_class("taskview-card");
                let v = gtk::Box::new(gtk::Orientation::Vertical, 10);
                let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let img = match apps::for_class(&c.class) {
                    Some(i) => gtk::Image::from_gicon(&apps::icon_of(&i)),
                    None => gtk::Image::from_icon_name("application-x-executable"),
                };
                img.set_pixel_size(20);
                let t = gtk::Label::new(Some(if c.title.is_empty() { &c.class } else { &c.title }));
                t.add_css_class("taskview-title");
                t.set_ellipsize(gtk::pango::EllipsizeMode::End);
                t.set_max_width_chars(28);
                t.set_xalign(0.0);
                top.append(&img);
                top.append(&t);
                let big = gtk::Image::from_gicon(
                    &apps::for_class(&c.class)
                        .map(|i| apps::icon_of(&i))
                        .unwrap_or_else(|| gtk::gio::ThemedIcon::new("application-x-executable").upcast()),
                );
                big.set_pixel_size(72);
                big.set_vexpand(true);
                v.append(&top);
                v.append(&big);
                let ws = gtk::Label::new(Some(&if c.minimized() {
                    "Minimized".to_string()
                } else {
                    format!("Desktop {}", c.workspace.id)
                }));
                ws.add_css_class("rec-sub");
                v.append(&ws);
                b.set_child(Some(&v));
                let (p, cc) = (popup.clone(), c.clone());
                b.connect_clicked(move |_| {
                    p.hide();
                    let cc = cc.clone();
                    std::thread::spawn(move || {
                        hypr::restore_and_focus(&cc);
                    });
                });
                flow.insert(&b, -1);
            }
        }
    };
    popup.window.connect_show(move |_| populate());
    popup
}
