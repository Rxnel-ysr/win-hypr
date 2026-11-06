//! Task View (Win+Tab), laid out like Windows 11:
//!
//! * dimmed + blurred wallpaper behind everything
//! * the current desktop's windows as a justified grid of live-ish thumbnails, each with an app
//!   icon + title above it and a close button on hover (left click = switch, middle click = close)
//! * a desktop strip at the bottom: one thumbnail per desktop (with its windows drawn in
//!   miniature), a close button on hover, and a "New desktop" tile; hovering a desktop previews
//!   its windows in the grid
//! * keyboard: arrows move, Enter opens, Delete closes, Tab jumps between windows and desktops,
//!   Esc / Win+Tab again / click on empty space dismisses
//!
//! Layout of this module:
//!   mod.rs     state, build/refresh, window grid
//!   layout.rs  pure grid geometry (unit-tested)
//!   strip.rs   desktop strip at the bottom
//!   keys.rs    keyboard navigation
//!
//! Memory: everything (thumbnail textures, icons, the app list) is dropped when the window hides
//! and the heap is trimmed, so an idle Task View process stays small.

mod keys;
mod layout;
mod strip;

use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use w11_common::hypr::{self, Client};
use w11_common::mem;
use w11_common::popup::{Placement, Popup};
use w11_common::{apps, desktops, thumbs};

const TITLE_H: f64 = 30.0; // icon + title row above a thumbnail
const CARD_PAD: f64 = 8.0; // 2px border + 6px padding around a card
const GAP_X: f64 = 14.0;
const ROW_GAP: f64 = 14.0;
const SIDE_PAD: f64 = 64.0;
const TOP: f64 = 44.0;
const BAR_GAP: f64 = 20.0;
const BAR_PAD: f64 = 22.0;
const DESK_EXTRA: f64 = 52.0; // label + paddings under a desktop thumbnail

// ----------------------------------------------------------------- state

#[derive(Clone, Copy, PartialEq, Eq)]
enum Zone {
    Windows,
    Desktops,
}

struct Card {
    widget: gtk::Widget,
    client: Client,
    row: usize,
    cx: f64,
}

struct St {
    clients: Vec<Client>,
    origin: desktops::Origins,
    current: i64,
    count: i64,
    shown: i64,
    cards: Vec<Card>,
    desks: Vec<gtk::Widget>,
    sel: Option<usize>,
    dsel: Option<usize>,
    zone: Zone,
    textures: HashMap<String, gdk::Texture>,
    /// window class -> icon, resolved once per open (the full app list is not kept around)
    icons: HashMap<String, gio::Icon>,
    mon: (f64, f64),
    /// top-left of the focused monitor, fetched once per open
    mon_origin: (f64, f64),
    desk_size: (f64, f64),
    stage: (f64, f64),
}

struct Tv {
    popup: Rc<Popup>,
    stage: gtk::Fixed,
    bar: gtk::Box,
    revert: RefCell<Option<glib::SourceId>>,
    st: RefCell<St>,
}

fn mru_key(c: &Client) -> i64 {
    if c.focus_history_id < 0 {
        i64::MAX
    } else {
        c.focus_history_id
    }
}

fn is_interactive(w: &gtk::Widget) -> bool {
    let mut cur = Some(w.clone());
    while let Some(x) = cur {
        if x.has_css_class("tv-card") || x.has_css_class("tv-desk") {
            return true;
        }
        cur = x.parent();
    }
    false
}

fn default_icon() -> gio::Icon {
    gio::ThemedIcon::new("application-x-executable").upcast()
}

/// Match every distinct window class to its app icon, then free the (large) app list again.
fn resolve_icons(clients: &[Client]) -> HashMap<String, gio::Icon> {
    let list = apps::all();
    let mut out: HashMap<String, gio::Icon> = HashMap::new();
    for c in clients {
        if out.contains_key(&c.class) {
            continue;
        }
        let icon = list
            .iter()
            .find(|a| apps::class_matches(a, &c.class))
            .map(apps::icon_of)
            .unwrap_or_else(default_icon);
        out.insert(c.class.clone(), icon);
    }
    drop(list);
    apps::invalidate();
    out
}

