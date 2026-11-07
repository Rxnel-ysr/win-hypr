//! Network / volume / battery cluster -> opens Quick Settings.

use crate::widgets;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use w11_common::sys;

const POLL_SECS: u32 = 15;

#[derive(Clone)]
struct Icons {
    net: gtk::Image,
    vol: gtk::Image,
    bat: gtk::Image,
}

pub fn build() -> gtk::Button {
    let button = widgets::tray_button();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let icons = Icons {
        net: gtk::Image::from_icon_name("network-wireless-symbolic"),
        vol: gtk::Image::from_icon_name("audio-volume-high-symbolic"),
        bat: gtk::Image::from_icon_name("battery-full-symbolic"),
    };
    icons.bat.set_visible(false);
    row.append(&icons.net);
    row.append(&icons.vol);
    row.append(&icons.bat);
    button.set_child(Some(&row));
    widgets::opens(&button, "w11-flyout quick");

    poll(&icons);
    glib::timeout_add_seconds_local(POLL_SECS, move || {
        poll(&icons);
        glib::ControlFlow::Continue
    });
    button
}

/// Only touch a widget when its icon actually changes.
fn set_icon(img: &gtk::Image, name: &str) {
    if img.icon_name().as_deref() != Some(name) {
        img.set_icon_name(Some(name));
    }
}

/// Probe CLI tools off the UI thread, then paint.
fn poll(icons: &Icons) {
    let icons = icons.clone();
    glib::spawn_future_local(async move {
        let Ok((net, vol, bat)) = gio::spawn_blocking(|| {
            let (v, m) = sys::volume();
            (sys::network_icon(), sys::volume_icon(v, m), sys::battery())
        })
        .await
        else {
            return;
        };
        set_icon(&icons.net, net);
        set_icon(&icons.vol, vol);
        match bat {
            Some(b) => {
                set_icon(&icons.bat, &sys::battery_icon(b));
                icons.bat.set_tooltip_text(Some(&format!("{}%", b.percent)));
                icons.bat.set_visible(true);
            }
            None => icons.bat.set_visible(false),
        }
    });
}
