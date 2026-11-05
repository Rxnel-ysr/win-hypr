//! Windows 11 style Settings app. Pages are built lazily and dropped on navigation
//! to keep RSS low; everything slow runs off the UI thread.

use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::Duration;
use w11_common::config::{self, Config};
use w11_common::{apps, hypr, sys, theme};

type Win = gtk::ApplicationWindow;
type Build = fn(&Win) -> gtk::Box;
pub const CSS: &str = include_str!("css/setting.css");

const PAGES: &[(&str, &str, &str, Build)] = &[
    ("system", "computer-symbolic", "System", page_system),
    (
        "bluetooth",
        "bluetooth-symbolic",
        "Bluetooth & devices",
        page_bluetooth,
    ),
    (
        "network",
        "network-wireless-symbolic",
        "Network & internet",
        page_network,
    ),
    (
        "personalization",
        "preferences-desktop-wallpaper-symbolic",
        "Personalization",
        page_personalization,
    ),
    ("apps", "view-app-grid-symbolic", "Apps", page_apps),
    (
        "accounts",
        "avatar-default-symbolic",
        "Accounts",
        page_accounts,
    ),
    (
        "time",
        "preferences-system-time-symbolic",
        "Time & language",
        page_time,
    ),
    (
        "update",
        "software-update-available-symbolic",
        "System update",
        page_update,
    ),
];

const ACCENTS: [&str; 8] = [
    "#60cdff", "#0078d4", "#8e8cd8", "#c239b3", "#e74856", "#ff8c00", "#10c469", "#9a9a9a",
];

const IDLE_OPTS: [(&str, u32); 10] = [
    ("1 minute", 60),
    ("2 minutes", 120),
    ("3 minutes", 180),
    ("5 minutes", 300),
    ("10 minutes", 600),
    ("15 minutes", 900),
    ("20 minutes", 1200),
    ("30 minutes", 1800),
    ("1 hour", 3600),
    ("Never", 0),
];

// ------------------------------------------------------------------ main

fn main() -> glib::ExitCode {
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "cairo");
    }
    let app = gtk::Application::builder()
        .application_id("dev.win11hypr.Settings")
        .build();
    app.connect_startup(|_| {
        theme::apply();
        if let Some(d) = gdk::Display::default() {
            let p = gtk::CssProvider::new();
            p.load_from_data(CSS);
            gtk::style_context_add_provider_for_display(
                &d,
                &p,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        }
    });
    let start = std::env::args().nth(1); // e.g. `w11-settings network`
    app.connect_activate(move |app| {
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        build(app, start.as_deref());
    });
    let prog = std::env::args().next().unwrap_or_default();
    app.run_with_args(&[prog.as_str()])
}

fn build(app: &gtk::Application, start: Option<&str>) {
    let win = gtk::ApplicationWindow::builder()
        .application(app)
        .default_width(1100)
        .default_height(720)
        .title("Settings")
        .build();
    win.add_css_class("settings");

    // title bar
    let header = gtk::HeaderBar::new();
    header.set_show_title_buttons(false);
    header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    let brand = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    brand.append(&text("Settings", "s-appname"));
    header.pack_start(&brand);
    let caption = |icon: &str, extra: &str| {
        let b = gtk::Button::from_icon_name(icon);
        b.add_css_class("caption-button");
        if !extra.is_empty() {
            b.add_css_class(extra);
        }
        b
    };
    let (b_min, b_max, b_close) = (
        caption("window-minimize-symbolic", ""),
        caption("window-maximize-symbolic", ""),
        caption("window-close-symbolic", "close"),
    );
    header.pack_end(&b_close);
    header.pack_end(&b_max);
    header.pack_end(&b_min);
    win.set_titlebar(Some(&header));
    b_min.connect_clicked(|_| {
        std::thread::spawn(hypr::minimize_active);
    });
    b_max.connect_clicked(|_| {
        std::thread::spawn(hypr::toggle_maximize_active);
    });
    {
        let w = win.clone();
        b_close.connect_clicked(move |_| w.close());
    }

    // sidebar
    let side = gtk::Box::new(gtk::Orientation::Vertical, 12);
    side.add_css_class("s-side");
    side.set_size_request(300, -1);

    let name = sys::user_name();
    let prof = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    prof.set_margin_bottom(6);
    let avatar = gtk::Label::new(Some(
        &name
            .chars()
            .next()
            .unwrap_or('U')
            .to_uppercase()
            .to_string(),
    ));
    avatar.add_css_class("s-avatar");
    let pv = gtk::Box::new(gtk::Orientation::Vertical, 2);
    pv.set_valign(gtk::Align::Center);
    pv.append(&text(&name, "s-profile-name"));
    pv.append(&text("Local account", "s-profile-sub"));
    prof.append(&avatar);
    prof.append(&pv);
    side.append(&prof);

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Find a setting"));
    side.append(&search);

    let nav = gtk::ListBox::new();
    nav.add_css_class("s-nav");
    nav.set_selection_mode(gtk::SelectionMode::Single);
    for (_, icon, title, _) in PAGES {
        let r = gtk::ListBoxRow::new();
        r.set_widget_name(&title.to_lowercase());
        let h = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        h.append(&icon_img(icon, 18));
        h.append(&gtk::Label::new(Some(title)));
        r.set_child(Some(&h));
        nav.append(&r);
    }
    let nav_scroll = gtk::ScrolledWindow::new();
    nav_scroll.set_vexpand(true);
    nav_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    nav_scroll.set_child(Some(&nav));
    side.append(&nav_scroll);

    let q = Rc::new(RefCell::new(String::new()));
    {
        let q_filter = q.clone();
        nav.set_filter_func(move |row| {
            let q = q_filter.borrow();
            q.is_empty() || row.widget_name().to_lowercase().contains(q.as_str())
        });
        let nav = nav.clone();
        search.connect_search_changed(move |s| {
            *q.borrow_mut() = s.text().to_lowercase();
            nav.invalidate_filter();
        });
    }

    // content (one page alive at a time)
    let content = gtk::ScrolledWindow::new();
    content.add_css_class("s-content");
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    {
        let (win, content) = (win.clone(), content.clone());
        nav.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let build = PAGES[row.index() as usize].3;
            content.set_child(Some(&build(&win)));
            content.vadjustment().set_value(0.0);
            // hand freed pages back to the OS
            glib::idle_add_local_once(|| unsafe {
                libc::malloc_trim(0);
            });
        });
    }

    let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    root.append(&side);
    root.append(&content);
    win.set_child(Some(&root));

    let idx = start
        .and_then(|s| PAGES.iter().position(|p| p.0 == s))
        .unwrap_or(0);
    nav.select_row(nav.row_at_index(idx as i32).as_ref());
    win.present();
}

