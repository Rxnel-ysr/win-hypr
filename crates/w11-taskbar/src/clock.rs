//! Clock + date + notification bell -> opens the calendar flyout.
//! One timer, aligned to the minute boundary.

use crate::widgets;
use gtk::glib;
use gtk::prelude::*;

pub fn build() -> gtk::Button {
    let button = widgets::tray_button();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let time_l = gtk::Label::new(None);
    time_l.add_css_class("tb-clock-time");
    time_l.set_halign(gtk::Align::End);
    let date_l = gtk::Label::new(None);
    date_l.add_css_class("tb-clock-date");
    date_l.set_halign(gtk::Align::End);
    text.append(&time_l);
    text.append(&date_l);

    row.append(&text);
    row.append(&gtk::Image::from_icon_name(
        "preferences-system-notifications-symbolic",
    ));
    button.set_child(Some(&row));
    widgets::opens(&button, "w11-flyout calendar");

    tick(time_l, date_l);
    button
}

fn tick(time_l: gtk::Label, date_l: gtk::Label) {
    let wait = match glib::DateTime::now_local() {
        Ok(now) => {
            if let Ok(t) = now.format("%-I:%M %p") {
                time_l.set_text(&t);
            }
            if let Ok(d) = now.format("%-m/%-d/%Y") {
                date_l.set_text(&d);
            }
            (60 - now.second()).max(1) as u32
        }
        Err(_) => 30,
    };
    glib::timeout_add_seconds_local_once(wait, move || tick(time_l, date_l));
}
