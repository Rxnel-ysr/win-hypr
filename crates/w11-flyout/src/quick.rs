//! Quick Settings flyout (Win+A / the network-volume cluster on the taskbar).
//!
//! All system state is read in ONE off-thread `snapshot()` (the slow probes run in parallel), so
//! opening the flyout never blocks the UI thread and every CLI tool is forked at most once per
//! refresh. Sliders are guarded while the UI is being synced, so refreshing never re-applies the
//! values it just read (which used to fork `wpctl` / `brightnessctl` on every open).

use crate::mem;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;
use std::time::Duration;
use w11_common::config;
use w11_common::popup::{Placement, Popup};
use w11_common::sys;

// ------------------------------------------------------------------ state

/// Everything the flyout shows, gathered in one go.
struct Snapshot {
    wifi: bool,
    ssid: Option<String>,
    bt: bool,
    saver: bool,
    night: bool,
    dnd: bool,
    volume: f64,
    muted: bool,
    brightness: Option<f64>,
    battery: Option<sys::Battery>,
}

fn snapshot() -> Snapshot {
    let wifi = sys::wifi_enabled(); // sysfs read, cheap
    std::thread::scope(|s| {
        // the slow ones fork a CLI tool: run them side by side
        let ssid = s.spawn(move || if wifi { sys::wifi_ssid() } else { None });
        let bt = s.spawn(sys::bluetooth_enabled);
        let saver = s.spawn(sys::battery_saver);
        let bright = s.spawn(sys::brightness);
        let (volume, muted) = sys::volume();
        Snapshot {
            wifi,
            ssid: ssid.join().unwrap_or(None),
            bt: bt.join().unwrap_or(false),
            saver: saver.join().unwrap_or(false),
            night: sys::night_light(),
            dnd: config::dnd_enabled(),
            volume,
            muted,
            brightness: bright.join().unwrap_or(None),
            battery: sys::battery(),
        }
    })
}

// ------------------------------------------------------------------ tiles

#[derive(Clone, Copy)]
enum Kind {
    Wifi,
    Bluetooth,
    Airplane,
    BatterySaver,
    NightLight,
    FocusAssist,
}

const TILES: [(Kind, &str, &str); 6] = [
    (Kind::Wifi, "network-wireless-symbolic", "Wi-Fi"),
    (Kind::Bluetooth, "bluetooth-symbolic", "Bluetooth"),
    (Kind::Airplane, "airplane-mode-symbolic", "Airplane mode"),
    (
        Kind::BatterySaver,
        "battery-level-100-symbolic",
        "Battery saver",
    ),
    (Kind::NightLight, "night-light-symbolic", "Night light"),
    (
        Kind::FocusAssist,
        "notifications-disabled-symbolic",
        "Focus assist",
    ),
];

impl Kind {
    fn set(self, on: bool) {
        match self {
            Kind::Wifi => sys::set_wifi(on),
            Kind::Bluetooth => sys::set_bluetooth(on),
            Kind::Airplane => sys::set_airplane_mode(on),
            Kind::BatterySaver => sys::set_battery_saver(on),
            Kind::NightLight => sys::set_night_light(on),
            Kind::FocusAssist => config::set_dnd(on),
        }
    }

    /// (is it on, caption under the label)
    fn state(self, s: &Snapshot) -> (bool, String) {
        let on = match self {
            Kind::Wifi => s.wifi,
            Kind::Bluetooth => s.bt,
            Kind::Airplane => !s.wifi && !s.bt,
            Kind::BatterySaver => s.saver,
            Kind::NightLight => s.night,
            Kind::FocusAssist => s.dnd,
        };
        let caption = match self {
            Kind::Wifi if !s.wifi => "Off".to_string(),
            Kind::Wifi => s.ssid.clone().unwrap_or_else(|| "Not connected".into()),
            Kind::FocusAssist if on => "Do not disturb".to_string(),
            _ => String::from(if on { "On" } else { "Off" }),
        };
        (on, caption)
    }
}

struct Tile {
    kind: Kind,
    button: gtk::Button,
    caption: gtk::Label,
}