// --------------------------------------------------------------- helpers

fn read(p: impl AsRef<Path>) -> String {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

/// Run `work` on a thread-pool thread, then `done` on the UI thread.
fn bg<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(T) + 'static,
) {
    glib::spawn_future_local(async move {
        if let Ok(v) = gio::spawn_blocking(work).await {
            done(v);
        }
    });
}

fn icon_img(name: &str, px: i32) -> gtk::Image {
    let i = gtk::Image::from_icon_name(name);
    i.set_pixel_size(px);
    i
}

fn text(t: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(t));
    l.add_css_class(class);
    l.set_xalign(0.0);
    l.set_wrap(true);
    l.set_max_width_chars(80);
    l
}

fn page(title: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 4);
    b.set_margin_top(18);
    b.set_margin_bottom(32);
    b.set_margin_start(40);
    b.set_margin_end(40);
    b.append(&text(title, "s-page-title"));
    b
}

fn heading(p: &gtk::Box, t: &str) {
    p.append(&text(t, "s-heading"));
}

fn card() -> gtk::Box {
    let c = gtk::Box::new(gtk::Orientation::Vertical, 0);
    c.add_css_class("s-card");
    c
}

fn row_i(img: Option<gtk::Image>, title: &str, sub: &str) -> (gtk::Box, gtk::Label) {
    let r = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    r.add_css_class("s-row");
    if let Some(i) = img {
        i.set_pixel_size(20);
        r.append(&i);
    }
    let v = gtk::Box::new(gtk::Orientation::Vertical, 2);
    v.set_hexpand(true);
    v.set_valign(gtk::Align::Center);
    v.append(&text(title, "s-title"));
    let s = text(sub, "s-sub");
    s.set_visible(!sub.is_empty());
    v.append(&s);
    r.append(&v);
    (r, s)
}

fn row(icon: &str, title: &str, sub: &str) -> (gtk::Box, gtk::Label) {
    row_i(
        if icon.is_empty() {
            None
        } else {
            Some(icon_img(icon, 20))
        },
        title,
        sub,
    )
}

fn add_ctl(r: &gtk::Box, c: &impl IsA<gtk::Widget>) {
    c.set_valign(gtk::Align::Center);
    r.append(c);
}

fn button(label: &str, accent: bool) -> gtk::Button {
    let b = gtk::Button::with_label(label);
    b.add_css_class("s-btn");
    if accent {
        b.add_css_class("accent");
    }
    b
}

fn switch(on: bool, f: impl Fn(bool) + 'static) -> gtk::Switch {
    let s = gtk::Switch::new();
    s.set_active(on);
    s.connect_state_set(move |_, st| {
        f(st);
        glib::Propagation::Proceed
    });
    s
}

fn slider(value: f64, f: impl Fn(f64) + 'static) -> gtk::Scale {
    let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    s.set_draw_value(false);
    s.set_value(value);
    s.set_width_request(240);
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let f = Rc::new(f);
    s.connect_value_changed(move |s| {
        if let Some(id) = pending.borrow_mut().take() {
            id.remove();
        }
        let v = s.value();
        let (p, f) = (pending.clone(), f.clone());
        let id = glib::timeout_add_local_once(Duration::from_millis(60), move || {
            p.borrow_mut().take();
            f(v);
        });
        *pending.borrow_mut() = Some(id);
    });
    s
}

fn dropdown(items: &[&str], sel: u32, f: impl Fn(u32) + 'static) -> gtk::DropDown {
    let d = gtk::DropDown::from_strings(items);
    d.set_selected(sel);
    d.connect_selected_notify(move |d| f(d.selected()));
    d
}

fn open_file(path: &Path) {
    sys::spawn(&format!("xdg-open {}", shq(&path.to_string_lossy())));
}

fn restart_taskbar() {
    sys::spawn("pkill -x w11-taskbar; setsid -f w11-taskbar");
}

// ----------------------------------------------------------------- System