// ----------------------------------------------------------------- build

pub fn build(app: &gtk::Application) -> Rc<Popup> {
    w11_common::load_css!("../css/taskview.css");
    let popup = Popup::new(app, "w11-taskview", 0, 0, Placement::Fullscreen);
    popup.window.add_css_class("taskview-bg");

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_hexpand(true);
    root.set_vexpand(true);

    let stage = gtk::Fixed::new();
    stage.set_halign(gtk::Align::Center);
    stage.set_valign(gtk::Align::Start);
    stage.set_margin_top(TOP as i32);
    root.append(&stage);

    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    bar.set_halign(gtk::Align::Center);
    bar.set_valign(gtk::Align::End);
    bar.set_vexpand(true);
    bar.set_margin_bottom(BAR_PAD as i32);
    root.append(&bar);

    popup.window.set_child(Some(&root));

    let tv = Rc::new(Tv {
        popup: popup.clone(),
        stage,
        bar,
        revert: RefCell::new(None),
        st: RefCell::new(St {
            clients: Vec::new(),
            origin: HashMap::new(),
            current: 1,
            count: 1,
            shown: 1,
            cards: Vec::new(),
            desks: Vec::new(),
            sel: None,
            dsel: None,
            zone: Zone::Windows,
            textures: HashMap::new(),
            icons: HashMap::new(),
            mon: (1920.0, 1080.0),
            mon_origin: (0.0, 0.0),
            desk_size: (170.0, 96.0),
            stage: (1400.0, 600.0),
        }),
    });

    // click on empty space dismisses
    let bg = gtk::GestureClick::builder().button(1).build();
    {
        let tv = tv.clone();
        bg.connect_released(move |g, _, x, y| {
            let Some(w) = g.widget() else { return };
            let hit = w.pick(x, y, gtk::PickFlags::DEFAULT);
            if !hit.map(|h| is_interactive(&h)).unwrap_or(false) {
                tv.popup.hide();
            }
        });
    }
    root.add_controller(bg);

    // keyboard
    let key = gtk::EventControllerKey::new();
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let tv = tv.clone();
        key.connect_key_pressed(move |_, k, _, _| tv.on_key(k));
    }
    popup.window.add_controller(key);

    {
        let tv = tv.clone();
        popup.window.connect_show(move |_| tv.refresh());
    }
    {
        // free thumbnails (and their GPU/heap memory) while hidden
        let tv = tv.clone();
        popup.window.connect_hide(move |_| {
            tv.clear();
            mem::trim_soon();
        });
    }
    popup
}

impl Tv {
    fn clear(&self) {
        while let Some(c) = self.stage.first_child() {
            self.stage.remove(&c);
        }
        while let Some(c) = self.bar.first_child() {
            self.bar.remove(&c);
        }
        if let Some(id) = self.revert.borrow_mut().take() {
            id.remove();
        }
        let mut st = self.st.borrow_mut();
        st.clients.clear();
        st.origin.clear();
        st.cards.clear();
        st.desks.clear();
        st.textures.clear();
        st.icons.clear();
        st.sel = None;
        st.dsel = None;
        drop(st);
        apps::invalidate();
    }