/// Lets tile click handlers reach the `Quick` that is built after them. The reference cycle is
/// harmless: the flyout lives as long as the process.
type Hub = Rc<OnceCell<Rc<Quick>>>;

fn set_class(w: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    if on {
        w.add_css_class(class);
    } else {
        w.remove_css_class(class);
    }
}

fn make_tile(kind: Kind, icon: &str, label: &str, hub: &Hub) -> Tile {
    let button = gtk::Button::new();
    button.add_css_class("quick-tile");
    button.set_hexpand(true);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let img = gtk::Image::from_icon_name(icon);
    img.set_pixel_size(18);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
    text.set_valign(gtk::Align::Center);
    let name = gtk::Label::new(Some(label));
    name.add_css_class("quick-tile-label");
    name.set_xalign(0.0);
    let caption = gtk::Label::new(None);
    caption.add_css_class("quick-tile-caption");
    caption.set_xalign(0.0);
    text.append(&name);
    text.append(&caption);
    row.append(&img);
    row.append(&text);
    button.set_child(Some(&row));

    let hub = hub.clone();
    button.connect_clicked(move |btn| {
        let on = !btn.has_css_class("on");
        kind.set(on);
        set_class(btn, "on", on); // optimistic; the real state is re-read shortly after
        let hub = hub.clone();
        glib::timeout_add_local_once(Duration::from_millis(900), move || {
            if let Some(q) = hub.get() {
                q.refresh();
            }
        });
    });

    Tile {
        kind,
        button,
        caption,
    }
}

// ----------------------------------------------------------------- sliders

/// `lead` (icon or mute button) + a 0..100 slider. `on_change` is debounced and is NOT called
/// while `syncing` is set (i.e. when the UI itself moves the slider).
fn slider_row(
    lead: &impl IsA<gtk::Widget>,
    syncing: Rc<Cell<bool>>,
    on_change: impl Fn(f64) + 'static,
) -> (gtk::Box, gtk::Scale) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_hexpand(true);
    scale.set_draw_value(false);

    // debounce so dragging doesn't fork a process per pixel
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let on_change = Rc::new(on_change);
    scale.connect_value_changed(move |s| {
        if syncing.get() {
            return;
        }
        if let Some(id) = pending.borrow_mut().take() {
            id.remove();
        }
        let v = s.value();
        let (p, f) = (pending.clone(), on_change.clone());
        let id = glib::timeout_add_local_once(Duration::from_millis(60), move || {
            p.borrow_mut().take();
            f(v);
        });
        *pending.borrow_mut() = Some(id);
    });

    row.append(lead);
    row.append(&scale);
    (row, scale)
}

// ------------------------------------------------------------------- Quick

struct Quick {
    tiles: Vec<Tile>,
    bright_row: gtk::Box,
    bright: gtk::Scale,
    vol: gtk::Scale,
    vol_icon: gtk::Image,
    bat: gtk::Box,
    bat_icon: gtk::Image,
    bat_label: gtk::Label,
    syncing: Rc<Cell<bool>>,
    muted: Rc<Cell<bool>>,
    level: Rc<Cell<f64>>,
}

impl Quick {
    /// Read the system state off the UI thread, then paint it.
    fn refresh(self: &Rc<Self>) {
        let q = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(s) = gio::spawn_blocking(snapshot).await {
                q.apply(&s);
            }
        });
    }

    fn apply(&self, s: &Snapshot) {
        self.syncing.set(true);

        for t in &self.tiles {
            let (on, caption) = t.kind.state(s);
            set_class(&t.button, "on", on);
            t.caption.set_text(&caption);
        }

        self.bright_row.set_visible(s.brightness.is_some());
        if let Some(b) = s.brightness {
            self.bright.set_value(b);
        }

        self.muted.set(s.muted);
        self.level.set(s.volume.min(1.0));
        self.vol.set_value(s.volume * 100.0);
        self.vol_icon
            .set_icon_name(Some(sys::volume_icon(s.volume, s.muted)));

        match s.battery {
            Some(b) => {
                self.bat_icon.set_icon_name(Some(&sys::battery_icon(b)));
                self.bat_label.set_text(&format!("{}%", b.percent));
                self.bat.set_visible(true);
            }
            None => self.bat.set_visible(false),
        }

        self.syncing.set(false);
    }
}