fn cpu_model() -> String {
    read("/proc/cpuinfo")
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn meminfo_gb(key: &str) -> f64 {
    read("/proc/meminfo")
        .lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
        / 1_048_576.0
}

fn os_name() -> String {
    read("/etc/os-release")
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_else(|| "Linux".into())
}

fn idle_path() -> PathBuf {
    glib::user_config_dir().join("hypr/hypridle.conf")
}

/// [lock, screen off, sleep] in seconds; 0 = never.
fn idle_read() -> [u32; 3] {
    let Ok(s) = std::fs::read_to_string(idle_path()) else {
        return [300, 360, 1200];
    };
    let mut out = [0u32; 3];
    let mut last = 0u32;
    for l in s.lines().map(str::trim) {
        if let Some(v) = l.strip_prefix("timeout") {
            last = v.trim_start_matches([' ', '=']).trim().parse().unwrap_or(0);
        } else if l.starts_with("on-timeout") {
            if l.contains("lock-session") {
                out[0] = last;
            } else if l.contains("disable") {
                out[1] = last;
            } else if l.contains("suspend") {
                out[2] = last;
            }
        }
    }
    out
}

fn idle_write(t: [u32; 3]) {
    let mut s = String::from(
        "# ~/.config/hypr/hypridle.conf - written by w11-settings\n\
         general {\n    lock_cmd = pidof hyprlock || hyprlock\n    before_sleep_cmd = loginctl lock-session\n    \
         after_sleep_cmd = hyprctl dispatch 'hl.dsp.dpms({ action = \"enable\" })'\n}\n",
    );
    if t[0] > 0 {
        s += &format!(
            "\nlistener {{\n    timeout = {}\n    on-timeout = loginctl lock-session\n}}\n",
            t[0]
        );
    }
    if t[1] > 0 {
        s += &format!(
            "\nlistener {{\n    timeout = {}\n    on-timeout = hyprctl dispatch 'hl.dsp.dpms({{ action = \"disable\" }})'\n    \
             on-resume = hyprctl dispatch 'hl.dsp.dpms({{ action = \"enable\" }})'\n}}\n",
            t[1]
        );
    }
    if t[2] > 0 {
        s += &format!(
            "\nlistener {{\n    timeout = {}\n    on-timeout = systemctl suspend\n}}\n",
            t[2]
        );
    }
    let _ = std::fs::write(idle_path(), s);
    sys::spawn("pkill -x hypridle; setsid -f hypridle");
}

fn idle_index(secs: u32) -> u32 {
    if secs == 0 {
        return 9;
    }
    IDLE_OPTS[..9]
        .iter()
        .enumerate()
        .min_by_key(|(_, o)| (o.1 as i64 - secs as i64).abs())
        .map(|(i, _)| i as u32)
        .unwrap_or(9)
}

fn page_system(win: &Win) -> gtk::Box {
    let p = page("System");

    let host = read("/etc/hostname");
    let c = card();
    c.append(&row("computer-symbolic", &host, &cpu_model()).0);
    p.append(&c);

    // ---- Display
    heading(&p, "Display");
    let c = card();
    if let Some(b) = sys::brightness() {
        let (r, _) = row(
            "display-brightness-symbolic",
            "Brightness",
            "Built-in display",
        );
        add_ctl(&r, &slider(b, sys::set_brightness));
        c.append(&r);
    }
    let (r, _) = row(
        "night-light-symbolic",
        "Night light",
        "Warmer colours at night (hyprsunset)",
    );
    add_ctl(&r, &switch(sys::night_light(), sys::set_night_light));
    c.append(&r);
    for m in hypr::monitors() {
        let sub = format!(
            "{}×{} · scale {}%",
            m.width,
            m.height,
            (m.scale * 100.0).round()
        );
        c.append(&row("video-display-symbolic", &m.name, &sub).0);
    }
    let (r, _) = row(
        "",
        "Multiple displays",
        "Arrange or rescale displays in modules/monitors.lua",
    );
    let b = button("Edit file", false);
    b.connect_clicked(|_| open_file(&glib::user_config_dir().join("hypr/modules/monitors.lua")));
    add_ctl(&r, &b);
    c.append(&r);
    p.append(&c);

    // ---- Sound
    heading(&p, "Sound");
    let c = card();
    let (v, muted) = sys::volume();
    let (r, _) = row("audio-volume-high-symbolic", "Volume", "");
    add_ctl(&r, &slider(v * 100.0, |x| sys::set_volume(x / 100.0)));
    c.append(&r);
    let (r, _) = row("audio-volume-muted-symbolic", "Mute", "");
    add_ctl(
        &r,
        &switch(muted, |on| {
            sys::spawn(&format!(
                "wpctl set-mute @DEFAULT_AUDIO_SINK@ {}",
                if on { 1 } else { 0 }
            ))
        }),
    );
    c.append(&r);
    let out = sys::sh(
        r#"wpctl inspect @DEFAULT_AUDIO_SINK@ | sed -n 's/.*node.description = "\(.*\)"/\1/p' | head -1"#,
    );
    let (r, _) = row(
        "audio-speakers-symbolic",
        "Output device",
        if out.is_empty() { "Unknown" } else { &out },
    );
    let b = button("More sound settings", false);
    b.connect_clicked(|_| sys::spawn("pavucontrol"));
    add_ctl(&r, &b);
    c.append(&r);
    p.append(&c);

    // ---- Power & battery
    heading(&p, "Power & battery");
    let c = card();
    if let Some(b) = sys::battery() {
        let sub = format!(
            "{}%{}",
            b.percent,
            if b.charging { " · Charging" } else { "" }
        );
        c.append(&row(&sys::battery_icon(b), "Battery", &sub).0);
    }
    let mode = sys::run("powerprofilesctl", &["get"]);
    if !mode.is_empty() {
        const MODES: [&str; 3] = ["power-saver", "balanced", "performance"];
        let (r, _) = row("", "Power mode", "Optimise for battery life or speed");
        let sel = MODES.iter().position(|m| *m == mode).unwrap_or(1) as u32;
        let d = dropdown(
            &["Best power efficiency", "Balanced", "Best performance"],
            sel,
            |i| {
                sys::spawn(&format!("powerprofilesctl set {}", MODES[i as usize]));
            },
        );
        add_ctl(&r, &d);
        c.append(&r);
    }
    let times = Rc::new(Cell::new(idle_read()));
    let names: Vec<&str> = IDLE_OPTS.iter().map(|o| o.0).collect();
    for (k, (title, sub)) in [
        ("Lock the screen after", "Requires a password to continue"),
        ("Turn off my screen after", "When idle"),
        ("Put my device to sleep after", "When idle"),
    ]
    .iter()
    .enumerate()
    {
        let (r, _) = row("", title, sub);
        let t = times.clone();
        let d = dropdown(&names, idle_index(times.get()[k]), move |i| {
            let mut a = t.get();
            a[k] = IDLE_OPTS[i as usize].1;
            t.set(a);
            idle_write(a);
        });
        add_ctl(&r, &d);
        c.append(&r);
    }
    p.append(&c);

    // ---- Notifications
    heading(&p, "Notifications");
    let c = card();
    let (r, _) = row(
        "notifications-disabled-symbolic",
        "Do not disturb",
        "Toasts are hidden; they still land in the notification centre",
    );
    add_ctl(&r, &switch(config::dnd_enabled(), config::set_dnd));
    c.append(&r);
    p.append(&c);

    // ---- Storage
    heading(&p, "Storage");
    let c = card();
    let (r, sub) = row("drive-harddisk-symbolic", "Local Disk (/)", "Calculating…");
    let bar = gtk::ProgressBar::new();
    bar.set_width_request(220);
    add_ctl(&r, &bar);
    c.append(&r);
    p.append(&c);
    bg(
        || {
            let o = sys::run("df", &["-B1", "--output=size,used", "/"]);
            let l = o.lines().nth(1).unwrap_or("").to_string();
            let mut it = l.split_whitespace().filter_map(|x| x.parse::<f64>().ok());
            (it.next().unwrap_or(0.0), it.next().unwrap_or(0.0))
        },
        move |(size, used)| {
            if size > 0.0 {
                bar.set_fraction(used / size);
                sub.set_text(&format!(
                    "{:.1} GB used of {:.1} GB",
                    used / 1e9,
                    size / 1e9
                ));
            }
        },
    );

    // ---- About
    heading(&p, "About");
    let total = meminfo_gb("MemTotal:");
    let used = total - meminfo_gb("MemAvailable:");
    let info = vec![
        ("Device name", host.clone()),
        ("Processor", cpu_model()),
        (
            "Installed RAM",
            format!("{:.1} GB ({:.1} GB in use)", total, used),
        ),
        ("Operating system", os_name()),
        ("Kernel", sys::run("uname", &["-r"])),
        ("Compositor", sys::sh("hyprctl version | head -1")),
    ];
    let c = card();
    let (r, _) = row("", "Device specifications", "");
    let copy = button("Copy", false);
    {
        let win = win.clone();
        let blob = info
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        copy.connect_clicked(move |_| win.clipboard().set_text(&blob));
    }
    add_ctl(&r, &copy);
    c.append(&r);
    for (k, v) in &info {
        c.append(&row("", k, v).0);
    }
    p.append(&c);
    p
}

// -------------------------------------------------------------- Bluetooth

fn page_bluetooth(_win: &Win) -> gtk::Box {
    let p = page("Bluetooth & devices");
    let c = card();
    let (r, _) = row("bluetooth-symbolic", "Bluetooth", "");
    add_ctl(&r, &switch(sys::bluetooth_enabled(), sys::set_bluetooth));
    c.append(&r);
    let (r, _) = row(
        "list-add-symbolic",
        "Add device",
        "Pair a new Bluetooth device",
    );
    let b = button("Add device", true);
    b.connect_clicked(|_| {
        sys::spawn(&format!(
            "blueman-manager || {} -e bluetoothctl",
            sys::TERMINAL
        ))
    });
    add_ctl(&r, &b);
    c.append(&r);
    p.append(&c);

    heading(&p, "Devices");
    let list = card();
    p.append(&list);
    fill_bt(&list);
    p
}

fn fill_bt(list: &gtk::Box) {
    clear(list);
    list.append(&row("", "Loading devices…", "").0);
    let list = list.clone();
    bg(
        || {
            sys::run("bluetoothctl", &["devices"])
                .lines()
                .filter_map(|l| {
                    let mut it = l.splitn(3, ' ');
                    if it.next()? != "Device" {
                        return None;
                    }
                    let mac = it.next()?.to_string();
                    let name = it.next().unwrap_or(&mac).to_string();
                    let on = sys::run("bluetoothctl", &["info", &mac]).contains("Connected: yes");
                    Some((mac, name, on))
                })
                .collect::<Vec<_>>()
        },
        move |devs| {
            clear(&list);
            if devs.is_empty() {
                list.append(&row("", "No paired devices", "").0);
            }
            for (mac, name, on) in devs {
                let (r, _) = row(
                    "bluetooth-symbolic",
                    &name,
                    if on { "Connected" } else { "Paired" },
                );
                let b = button(if on { "Disconnect" } else { "Connect" }, !on);
                let l2 = list.clone();
                b.connect_clicked(move |b| {
                    b.set_sensitive(false);
                    sys::spawn(&format!(
                        "bluetoothctl {} {}",
                        if on { "disconnect" } else { "connect" },
                        mac
                    ));
                    let l3 = l2.clone();
                    glib::timeout_add_local_once(Duration::from_millis(2500), move || fill_bt(&l3));
                });
                add_ctl(&r, &b);
                list.append(&r);
            }
        },
    );
}

// ---------------------------------------------------------------- Network

fn page_network(win: &Win) -> gtk::Box {
    let p = page("Network & internet");
    let c = card();
    let (r, _) = row("network-wireless-symbolic", "Wi-Fi", "");
    add_ctl(&r, &switch(sys::wifi_enabled(), sys::set_wifi));
    c.append(&r);
    if sys::has_ethernet() {
        c.append(&row("network-wired-symbolic", "Ethernet", "Connected").0);
    }
    let (r, _) = row("", "Advanced network settings", "Profiles, VPN, static IP");
    let b = button("Open", false);
    b.connect_clicked(|_| sys::spawn("nm-connection-editor"));
    add_ctl(&r, &b);
    c.append(&r);
    p.append(&c);

    heading(&p, "Available networks");
    let list = card();
    let top = card();
    let (r, _) = row("view-refresh-symbolic", "Wi-Fi networks", "");
    let rescan = button("Rescan", false);
    {
        let (win, list) = (win.clone(), list.clone());
        rescan.connect_clicked(move |_| {
            sys::spawn("nmcli dev wifi rescan");
            let (w, l) = (win.clone(), list.clone());
            glib::timeout_add_local_once(Duration::from_millis(2500), move || fill_wifi(&w, &l));
        });
    }
    add_ctl(&r, &rescan);
    top.append(&r);
    p.append(&top);
    p.append(&list);
    fill_wifi(win, &list);
    p
}

fn parse_wifi(out: &str) -> Vec<(bool, String, u32, bool)> {
    let mut v: Vec<(bool, String, u32, bool)> = Vec::new();
    for l in out.lines() {
        let mut it = l.rsplitn(3, ':');
        let sec = it.next().unwrap_or("");
        let sig = it.next().unwrap_or("0");
        let rest = it.next().unwrap_or("");
        let (inuse, ssid) = rest.split_once(':').unwrap_or(("", rest));
        let ssid = ssid.replace("\\:", ":");
        if ssid.is_empty() {
            continue;
        }
        v.push((
            inuse == "*",
            ssid,
            sig.parse().unwrap_or(0),
            !sec.is_empty() && sec != "--",
        ));
    }
    v.sort_by(|a, b| b.0.cmp(&a.0).then(b.2.cmp(&a.2)));
    let mut seen: Vec<String> = Vec::new();
    v.retain(|x| {
        if seen.contains(&x.1) {
            false
        } else {
            seen.push(x.1.clone());
            true
        }
    });
    v.truncate(15);
    v
}

fn fill_wifi(win: &Win, list: &gtk::Box) {
    clear(list);
    list.append(&row("", "Searching…", "").0);
    let (win, list) = (win.clone(), list.clone());
    bg(
        || {
            sys::run(
                "nmcli",
                &[
                    "-t",
                    "-f",
                    "IN-USE,SSID,SIGNAL,SECURITY",
                    "dev",
                    "wifi",
                    "list",
                    "--rescan",
                    "no",
                ],
            )
        },
        move |out| {
            clear(&list);
            let nets = parse_wifi(&out);
            if nets.is_empty() {
                list.append(&row("", "No networks found", "Is Wi-Fi turned on?").0);
            }
            for (inuse, ssid, sig, secured) in nets {
                let lvl = match sig {
                    80..=100 => "excellent",
                    55..=79 => "good",
                    30..=54 => "ok",
                    _ => "weak",
                };
                let sub = if inuse {
                    "Connected"
                } else if secured {
                    "Secured"
                } else {
                    "Open"
                };
                let (r, _) = row(
                    &format!("network-wireless-signal-{lvl}-symbolic"),
                    &ssid,
                    sub,
                );
                let b = button(if inuse { "Disconnect" } else { "Connect" }, !inuse);
                let (w, l, s) = (win.clone(), list.clone(), ssid.clone());
                b.connect_clicked(move |b| {
                    b.set_sensitive(false);
                    if inuse {
                        let (w, l, s) = (w.clone(), l.clone(), s.clone());
                        bg(
                            move || {
                                let _ = Command::new("nmcli")
                                    .args(["connection", "down", "id", &s])
                                    .stdout(Stdio::null())
                                    .stderr(Stdio::null())
                                    .status();
                            },
                            move |_| fill_wifi(&w, &l),
                        );
                    } else {
                        connect_wifi(&w, &l, s.clone(), None, secured);
                    }
                });
                add_ctl(&r, &b);
                list.append(&r);
            }
        },
    );
}

fn connect_wifi(win: &Win, list: &gtk::Box, ssid: String, pass: Option<String>, secured: bool) {
    let (w, l, s) = (win.clone(), list.clone(), ssid.clone());
    let had_pass = pass.is_some();
    bg(
        move || {
            let mut c = Command::new("nmcli");
            c.args(["-w", "20", "dev", "wifi", "connect", &ssid]);
            if let Some(p) = &pass {
                c.args(["password", p]);
            }
            c.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        },
        move |ok| {
            if !ok && secured && !had_pass {
                let (w2, l2, s2) = (w.clone(), l.clone(), s.clone());
                ask_password(&w, &s, move |pw| {
                    connect_wifi(&w2, &l2, s2.clone(), Some(pw), true)
                });
            }
            fill_wifi(&w, &l);
        },
    );
}

fn ask_password(win: &Win, ssid: &str, ok: impl Fn(String) + 'static) {
    let dlg = gtk::Window::builder()
        .transient_for(win)
        .modal(true)
        .title("Connect to Wi-Fi")
        .default_width(380)
        .resizable(false)
        .build();
    dlg.add_css_class("settings");
    let v = gtk::Box::new(gtk::Orientation::Vertical, 12);
    for f in [
        gtk::Widget::set_margin_top,
        gtk::Widget::set_margin_bottom,
        gtk::Widget::set_margin_start,
        gtk::Widget::set_margin_end,
    ] {
        f(v.upcast_ref(), 18);
    }
    v.append(&text(
        &format!("Enter the network security key for {ssid}"),
        "s-title",
    ));
    let entry = gtk::PasswordEntry::new();
    entry.set_show_peek_icon(true);
    v.append(&entry);
    let row_b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row_b.set_halign(gtk::Align::End);
    let cancel = button("Cancel", false);
    let go_btn = button("Connect", true);
    row_b.append(&cancel);
    row_b.append(&go_btn);
    v.append(&row_b);
    dlg.set_child(Some(&v));

    let ok = Rc::new(ok);
    let go = {
        let (d, e) = (dlg.clone(), entry.clone());
        Rc::new(move || {
            let t = e.text().to_string();
            d.close();
            if !t.is_empty() {
                ok(t);
            }
        })
    };
    let g = go.clone();
    go_btn.connect_clicked(move |_| g());
    let g = go.clone();
    entry.connect_activate(move |_| g());
    let d = dlg.clone();
    cancel.connect_clicked(move |_| d.close());
    dlg.present();
    entry.grab_focus();
}

// -------------------------------------------------------- Personalization

thread_local! {
    static ACCENT: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
}

const ACC_BEGIN: &str = "/* w11-settings:accent */";
const ACC_END: &str = "/* w11-settings:end */";

fn current_accent() -> String {
    let css = read(config::config_dir().join("user.css"));
    css.find("@define-color accent ")
        .and_then(|i| css[i + 21..].split(';').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| ACCENTS[0].into())
}

fn set_accent(hex: &str) {
    let path = config::config_dir().join("user.css");
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let mut keep = old.clone();
    if let (Some(i), Some(j)) = (old.find(ACC_BEGIN), old.find(ACC_END)) {
        if j > i {
            keep = format!("{}{}", &old[..i], &old[j + ACC_END.len()..]);
        }
    }
    let css = format!(
        "{}\n{ACC_BEGIN}\n@define-color accent {hex};\n{ACC_END}\n",
        keep.trim_end()
    );
    let _ = std::fs::write(&path, css);

    // live-apply to this window
    if let Some(d) = gdk::Display::default() {
        ACCENT.with(|a| {
            if let Some(old) = a.borrow_mut().take() {
                gtk::style_context_remove_provider_for_display(&d, &old);
            }
            let p = gtk::CssProvider::new();
            p.load_from_data(&format!("@define-color accent {hex};"));
            gtk::style_context_add_provider_for_display(
                &d,
                &p,
                gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
            );
            *a.borrow_mut() = Some(p);
        });
    }
}

fn thumb(p: &Path) -> Option<gdk::Texture> {
    let pb = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(p, 360, 202, true).ok()?;
    Some(gdk::Texture::for_pixbuf(&pb))
}

fn apply_wallpaper(path: &Path) {
    let p = path.to_string_lossy().to_string();
    let _ = std::fs::write(config::config_dir().join("wallpaper.txt"), p.as_bytes());
    sys::spawn(&format!(
        "old=$(pgrep -x swaybg); pkill -x wpaperd; setsid -f swaybg -m fill -i {}; sleep 0.4; [ -z \"$old\" ] || kill $old",
        shq(&p)
    ));
}

fn page_personalization(win: &Win) -> gtk::Box {
    let p = page("Personalization");

    // ---- Background
    heading(&p, "Background");
    let c = card();
    let saved = read(config::config_dir().join("wallpaper.txt"));
    let cur = if saved.is_empty() {
        glib::home_dir().join("Pictures/Wallpapers/bloom.jpg")
    } else {
        PathBuf::from(saved)
    };
    let (r, sub) = row(
        "preferences-desktop-wallpaper-symbolic",
        "Choose a photo",
        &cur.display().to_string(),
    );
    let browse = button("Browse photos", false);
    add_ctl(&r, &browse);
    c.append(&r);
    let pic = gtk::Picture::new();
    pic.set_halign(gtk::Align::Start);
    if let Some(t) = thumb(&cur) {
        pic.set_paintable(Some(&t));
    }
    let pr = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    pr.add_css_class("s-row");
    pr.append(&pic);
    c.append(&pr);
    {
        let win = win.clone();
        browse.connect_clicked(move |_| {
            let dlg = gtk::FileDialog::new();
            dlg.set_title("Choose a photo");
            let f = gtk::FileFilter::new();
            f.add_mime_type("image/*");
            f.set_name(Some("Images"));
            let store = gio::ListStore::new::<gtk::FileFilter>();
            store.append(&f);
            dlg.set_filters(Some(&store));
            let (pic, sub) = (pic.clone(), sub.clone());
            dlg.open(Some(&win), gio::Cancellable::NONE, move |res| {
                if let Some(path) = res.ok().and_then(|f| f.path()) {
                    apply_wallpaper(&path);
                    if let Some(t) = thumb(&path) {
                        pic.set_paintable(Some(&t));
                    }
                    sub.set_text(&path.display().to_string());
                }
            });
        });
    }
    p.append(&c);

    // ---- Colours
    heading(&p, "Colours");
    let c = card();
    let (r, _) = row(
        "applications-graphics-symbolic",
        "Accent colour",
        "Applies to this app now and to the shell after a taskbar restart",
    );
    let sw = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let now = current_accent();
    let btns: Rc<Vec<gtk::Button>> = Rc::new(
        ACCENTS
            .iter()
            .enumerate()
            .map(|(i, hex)| {
                let b = gtk::Button::new();
                b.add_css_class("swatch");
                b.add_css_class(&format!("sw{i}"));
                if hex.eq_ignore_ascii_case(&now) {
                    b.add_css_class("on");
                }
                sw.append(&b);
                b
            })
            .collect(),
    );
    for (i, b) in btns.iter().enumerate() {
        let btns = btns.clone();
        b.connect_clicked(move |_| {
            set_accent(ACCENTS[i]);
            for (j, o) in btns.iter().enumerate() {
                if i == j {
                    o.add_css_class("on");
                } else {
                    o.remove_css_class("on");
                }
            }
        });
    }
    add_ctl(&r, &sw);
    c.append(&r);
    p.append(&c);

    // ---- Taskbar
    heading(&p, "Taskbar");
    let cfg = Rc::new(RefCell::new(Config::load()));
    let c = card();
    let (r, _) = row(
        "weather-few-clouds-symbolic",
        "Weather widget",
        "Shows the temperature on the left of the taskbar",
    );
    {
        let weather_on = cfg.borrow().weather;
        let cfg = cfg.clone();
        add_ctl(
            &r,
            &switch(weather_on, move |on| {
                let mut c = cfg.borrow_mut();
                c.weather = on;
                c.save();
            }),
        );
    }
    c.append(&r);
    let (r, _) = row("", "Weather location", "City name, empty = automatic");
    let e = gtk::Entry::new();
    e.set_width_request(200);
    e.set_text(&cfg.borrow().weather_location);
    {
        let cfg = cfg.clone();
        e.connect_activate(move |e| {
            let mut c = cfg.borrow_mut();
            c.weather_location = e.text().to_string();
            c.save();
        });
    }
    add_ctl(&r, &e);
    c.append(&r);
    let (r, _) = row(
        "view-refresh-symbolic",
        "Apply taskbar changes",
        "Restarts the taskbar",
    );
    let b = button("Restart taskbar", true);
    b.connect_clicked(|_| restart_taskbar());
    add_ctl(&r, &b);
    c.append(&r);
    p.append(&c);

    heading(&p, "Pinned apps");
    let pins = card();
    let ids = cfg.borrow().pinned.clone();
    for id in ids {
        let Some(info) = apps::by_id(&id) else {
            continue;
        };
        let (r, _) = row_i(
            Some(gtk::Image::from_gicon(&apps::icon_of(&info))),
            &info.name(),
            "",
        );
        let b = button("Unpin", false);
        let (cfg, pins2, r2) = (cfg.clone(), pins.clone(), r.clone());
        b.connect_clicked(move |_| {
            let mut c = cfg.borrow_mut();
            c.pinned.retain(|x| *x != id);
            c.save();
            pins2.remove(&r2);
        });
        add_ctl(&r, &b);
        pins.append(&r);
    }
    p.append(&pins);
    p
}

// ------------------------------------------------------------------- Apps

fn page_apps(_win: &Win) -> gtk::Box {
    let p = page("Apps");

    heading(&p, "Default apps");
    let c = card();
    for (label, icon, mime) in [
        (
            "Web browser",
            "web-browser-symbolic",
            "x-scheme-handler/http",
        ),
        (
            "File manager",
            "system-file-manager-symbolic",
            "inode/directory",
        ),
        ("Image viewer", "image-x-generic-symbolic", "image/png"),
        ("Video player", "video-x-generic-symbolic", "video/mp4"),
        ("Music player", "audio-x-generic-symbolic", "audio/mpeg"),
        ("PDF viewer", "application-pdf-symbolic", "application/pdf"),
        ("Text editor", "text-x-generic-symbolic", "text/plain"),
    ] {
        let list = gio::AppInfo::all_for_type(mime);
        if list.is_empty() {
            continue;
        }
        let cur = gio::AppInfo::default_for_type(mime, false)
            .and_then(|a| a.id())
            .map(|s| s.to_string());
        let names: Vec<String> = list.iter().map(|a| a.name().to_string()).collect();
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let sel = list
            .iter()
            .position(|a| a.id().map(|s| s.to_string()) == cur)
            .unwrap_or(0) as u32;
        let (r, _) = row(icon, label, "");
        let d = dropdown(&refs, sel, move |i| {
            if let Some(a) = list.get(i as usize) {
                let _ = a.set_as_default_for_type(mime);
            }
        });
        add_ctl(&r, &d);
        c.append(&r);
    }
    p.append(&c);

    heading(&p, "Installed apps");
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search apps"));
    search.set_margin_bottom(8);
    p.append(&search);
    let all = Rc::new(apps::all());
    let list = card();
    let fill = {
        let (all, list) = (all.clone(), list.clone());
        Rc::new(move |q: &str| {
            clear(&list);
            let q = q.to_lowercase();
            for a in all
                .iter()
                .filter(|a| q.is_empty() || a.name().to_lowercase().contains(&q))
                .take(40)
            {
                let (r, _) = row_i(
                    Some(gtk::Image::from_gicon(&apps::icon_of(a))),
                    &a.name(),
                    "",
                );
                let b = button("Open", false);
                let a = a.clone();
                b.connect_clicked(move |_| apps::launch(&a));
                add_ctl(&r, &b);
                list.append(&r);
            }
        })
    };
    fill("");
    {
        let fill = fill.clone();
        search.connect_search_changed(move |s| fill(&s.text()));
    }
    p.append(&list);
    p
}

// --------------------------------------------------------------- Accounts

fn page_accounts(win: &Win) -> gtk::Box {
    let p = page("Accounts");
    let name = sys::user_name();
    let user = std::env::var("USER").unwrap_or_default();

    let c = card();
    let (r, _) = row_i(None, &name, &format!("Local account · {user}"));
    let av = gtk::Label::new(Some(
        &name
            .chars()
            .next()
            .unwrap_or('U')
            .to_uppercase()
            .to_string(),
    ));
    av.add_css_class("s-avatar");
    r.prepend(&av);
    c.append(&r);
    p.append(&c);

    heading(&p, "Your info");
    let c = card();
    let (r, sub) = row(
        "avatar-default-symbolic",
        "Profile picture",
        "Shown on the lock screen (~/.face)",
    );
    let b = button("Browse files", false);
    {
        let win = win.clone();
        b.connect_clicked(move |_| {
            let dlg = gtk::FileDialog::new();
            dlg.set_title("Choose a picture");
            let f = gtk::FileFilter::new();
            f.add_mime_type("image/*");
            let store = gio::ListStore::new::<gtk::FileFilter>();
            store.append(&f);
            dlg.set_filters(Some(&store));
            let sub = sub.clone();
            dlg.open(Some(&win), gio::Cancellable::NONE, move |res| {
                if let Some(path) = res.ok().and_then(|f| f.path()) {
                    match std::fs::copy(&path, glib::home_dir().join(".face")) {
                        Ok(_) => sub.set_text("Updated. Visible on the next lock."),
                        Err(e) => sub.set_text(&format!("Couldn't save: {e}")),
                    }
                }
            });
        });
    }
    add_ctl(&r, &b);
    c.append(&r);
    let (r, _) = row(
        "system-lock-screen-symbolic",
        "Password",
        "Change your account password in a terminal",
    );
    let b = button("Change password", false);
    b.connect_clicked(|_| sys::spawn(&format!("{} -e passwd", sys::TERMINAL)));
    add_ctl(&r, &b);
    c.append(&r);
    p.append(&c);

    heading(&p, "Session");
    let c = card();
    let (r, _) = row("system-lock-screen-symbolic", "Lock", "");
    let b = button("Lock now", false);
    b.connect_clicked(|_| sys::lock());
    add_ctl(&r, &b);
    c.append(&r);
    let (r, _) = row("system-log-out-symbolic", "Sign out", "");
    let b = button("Sign out", false);
    b.connect_clicked(|_| sys::sign_out());
    add_ctl(&r, &b);
    c.append(&r);
    p.append(&c);
    p
}

// -------------------------------------------------------- Time & language

fn page_time(_win: &Win) -> gtk::Box {
    let p = page("Time & language");
    heading(&p, "Date & time");
    let c = card();
    let now = glib::DateTime::now_local()
        .ok()
        .and_then(|d| d.format("%-I:%M %p · %A, %B %-d, %Y").ok())
        .map(|s| s.to_string())
        .unwrap_or_default();
    c.append(
        &row(
            "preferences-system-time-symbolic",
            "Current date and time",
            &now,
        )
        .0,
    );

    let ntp = sys::run("timedatectl", &["show", "-p", "NTP", "--value"]) == "yes";
    let (r, _) = row(
        "",
        "Set time automatically",
        "Sync with network time servers",
    );
    add_ctl(
        &r,
        &switch(ntp, |on| {
            sys::spawn(&format!(
                "timedatectl set-ntp {}",
                if on { "true" } else { "false" }
            ))
        }),
    );
    c.append(&r);

    let (r, sub) = row("", "Time zone", "Loading…");
    c.append(&r);
    bg(
        || {
            (
                sys::run("timedatectl", &["show", "-p", "Timezone", "--value"]),
                sys::run("timedatectl", &["list-timezones"]),
            )
        },
        move |(cur, list)| {
            let zones: Vec<String> = list.lines().map(String::from).collect();
            if zones.is_empty() {
                sub.set_text("Unavailable");
                return;
            }
            let pos = zones.iter().position(|z| *z == cur).unwrap_or(0) as u32;
            sub.set_text(&cur);
            let d = {
                let refs: Vec<&str> = zones.iter().map(|s| s.as_str()).collect();
                let zs = zones.clone();
                dropdown(&refs, pos, move |i| {
                    if let Some(z) = zs.get(i as usize).cloned() {
                        bg(
                            move || {
                                sys::run("timedatectl", &["set-timezone", &z]);
                            },
                            |_| restart_taskbar(),
                        );
                    }
                })
            };
            d.set_enable_search(true);
            let expr = gtk::PropertyExpression::new(
                gtk::StringObject::static_type(),
                None::<gtk::Expression>,
                "string",
            );
            d.set_expression(Some(&expr));
            add_ctl(&r, &d);
        },
    );
    p.append(&c);

    heading(&p, "Language & region");
    let c = card();
    let lang = std::env::var("LANG").unwrap_or_else(|_| "unknown".into());
    c.append(&row("preferences-desktop-locale-symbolic", "Language", &lang).0);
    let kb = read(glib::user_config_dir().join("hypr/modules/input.lua"))
        .lines()
        .find(|l| l.contains("kb_layout"))
        .and_then(|l| l.split('"').nth(1))
        .unwrap_or("us")
        .to_string();
    let (r, _) = row("input-keyboard-symbolic", "Keyboard layout", &kb);
    let b = button("Edit file", false);
    b.connect_clicked(|_| open_file(&glib::user_config_dir().join("hypr/modules/input.lua")));
    add_ctl(&r, &b);
    c.append(&r);
    p.append(&c);
    p
}

// ----------------------------------------------------------------- Update

fn page_update(_win: &Win) -> gtk::Box {
    let p = page("System update");
    let c = card();
    let (r, sub) = row(
        "software-update-available-symbolic",
        "Arch Linux updates",
        "Press “Check for updates”",
    );
    let check = button("Check for updates", true);
    add_ctl(&r, &check);
    c.append(&r);
    let (r, _) = row("", "Install updates", "Runs pacman -Syu in a terminal");
    let inst = button("Download & install", false);
    inst.connect_clicked(|_| {
        sys::spawn(&format!(
            "{} -e bash -c 'sudo pacman -Syu; echo; read -rp \"Press Enter to close\"'",
            sys::TERMINAL
        ))
    });
    add_ctl(&r, &inst);
    c.append(&r);
    p.append(&c);

    let pk = card();
    pk.set_visible(false);
    p.append(&pk);

    check.connect_clicked(move |b| {
        b.set_sensitive(false);
        sub.set_text("Checking for updates…");
        let (b, sub, pk) = (b.clone(), sub.clone(), pk.clone());
        bg(
            || match Command::new("checkupdates").output() {
                Ok(o) => Some((
                    o.status.code().unwrap_or(1),
                    String::from_utf8_lossy(&o.stdout).to_string(),
                )),
                Err(_) => None,
            },
            move |res| {
                b.set_sensitive(true);
                clear(&pk);
                pk.set_visible(false);
                match res {
                    None => {
                        sub.set_text("Install pacman-contrib to check for updates (checkupdates).")
                    }
                    Some((2, _)) => sub.set_text("You're up to date"),
                    Some((0, out)) => {
                        sub.set_text(&format!("{} updates available", out.lines().count()));
                        for l in out.lines().take(15) {
                            let mut it = l.split_whitespace();
                            let name = it.next().unwrap_or("");
                            let ver = it.collect::<Vec<_>>().join(" ");
                            pk.append(&row("package-x-generic-symbolic", name, &ver).0);
                        }
                        pk.set_visible(true);
                    }
                    Some(_) => sub.set_text("Couldn't check for updates"),
                }
            },
        );
    });
    p
}
