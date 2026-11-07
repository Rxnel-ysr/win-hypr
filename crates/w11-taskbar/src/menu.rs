//! Right-click jump menu. Built when requested, destroyed when it closes, so an idle taskbar
//! holds zero popover widgets.

use crate::strip::Strip;
use gtk::glib;
use gtk::prelude::*;
use std::rc::Rc;
use w11_common::{apps, hypr};

fn entry(pop: &gtk::Popover, label: &str, act: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::with_label(label);
    b.add_css_class("menu-item");
    if let Some(l) = b.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
        l.set_xalign(0.0);
    }
    let pop = pop.downgrade();
    b.connect_clicked(move |_| {
        if let Some(p) = pop.upgrade() {
            p.popdown();
        }
        act();
    });
    b
}

pub fn show(strip: &Rc<Strip>, idx: usize, anchor: &gtk::Overlay) {
    let Some(m) = strip.menu_info(idx) else {
        return;
    };
    strip.drop_menu();

    let pop = gtk::Popover::new();
    pop.set_position(gtk::PositionType::Top);
    pop.set_has_arrow(false);
    pop.set_parent(anchor);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);

    if let Some(info) = m.info.clone() {
        let i = info.clone();
        list.append(&entry(&pop, &m.name, move || apps::launch(&i)));

        let s = strip.clone();
        let label = if m.pinned {
            "Unpin from taskbar"
        } else {
            "Pin to taskbar"
        };
        list.append(&entry(&pop, label, move || s.toggle_pin(&info)));
    }

    if !m.addrs.is_empty() {
        let label = if m.addrs.len() > 1 {
            "Close all windows"
        } else {
            "Close window"
        };
        let addrs = m.addrs;
        list.append(&entry(&pop, label, move || {
            let addrs = addrs.clone();
            std::thread::spawn(move || {
                for a in addrs {
                    hypr::close(&a);
                }
            });
        }));
    }

    pop.set_child(Some(&list));
    {
        let s = strip.clone();
        pop.connect_closed(move |_| {
            let s = s.clone();
            glib::idle_add_local_once(move || s.drop_menu());
        });
    }
    strip.track_menu(pop.clone());
    pop.popup();
}
