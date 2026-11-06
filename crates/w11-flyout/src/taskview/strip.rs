//! The desktop strip at the bottom of Task View: one tile per virtual desktop + "New desktop".

use super::{mru_key, Tv, Zone};
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::rc::Rc;
use std::time::Duration;
use w11_common::desktops;
use w11_common::hypr::{self, Client};

impl Tv {
    pub(super) fn build_bar(self: &Rc<Self>) {
        while let Some(c) = self.bar.first_child() {
            self.bar.remove(&c);
        }
        let (count, current, (dw, dh)) = {
            let st = self.st.borrow();
            (st.count, st.current, st.desk_size)
        };
        let mut widgets: Vec<gtk::Widget> = Vec::new();
        for d in 1..=count {
            let w = self.make_desk(d, d == current, dw, dh, count > 1);
            self.bar.append(&w);
            widgets.push(w.upcast());
        }
        if count < desktops::MAX_DESKTOPS {
            let w = self.make_new_tile(count as usize, dw, dh);
            self.bar.append(&w);
            widgets.push(w.upcast());
        }
        let mut st = self.st.borrow_mut();
        st.desks = widgets;
        st.dsel = None;
    }

    fn make_desk(
        self: &Rc<Self>,
        d: i64,
        is_current: bool,
        dw: f64,
        dh: f64,
        closable: bool,
    ) -> gtk::Box {
        let item = gtk::Box::new(gtk::Orientation::Vertical, 8);
        item.add_css_class("tv-desk");
        if is_current {
            item.add_css_class("current");
        }
        item.set_halign(gtk::Align::Center);

        let thumb = gtk::Overlay::new();
        thumb.add_css_class("tv-desk-thumb");
        thumb.set_overflow(gtk::Overflow::Hidden);
        thumb.set_size_request(dw as i32, dh as i32);

        // the desktop's windows, drawn in miniature
        let fixed = gtk::Fixed::new();
        {
            let st = self.st.borrow();
            let (mw, mh) = st.mon;
            let (mx, my) = st.mon_origin; // cached on open: no IPC round-trip per desktop
            let (sx, sy) = (dw / mw, dh / mh);
            let mut wins: Vec<&Client> = st
                .clients
                .iter()
                .filter(|c| !c.minimized() && desktops::desktop_of(c, &st.origin, st.current) == d)
                .collect();
            // back-to-front so the most recently used window ends up on top
            wins.sort_by_key(|c| std::cmp::Reverse(mru_key(c)));
            for c in wins {
                let x = ((c.at[0] as f64 - mx) * sx).max(0.0);
                let y = ((c.at[1] as f64 - my) * sy).max(0.0);
                let w = (c.size[0] as f64 * sx).max(6.0);
                let h = (c.size[1] as f64 * sy).max(6.0);
                let mini: gtk::Widget = match st.textures.get(&c.address) {
                    Some(t) => {
                        let p = gtk::Picture::for_paintable(t);
                        p.set_content_fit(gtk::ContentFit::Cover);
                        p.set_can_shrink(true);
                        p.upcast()
                    }
                    None => {
                        let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                        b.add_css_class("mini-win");
                        b.upcast()
                    }
                };
                mini.set_size_request(w as i32, h as i32);
                fixed.put(&mini, x, y);
            }
        }
        // A Picture's natural size is its texture size, which would stretch the strip; a
        // non-propagating ScrolledWindow clamps the miniature to exactly dw x dh.
        let clamp = gtk::ScrolledWindow::new();
        clamp.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Never);
        clamp.set_min_content_width(dw as i32);
        clamp.set_min_content_height(dh as i32);
        clamp.set_propagate_natural_width(false);
        clamp.set_propagate_natural_height(false);
        clamp.set_child(Some(&fixed));
        thumb.set_child(Some(&clamp));

        if closable {
            let x = gtk::Button::from_icon_name("window-close-symbolic");
            x.add_css_class("tv-desk-close");
            x.set_halign(gtk::Align::End);
            x.set_valign(gtk::Align::Start);
            let tv = self.clone();
            x.connect_clicked(move |_| tv.close_desktop(d));
            thumb.add_overlay(&x);
        }

        let label = gtk::Label::new(Some(&format!("Desktop {d}")));
        label.add_css_class("tv-desk-label");
        item.append(&thumb);
        item.append(&label);

        let click = gtk::GestureClick::builder().button(1).build();
        {
            let tv = self.clone();
            click.connect_released(move |_, _, _, _| tv.switch_desktop(d));
        }
        item.add_controller(click);

