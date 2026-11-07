//! Small shared builders.

use crate::tray;
use gtk::prelude::*;
use w11_common::{hypr, sys};

pub fn symbolic(name: &str, px: i32) -> gtk::Image {
    gtk::Image::builder().icon_name(name).pixel_size(px).build()
}

pub fn icon_button(child: &impl IsA<gtk::Widget>, tip: &str) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("tb-button");
    b.set_child(Some(child));
    b.set_tooltip_text(Some(tip));
    b.set_width_request(44);
    b
}

pub fn tray_button() -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("tray-button");
    b
}

/// Button that runs a shell command (flyouts toggle themselves on re-launch).
pub fn opens(b: &gtk::Button, cmd: &'static str) {
    b.connect_clicked(move |_| sys::spawn(cmd));
}

pub fn windows_logo() -> gtk::Grid {
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

/// The "^" chevron with the hidden-tray-icons popover (icons are fetched only while open).
pub fn tray_overflow() -> gtk::Button {
    let items = tray::start();
    let chevron = tray_button();
    chevron.set_child(Some(&gtk::Image::from_icon_name("pan-up-symbolic")));

    let pop = gtk::Popover::new();
    pop.set_has_arrow(false);
    pop.set_position(gtk::PositionType::Top);
    pop.set_parent(&chevron);
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_max_children_per_line(5);
    pop.set_child(Some(&flow));
    tray::attach(items, &flow, &pop);

    chevron.connect_clicked(move |_| pop.popup());
    chevron
}

/// The thin "show desktop" sliver at the far right.
pub fn show_desktop_sliver() -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("tb-show-desktop");
    b.set_tooltip_text(Some("Show desktop"));
    b.connect_clicked(|_| {
        std::thread::spawn(hypr::toggle_show_desktop);
    });
    b
}