    /// Re-read the compositor state and rebuild everything.
    fn refresh(self: &Rc<Self>) {
        let clients = hypr::clients();
        let origin = desktops::origins();
        let current = desktops::current();
        let count = desktops::count_for(&clients, &origin, current);

        let (mw, mh, uw, uh, mx, my) = match hypr::focused_monitor() {
            Some(m) => {
                let (_, _, uw, uh) = m.usable();
                (
                    m.width as f64 / m.scale,
                    m.height as f64 / m.scale,
                    uw as f64,
                    uh as f64,
                    m.x as f64,
                    m.y as f64,
                )
            }
            None => (1920.0, 1080.0, 1920.0, 1032.0, 0.0, 0.0),
        };
        let dw = (uw / 9.0).clamp(150.0, 220.0).round();
        let dh = (dw * mh / mw).round();
        let stage_h = (uh - TOP - BAR_GAP - (dh + DESK_EXTRA) - BAR_PAD).max(200.0);

        let mut textures = HashMap::new();
        for c in &clients {
            if let Ok(t) = gdk::Texture::from_filename(thumbs::path_for(&c.address)) {
                textures.insert(c.address.clone(), t);
            }
        }
        let icons = resolve_icons(&clients);

        {
            let mut st = self.st.borrow_mut();
            st.clients = clients;
            st.origin = origin;
            st.current = current;
            st.count = count;
            st.shown = current;
            st.textures = textures;
            st.icons = icons;
            st.mon = (mw, mh);
            st.mon_origin = (mx, my);
            st.desk_size = (dw, dh);
            st.stage = (uw, stage_h);
            st.sel = None;
            st.dsel = None;
            st.zone = Zone::Windows;
        }
        self.stage.set_size_request(uw as i32, stage_h as i32);
        self.build_bar();
        self.show_desktop(current);
    }

    // ------------------------------------------------------ window grid