        let motion = gtk::EventControllerMotion::new();
        {
            let idx = (d - 1) as usize;
            let tv = self.clone();
            motion.connect_enter(move |_, _, _| {
                tv.hover_desk(idx);
                tv.preview(d);
            });
            let tv = self.clone();
            motion.connect_leave(move |_| {
                tv.unhover_desk(idx);
                tv.schedule_revert();
            });
        }
        item.add_controller(motion);
        item
    }

    fn make_new_tile(self: &Rc<Self>, idx: usize, dw: f64, dh: f64) -> gtk::Box {
        let item = gtk::Box::new(gtk::Orientation::Vertical, 8);
        item.add_css_class("tv-desk");
        item.set_halign(gtk::Align::Center);
        let thumb = gtk::Overlay::new();
        thumb.add_css_class("tv-desk-thumb");
        thumb.add_css_class("tv-new");
        thumb.set_size_request(dw as i32, dh as i32);
        let plus = gtk::Image::from_icon_name("list-add-symbolic");
        plus.set_pixel_size(24);
        plus.set_halign(gtk::Align::Center);
        plus.set_valign(gtk::Align::Center);
        thumb.set_child(Some(&plus));
        let label = gtk::Label::new(Some("New desktop"));
        label.add_css_class("tv-desk-label");
        item.append(&thumb);
        item.append(&label);
        let click = gtk::GestureClick::builder().button(1).build();
        {
            let tv = self.clone();
            click.connect_released(move |_, _, _, _| tv.new_desktop());
        }
        item.add_controller(click);
        let motion = gtk::EventControllerMotion::new();
        {
            let tv = self.clone();
            motion.connect_enter(move |_, _, _| tv.hover_desk(idx));
            let tv = self.clone();
            motion.connect_leave(move |_| tv.unhover_desk(idx));
        }
        item.add_controller(motion);
        item
    }

    pub(super) fn switch_desktop(&self, d: i64) {
        self.popup.hide();
        std::thread::spawn(move || {
            hypr::switch_workspace(d);
        });
    }

    pub(super) fn new_desktop(self: &Rc<Self>) {
        let tv = self.clone();
        glib::spawn_future_local(async move {
            let _ = gio::spawn_blocking(desktops::new_desktop).await;
            glib::timeout_future(Duration::from_millis(120)).await;
            tv.refresh();
        });
    }

    pub(super) fn close_desktop(self: &Rc<Self>, d: i64) {
        let tv = self.clone();
        glib::spawn_future_local(async move {
            let _ = gio::spawn_blocking(move || desktops::close_desktop(d)).await;
            glib::timeout_future(Duration::from_millis(150)).await;
            tv.refresh();
        });
    }

    /// Hovering a desktop shows its windows in the grid.
    pub(super) fn preview(self: &Rc<Self>, d: i64) {
        if let Some(id) = self.revert.borrow_mut().take() {
            id.remove();
        }
        if self.st.borrow().shown != d {
            self.show_desktop(d);
        }
    }

    fn schedule_revert(self: &Rc<Self>) {
        if let Some(id) = self.revert.borrow_mut().take() {
            id.remove();
        }
        let tv = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(140), move || {
            tv.revert.borrow_mut().take();
            let (shown, current, zone) = {
                let st = tv.st.borrow();
                (st.shown, st.current, st.zone)
            };
            if shown != current && zone == Zone::Windows {
                tv.show_desktop(current);
            }
        });
        *self.revert.borrow_mut() = Some(id);
    }

    pub(super) fn set_dsel(&self, new: Option<usize>) {
        let mut st = self.st.borrow_mut();
        if let Some(w) = st.dsel.and_then(|i| st.desks.get(i)) {
            w.remove_css_class("selected");
        }
        st.dsel = new;
        if let Some(w) = new.and_then(|i| st.desks.get(i)) {
            w.add_css_class("selected");
        }
    }

    /// Pointer entered a desktop tile: it becomes *the* highlighted item (one highlight at a time).
    fn hover_desk(self: &Rc<Self>, idx: usize) {
        self.set_sel(None);
        self.st.borrow_mut().zone = Zone::Desktops;
        self.set_dsel(Some(idx));
    }

    fn unhover_desk(self: &Rc<Self>, idx: usize) {
        let is_it = {
            let st = self.st.borrow();
            st.dsel == Some(idx) && st.zone == Zone::Desktops
        };
        if is_it {
            self.set_dsel(None);
            self.st.borrow_mut().zone = Zone::Windows;
        }
    }

    /// Mouse moved back onto the window grid: leave keyboard "desktop strip" mode.
    pub(super) fn leave_desktops(&self) {
        let in_desktops = self.st.borrow().zone == Zone::Desktops;
        if in_desktops {
            self.set_dsel(None);
            self.st.borrow_mut().zone = Zone::Windows;
        }
    }
}
