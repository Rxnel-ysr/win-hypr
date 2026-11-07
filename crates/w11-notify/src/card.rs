//! One toast card.

const TOAST: &str = "toast";
const APP: &str = "toast-app";
const SUMMARY: &str = "toast-summary";
const BODY: &str = "toast-body";

use gtk::gdk;
use gtk::gio;
use gtk::prelude::*;
use std::path::PathBuf;
use w11_common::config::StoredNotification;

const ICON_PX: i32 = 32;
const FALLBACK_ICON: &str = "dialog-information-symbolic";

pub fn build(n: &StoredNotification) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    card.add_css_class(TOAST);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    text.append(&label(&n.app, APP, 0, false));
    text.append(&label(&n.summary, SUMMARY, 36, true));
    if !n.body.is_empty() {
        let b = label(&n.body, BODY, 40, true);
        b.set_lines(4);
        b.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&b);
    }

    card.append(&icon(&n.icon));
    card.append(&text);
    card
}

fn label(text: &str, class: &str, max_chars: i32, wrap: bool) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class(class);
    l.set_xalign(0.0);
    if wrap {
        l.set_wrap(true);
        l.set_max_width_chars(max_chars);
    }
    l
}

/// Icon-theme name, or an image file. Files are decoded straight to icon size (2x for HiDPI)
/// so a 1024x1024 PNG never sits in RAM as a full texture.
fn icon(name: &str) -> gtk::Image {
    let img = if name.is_empty() {
        gtk::Image::from_icon_name(FALLBACK_ICON)
    } else if let Some(path) = icon_path(name) {
        match gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, ICON_PX * 2, ICON_PX * 2, true) {
            Ok(pb) => gtk::Image::from_paintable(Some(&gdk::Texture::for_pixbuf(&pb))),
            Err(_) => gtk::Image::from_icon_name(FALLBACK_ICON),
        }
    } else {
        gtk::Image::from_icon_name(name)
    };
    img.set_pixel_size(ICON_PX);
    img.set_valign(gtk::Align::Start);
    img
}

fn icon_path(name: &str) -> Option<PathBuf> {
    if name.starts_with('/') {
        Some(PathBuf::from(name))
    } else if name.starts_with("file://") {
        gio::File::for_uri(name).path()
    } else {
        None
    }
}