pub fn build(app: &gtk::Application) -> Rc<Popup> {
    w11_common::load_css!("css/quick.css");
    let popup = Popup::new(app, "w11-quick", 360, 420, Placement::BottomRight);

    let flyout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    flyout.add_css_class("flyout");
    flyout.set_size_request(360, -1);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
    body.add_css_class("quick-body");

    let hub: Hub = Rc::default();
    let syncing = Rc::new(Cell::new(false));
    let muted = Rc::new(Cell::new(false));
    let level = Rc::new(Cell::new(1.0_f64));

    // ---- toggle tiles
    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(8);
    grid.set_column_homogeneous(true);
    let tiles: Vec<Tile> = TILES
        .iter()
        .map(|&(kind, icon, label)| make_tile(kind, icon, label, &hub))
        .collect();
    for (i, t) in tiles.iter().enumerate() {
        grid.attach(&t.button, (i % 2) as i32, (i / 2) as i32, 1, 1);
    }
    body.append(&grid);

    // ---- brightness (hidden until we know a backlight exists)
    let bright_icon = gtk::Image::from_icon_name("display-brightness-symbolic");
    bright_icon.set_pixel_size(18);
    let (bright_row, bright) = slider_row(&bright_icon, syncing.clone(), sys::set_brightness);
    bright_row.set_visible(false);
    body.append(&bright_row);

    // ---- volume (the icon is a mute button)
    let vol_icon = gtk::Image::from_icon_name("audio-volume-high-symbolic");
    vol_icon.set_pixel_size(18);
    let vol_btn = gtk::Button::new();
    vol_btn.add_css_class("nav-button");
    vol_btn.set_child(Some(&vol_icon));
    let (vol_row, vol) = {
        let (icon, muted, level) = (vol_icon.clone(), muted.clone(), level.clone());
        slider_row(&vol_btn, syncing.clone(), move |v| {
            let v = v / 100.0;
            sys::set_volume(v);
            level.set(v);
            icon.set_icon_name(Some(sys::volume_icon(v, muted.get())));
        })
    };
    {
        let (icon, muted, level) = (vol_icon.clone(), muted.clone(), level.clone());
        vol_btn.connect_clicked(move |_| {
            sys::toggle_mute();
            muted.set(!muted.get()); // no extra `wpctl` round-trip on the UI thread
            icon.set_icon_name(Some(sys::volume_icon(level.get(), muted.get())));
        });
    }
    body.append(&vol_row);
    flyout.append(&body);

    // ---- footer: battery + all settings
    let footer = gtk::CenterBox::new();
    footer.add_css_class("start-footer");
    let bat_icon = gtk::Image::from_icon_name("battery-full-symbolic");
    let bat_label = gtk::Label::new(None);
    let bat = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bat.append(&bat_icon);
    bat.append(&bat_label);
    bat.set_visible(false);
    footer.set_start_widget(Some(&bat));

    let settings = gtk::Button::new();
    settings.add_css_class("power-button");
    settings.set_icon_name("emblem-system-symbolic");
    settings.set_tooltip_text(Some("All settings"));
    {
        let p = popup.clone();
        settings.connect_clicked(move |_| {
            sys::spawn(sys::SETTINGS_CMD);
            p.hide();
        });
    }
    footer.set_end_widget(Some(&settings));
    flyout.append(&footer);
    popup.window.set_child(Some(&flyout));

    // ---- wiring
    let quick = Rc::new(Quick {
        tiles,
        bright_row,
        bright,
        vol,
        vol_icon,
        bat,
        bat_icon,
        bat_label,
        syncing,
        muted,
        level,
    });
    let _ = hub.set(quick.clone());

    {
        let q = quick.clone();
        popup.window.connect_show(move |_| q.refresh());
    }
    popup.window.connect_hide(|_| mem::trim_soon());
    popup
}