    fn show_desktop(self: &Rc<Self>, d: i64) {
        while let Some(c) = self.stage.first_child() {
            self.stage.remove(&c);
        }
        let (mut wins, textures, icons, (sw, sh)) = {
            let st = self.st.borrow();
            let wins: Vec<Client> = st
                .clients
                .iter()
                .filter(|c| desktops::desktop_of(c, &st.origin, st.current) == d)
                .cloned()
                .collect();
            (wins, st.textures.clone(), st.icons.clone(), st.stage)
        };
        wins.sort_by_key(mru_key);

        let aspects: Vec<f64> = wins
            .iter()
            .map(|c| c.size[0].max(1) as f64 / c.size[1].max(1) as f64)
            .collect();
        let slots = layout::compute(&aspects, sw - 2.0 * SIDE_PAD, sh);

        let mut cards = Vec::with_capacity(wins.len());
        for (i, (c, s)) in wins.iter().zip(slots.iter()).enumerate() {
            let icon = icons.get(&c.class).cloned().unwrap_or_else(default_icon);
            let card = self.make_card(i, c, s, textures.get(&c.address), &icon);
            self.stage.put(&card, SIDE_PAD + s.x, s.y);
            cards.push(Card {
                widget: card.clone().upcast(),
                client: c.clone(),
                row: s.row,
                cx: s.x + s.w / 2.0,
            });
        }
        if cards.is_empty() {
            let l = gtk::Label::new(Some("No open windows"));
            l.add_css_class("tv-empty");
            l.set_size_request(sw as i32, 40);
            self.stage.put(&l, 0.0, sh / 2.0 - 30.0);
        }

        {
            let mut st = self.st.borrow_mut();
            st.shown = d;
            st.cards = cards;
            st.sel = None;
        }
        // let the first frame render in the "pre" state, then transition in
        let tv = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(16), move || {
            for c in tv.st.borrow().cards.iter() {
                c.widget.remove_css_class("pre");
            }
        });
    }

    fn make_card(
        self: &Rc<Self>,
        idx: usize,
        c: &Client,
        s: &layout::Slot,
        tex: Option<&gdk::Texture>,
        icon: &gio::Icon,
    ) -> gtk::Box {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("tv-card");
        card.add_css_class("pre");
        card.set_size_request(
            (s.w + 2.0 * CARD_PAD) as i32,
            (s.h + TITLE_H + 2.0 * CARD_PAD) as i32,
        );

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.add_css_class("tv-head");
        head.set_size_request(-1, TITLE_H as i32);
        let img = gtk::Image::from_gicon(icon);
        img.set_pixel_size(16);
        let title = gtk::Label::new(Some(if c.title.is_empty() {
            &c.class
        } else {
            &c.title
        }));
        title.add_css_class("tv-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("tv-close");
        close.set_valign(gtk::Align::Center);
        head.append(&img);
        head.append(&title);
        head.append(&close);

        let thumb = gtk::Overlay::new();
        thumb.add_css_class("tv-thumb");
        thumb.set_overflow(gtk::Overflow::Hidden);
        thumb.set_size_request(s.w as i32, s.h as i32);
        match tex {
            Some(t) => {
                let pic = gtk::Picture::for_paintable(t);
                pic.set_content_fit(gtk::ContentFit::Cover);
                pic.set_can_shrink(true);
                thumb.set_child(Some(&pic));
            }
            None => {
                let big = gtk::Image::from_gicon(icon);
                big.set_pixel_size(64);
                big.set_halign(gtk::Align::Center);
                big.set_valign(gtk::Align::Center);
                thumb.set_child(Some(&big));
            }
        }
        if c.minimized() {
            let m = gtk::Label::new(Some("Minimized"));
            m.add_css_class("tv-badge");
            m.set_halign(gtk::Align::Start);
            m.set_valign(gtk::Align::End);
            thumb.add_overlay(&m);
        }
        card.append(&head);
        card.append(&thumb);

        let click = gtk::GestureClick::builder().button(0).build();
        {
            let tv = self.clone();
            click.connect_released(move |g, _, _, _| match g.current_button() {
                1 => tv.activate(idx),
                2 => tv.close_window(idx),
                _ => {}
            });
        }
        card.add_controller(click);
        {
            let tv = self.clone();
            close.connect_clicked(move |_| tv.close_window(idx));
        }
        let motion = gtk::EventControllerMotion::new();
        {
            let tv = self.clone();
            motion.connect_enter(move |_, _, _| {
                tv.leave_desktops();
                tv.set_sel(Some(idx));
            });
            let tv = self.clone();
            motion.connect_leave(move |_| {
                if tv.st.borrow().sel == Some(idx) {
                    tv.set_sel(None);
                }
            });
        }
        card.add_controller(motion);
        card
    }

    fn set_sel(&self, new: Option<usize>) {
        let mut st = self.st.borrow_mut();
        if let Some(c) = st.sel.and_then(|i| st.cards.get(i)) {
            c.widget.remove_css_class("selected");
        }
        st.sel = new;
        if let Some(c) = new.and_then(|i| st.cards.get(i)) {
            c.widget.add_css_class("selected");
        }
    }

    fn activate(&self, idx: usize) {
        let c = self.st.borrow().cards.get(idx).map(|c| c.client.clone());
        if let Some(c) = c {
            self.popup.hide();
            std::thread::spawn(move || {
                hypr::restore_and_focus(&c);
            });
        }
    }

    fn close_window(self: &Rc<Self>, idx: usize) {
        let Some(c) = self.st.borrow().cards.get(idx).map(|c| c.client.clone()) else {
            return;
        };
        let addr = c.address.clone();
        std::thread::spawn(move || {
            hypr::close(&addr);
        });
        let (shown, remaining) = {
            let mut st = self.st.borrow_mut();
            st.clients.retain(|x| x.address != c.address);
            let shown = st.shown;
            let current = st.current;
            let origin = st.origin.clone();
            let remaining = st
                .clients
                .iter()
                .filter(|x| desktops::desktop_of(x, &origin, current) == shown)
                .count();
            (shown, remaining)
        };
        self.show_desktop(shown);
        self.build_bar();
        if remaining > 0 {
            self.set_sel(Some(idx.min(remaining - 1)));
        }
    }
}
