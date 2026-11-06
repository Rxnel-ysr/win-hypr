//! Small reusable widget builders.

use crate::ctx::HideFn;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use w11_common::apps;

pub fn clear_box(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

pub fn clear_flow(f: &gtk::FlowBox) {
    while let Some(c) = f.first_child() {
        f.remove(&c);
    }
}

pub fn section_title(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("section-title");
    l.set_xalign(0.0);
    l
}

/// "Title ............ [button]" header used by Home and All apps.
pub fn header_row(title: &str, button_label: &str) -> (gtk::Box, gtk::Button) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let t = section_title(title);
    t.set_hexpand(true);
    let btn = gtk::Button::with_label(button_label);
    btn.add_css_class("link-button");
    row.append(&t);
    row.append(&btn);
    (row, btn)
}

pub fn pin_tile(info: &gio::AppInfo, hide: &HideFn) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("pin-tile");
    let v = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let img = gtk::Image::from_gicon(&apps::icon_of(info));
    img.set_pixel_size(32);
    let label = gtk::Label::new(Some(&info.name()));
    label.add_css_class("pin-label");
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_max_width_chars(12);
    v.append(&img);
    v.append(&label);
    b.set_child(Some(&v));
    connect_launch(&b, info, hide);
    b
}

pub fn app_row(info: &gio::AppInfo, hide: &HideFn) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("app-row");
    let h = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let img = gtk::Image::from_gicon(&apps::icon_of(info));
    img.set_pixel_size(24);
    let l = gtk::Label::new(Some(&info.name()));
    l.set_xalign(0.0);
    h.append(&img);
    h.append(&l);
    b.set_child(Some(&h));
    connect_launch(&b, info, hide);
    b
}

fn connect_launch(b: &gtk::Button, info: &gio::AppInfo, hide: &HideFn) {
    let (info, hide) = (info.clone(), hide.clone());
    b.connect_clicked(move |_| {
        apps::launch(&info);
        hide();
    });
}

/// One "Recommended" entry. Returns None if the file no longer exists.
pub fn recent_row(uri: String, modified: i64, hide: &HideFn) -> Option<gtk::Button> {
    let file = gio::File::for_uri(&uri);
    let finfo = file
        .query_info(
            "standard::icon",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .ok()?;

    let b = gtk::Button::new();
    b.add_css_class("rec-row");
    let h = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    let img = match finfo.icon() {
        Some(i) => gtk::Image::from_gicon(&i),
        None => gtk::Image::from_icon_name("text-x-generic"),
    };
    img.set_pixel_size(28);

    let v = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let title = gtk::Label::new(Some(
        &file
            .basename()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    ));
    title.add_css_class("rec-title");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(24);

    let when = glib::DateTime::from_unix_local(modified)
        .ok()
        .and_then(|d| d.format("%b %-d, %-I:%M %p").ok())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let sub = gtk::Label::new(Some(&when));
    sub.add_css_class("rec-sub");
    sub.set_xalign(0.0);

    v.append(&title);
    v.append(&sub);
    h.append(&img);
    h.append(&v);
    b.set_child(Some(&h));

    let hide = hide.clone();
    b.connect_clicked(move |_| {
        let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
        hide();
    });
    Some(b)
}
