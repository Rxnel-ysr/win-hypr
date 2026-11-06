//! Notification center + month calendar (Win+N / clicking the clock).
//!
//! The card list is rebuilt every time the flyout opens and dropped when it closes, so a closed
//! flyout holds no per-notification widgets.

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use std::rc::Rc;
use w11_common::config::{self, StoredNotification};
use w11_common::mem;
use w11_common::popup::{Placement, Popup};

/// The history keeps 60 entries; only the newest ones are worth building widgets for.
const MAX_SHOWN: usize = 40;

pub fn build(app: &gtk::Application) -> Rc<Popup> {
    w11_common::load_css!("css/calendar.css");
    let popup = Popup::new(app, "w11-calendar", 380, 700, Placement::BottomRight);

    let flyout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    flyout.add_css_class("flyout");
    flyout.set_size_request(380, 680);

    // ---- header
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

    // ---- notification list
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    list.set_margin_start(16);
    list.set_margin_end(16);
    scroll.set_child(Some(&list));
    flyout.append(&scroll);

    // ---- calendar panel
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

    // ---- wiring
    {
        let (list, title) = (list.clone(), title.clone());
        clear.connect_clicked(move |_| {
            config::save_notifications(&[]);
            fill(&list, &title);
        });
    }
    {
        let (list, title) = (list.clone(), title.clone());
        popup.window.connect_show(move |_| {
            fill(&list, &title);
            if let Ok(now) = glib::DateTime::now_local() {
                cal.select_day(&now);
                if let Ok(s) = now.format("%A, %B %-d") {
                    date_title.set_text(&s);
                }
            }
        });
    }
    popup.window.connect_hide(move |_| {
        clear_children(&list);
        mem::trim_soon();
    });
    popup
}

fn clear_children(list: &gtk::Box) {
    while let Some(c) = list.first_child() {
        list.remove(&c);
    }
}

fn fill(list: &gtk::Box, title: &gtk::Label) {
    clear_children(list);
    title.set_text(if config::dnd_enabled() {
        "Notifications (Do not disturb)"
    } else {
        "Notifications"
    });

    let items = config::load_notifications();
    if items.is_empty() {
        let l = gtk::Label::new(Some("No new notifications"));
        l.add_css_class("notif-app");
        l.set_margin_top(60);
        l.set_margin_bottom(60);
        list.append(&l);
        return;
    }
    for n in items.iter().rev().take(MAX_SHOWN) {
        list.append(&notif_card(n));
    }
}

/// Icon theme name, or an absolute path (scaled while loading, so a huge PNG never sits in RAM).
fn notif_icon(name: &str) -> gtk::Image {
    let img = if name.is_empty() {
        gtk::Image::from_icon_name("dialog-information-symbolic")
    } else if name.starts_with('/') {
        match gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(name, 56, 56, true) {
            Ok(pb) => gtk::Image::from_paintable(Some(&gdk::Texture::for_pixbuf(&pb))),
            Err(_) => gtk::Image::from_icon_name("dialog-information-symbolic"),
        }
    } else {
        gtk::Image::from_icon_name(name)
    };
    img.set_pixel_size(28);
    img.set_valign(gtk::Align::Start);
    img
}

fn notif_card(n: &StoredNotification) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    card.add_css_class("notif-card");

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

    card.append(&notif_icon(&n.icon));
    card.append(&v);
    card
}
