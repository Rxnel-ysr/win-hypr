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

use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use w11_common::hypr::{self, Client};
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

// ---------------------------------------------------------------- layout

pub mod layout {
    use super::{CARD_PAD, GAP_X, ROW_GAP, TITLE_H};

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Slot {
        pub x: f64,
        pub y: f64,
        /// thumbnail size (the card adds a title row and padding around it)
        pub w: f64,
        pub h: f64,
        pub row: usize,
    }

    /// Justified rows of equal-height thumbnails, centred in `avail_w` x `avail_h`.
    /// Tries every row count and keeps the one that lets the thumbnails be biggest.
    pub fn compute(aspects: &[f64], avail_w: f64, avail_h: f64) -> Vec<Slot> {
        let n = aspects.len();
        if n == 0 {
            return Vec::new();
        }
        let a: Vec<f64> = aspects.iter().map(|x| x.clamp(0.6, 2.4)).collect();
        let total: f64 = a.iter().sum();
        let chrome = TITLE_H + 2.0 * CARD_PAD;

        let mut best: Option<(f64, Vec<Vec<usize>>)> = None;
        for r in 1..=n {
            let target = total / r as f64;
            let mut rows: Vec<Vec<usize>> = vec![Vec::new(); r];
            let mut cum = 0.0;
            for (i, ai) in a.iter().enumerate() {
                let k = (((cum + ai / 2.0) / target).floor() as usize).min(r - 1);
                rows[k].push(i);
                cum += ai;
            }
            rows.retain(|v| !v.is_empty());
            let nr = rows.len() as f64;
            let h_v = (avail_h - (nr - 1.0) * ROW_GAP) / nr - chrome;
            let h_w = rows
                .iter()
                .map(|row| {
                    let s: f64 = row.iter().map(|&i| a[i]).sum();
                    let fixed = GAP_X * (row.len() as f64 - 1.0) + 2.0 * CARD_PAD * row.len() as f64;
                    (avail_w - fixed) / s
                })
                .fold(f64::INFINITY, f64::min);
            let h = h_v.min(h_w);
            if best.as_ref().map_or(true, |(bh, _)| h > *bh + 0.5) {
                best = Some((h, rows));
            }
        }
        let (h, rows) = best.unwrap_or((120.0, vec![(0..n).collect()]));
        let h = h.min(avail_h * 0.62).max(40.0);

        let nr = rows.len() as f64;
        let block_h = nr * (h + chrome) + (nr - 1.0) * ROW_GAP;
        let mut y = ((avail_h - block_h) / 2.0).max(0.0);
        let mut out = vec![Slot::default(); n];
        for (ri, row) in rows.iter().enumerate() {
            let row_w: f64 = row.iter().map(|&i| a[i] * h + 2.0 * CARD_PAD).sum::<f64>()
                + GAP_X * (row.len() as f64 - 1.0);
            let mut x = ((avail_w - row_w) / 2.0).max(0.0);
            for &i in row {
                let w = a[i] * h;
                out[i] = Slot { x, y, w, h, row: ri };
                x += w + 2.0 * CARD_PAD + GAP_X;
            }
            y += h + chrome + ROW_GAP;
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn bounds_ok(slots: &[Slot], w: f64, h: f64) {
            for s in slots {
                assert!(s.x >= -0.5 && s.x + s.w + 2.0 * CARD_PAD <= w + 0.5, "x out of bounds: {s:?}");
                assert!(s.y >= -0.5 && s.y + s.h + TITLE_H + 2.0 * CARD_PAD <= h + 0.5, "y out of bounds: {s:?}");
            }
        }

        #[test]
        fn single_window_is_centred_and_capped() {
            let s = compute(&[16.0 / 9.0], 1400.0, 630.0);
            assert_eq!(s.len(), 1);
            assert!(s[0].h <= 630.0 * 0.62 + 0.01);
            let left = s[0].x;
            let right = 1400.0 - (s[0].x + s[0].w + 2.0 * CARD_PAD);
            assert!((left - right).abs() < 1.0);
            bounds_ok(&s, 1400.0, 630.0);
        }

        #[test]
        fn many_windows_fit() {
            for n in 1..=24 {
                let asp: Vec<f64> = (0..n).map(|i| [1.78, 1.33, 0.8, 2.1][i % 4]).collect();
                let s = compute(&asp, 1400.0, 630.0);
                assert_eq!(s.len(), n);
                bounds_ok(&s, 1400.0, 630.0);
            }
        }

        #[test]
        fn empty_is_empty() {
            assert!(compute(&[], 100.0, 100.0).is_empty());
        }

        #[test]
        fn rows_do_not_overlap() {
            let asp = vec![1.78; 8];
            let s = compute(&asp, 1400.0, 630.0);
            for a in &s {
                for b in &s {
                    if a == b {
                        continue;
                    }
                    let ax2 = a.x + a.w + 2.0 * CARD_PAD;
                    let bx2 = b.x + b.w + 2.0 * CARD_PAD;
                    let ay2 = a.y + a.h + TITLE_H + 2.0 * CARD_PAD;
                    let by2 = b.y + b.h + TITLE_H + 2.0 * CARD_PAD;
                    let overlap = a.x < bx2 - 0.5 && b.x < ax2 - 0.5 && a.y < by2 - 0.5 && b.y < ay2 - 0.5;
                    assert!(!overlap, "{a:?} overlaps {b:?}");
                }
            }
        }
    }
}

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
    apps: Vec<gio::AppInfo>,
    mon: (f64, f64),
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

// ----------------------------------------------------------------- build

pub fn build(app: &gtk::Application) -> Rc<Popup> {
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
            apps: Vec::new(),
            mon: (1920.0, 1080.0),
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
        // free thumbnails (and their GPU textures) while hidden
        let tv = tv.clone();
        popup.window.connect_hide(move |_| tv.clear());
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
        st.cards.clear();
        st.desks.clear();
        st.textures.clear();
        st.apps.clear();
        st.sel = None;
        st.dsel = None;
    }

    /// Re-read the compositor state and rebuild everything.
    fn refresh(self: &Rc<Self>) {
        let clients = hypr::clients();
        let origin = desktops::origins();
        let current = desktops::current();
        let count = desktops::count_for(&clients, &origin, current);

        let (mw, mh, uw, uh) = match hypr::focused_monitor() {
            Some(m) => {
                let (_, _, uw, uh) = m.usable();
                (m.width as f64 / m.scale, m.height as f64 / m.scale, uw as f64, uh as f64)
            }
            None => (1920.0, 1080.0, 1920.0, 1032.0),
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

        {
            let mut st = self.st.borrow_mut();
            st.clients = clients;
            st.origin = origin;
            st.current = current;
            st.count = count;
            st.shown = current;
            st.textures = textures;
            st.apps = apps::all();
            st.mon = (mw, mh);
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

    fn icon_for(apps_list: &[gio::AppInfo], class: &str) -> gio::Icon {
        apps_list
            .iter()
            .find(|a| apps::class_matches(a, class))
            .map(apps::icon_of)
            .unwrap_or_else(|| gio::ThemedIcon::new("application-x-executable").upcast())
    }

    // ------------------------------------------------------ window grid

    fn show_desktop(self: &Rc<Self>, d: i64) {
        while let Some(c) = self.stage.first_child() {
            self.stage.remove(&c);
        }
        let (mut wins, textures, apps_list, (sw, sh)) = {
            let st = self.st.borrow();
            let wins: Vec<Client> = st
                .clients
                .iter()
                .filter(|c| desktops::desktop_of(c, &st.origin, st.current) == d)
                .cloned()
                .collect();
            (wins, st.textures.clone(), st.apps.clone(), st.stage)
        };
        wins.sort_by_key(mru_key);

        let aspects: Vec<f64> = wins
            .iter()
            .map(|c| c.size[0].max(1) as f64 / c.size[1].max(1) as f64)
            .collect();
        let slots = layout::compute(&aspects, sw - 2.0 * SIDE_PAD, sh);

        let mut cards = Vec::with_capacity(wins.len());
        for (i, (c, s)) in wins.iter().zip(slots.iter()).enumerate() {
            let icon = Self::icon_for(&apps_list, &c.class);
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
        let title = gtk::Label::new(Some(if c.title.is_empty() { &c.class } else { &c.title }));
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
        let Some(c) = self.st.borrow().cards.get(idx).map(|c| c.client.clone()) else { return };
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

    // ---------------------------------------------------- desktop strip

    fn build_bar(self: &Rc<Self>) {
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

    fn make_desk(self: &Rc<Self>, d: i64, is_current: bool, dw: f64, dh: f64, closable: bool) -> gtk::Box {
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
            let mon = hypr::focused_monitor();
            let (mx, my) = mon.map(|m| (m.x as f64, m.y as f64)).unwrap_or((0.0, 0.0));
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

    fn switch_desktop(&self, d: i64) {
        self.popup.hide();
        std::thread::spawn(move || {
            hypr::switch_workspace(d);
        });
    }

    fn new_desktop(self: &Rc<Self>) {
        let tv = self.clone();
        glib::spawn_future_local(async move {
            let _ = gio::spawn_blocking(|| desktops::new_desktop()).await;
            glib::timeout_future(Duration::from_millis(120)).await;
            tv.refresh();
        });
    }

    fn close_desktop(self: &Rc<Self>, d: i64) {
        let tv = self.clone();
        glib::spawn_future_local(async move {
            let _ = gio::spawn_blocking(move || desktops::close_desktop(d)).await;
            glib::timeout_future(Duration::from_millis(150)).await;
            tv.refresh();
        });
    }

    /// Hovering a desktop shows its windows in the grid.
    fn preview(self: &Rc<Self>, d: i64) {
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

    fn set_dsel(&self, new: Option<usize>) {
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
    fn leave_desktops(&self) {
        let in_desktops = self.st.borrow().zone == Zone::Desktops;
        if in_desktops {
            self.set_dsel(None);
            self.st.borrow_mut().zone = Zone::Windows;
        }
    }

    // --------------------------------------------------------- keyboard

    fn on_key(self: &Rc<Self>, key: gdk::Key) -> glib::Propagation {
        use gdk::Key;
        let zone = self.st.borrow().zone;
        match (zone, key) {
            (_, Key::Tab) | (_, Key::ISO_Left_Tab) => {
                if zone == Zone::Windows {
                    self.enter_desktops();
                } else {
                    self.enter_windows();
                }
            }
            (Zone::Windows, Key::Left) => self.move_sel(-1),
            (Zone::Windows, Key::Right) => self.move_sel(1),
            (Zone::Windows, Key::Up) => self.move_vertical(-1),
            (Zone::Windows, Key::Down) => self.move_vertical(1),
            (Zone::Windows, Key::Home) => self.select_first_or_last(true),
            (Zone::Windows, Key::End) => self.select_first_or_last(false),
            (Zone::Windows, Key::Return) | (Zone::Windows, Key::KP_Enter) | (Zone::Windows, Key::space) => {
                if let Some(i) = self.st.borrow().sel {
                    self.activate(i);
                }
            }
            (Zone::Windows, Key::Delete) => {
                let sel = self.st.borrow().sel;
                if let Some(i) = sel {
                    self.close_window(i);
                }
            }
            (Zone::Desktops, Key::Left) => self.move_dsel(-1),
            (Zone::Desktops, Key::Right) => self.move_dsel(1),
            (Zone::Desktops, Key::Up) => self.enter_windows(),
            (Zone::Desktops, Key::Return) | (Zone::Desktops, Key::KP_Enter) | (Zone::Desktops, Key::space) => {
                let (dsel, count) = {
                    let st = self.st.borrow();
                    (st.dsel, st.count)
                };
                match dsel {
                    Some(i) if (i as i64) < count => self.switch_desktop(i as i64 + 1),
                    Some(_) => self.new_desktop(),
                    None => {}
                }
            }
            (Zone::Desktops, Key::Delete) => {
                let (dsel, count) = {
                    let st = self.st.borrow();
                    (st.dsel, st.count)
                };
                if let Some(i) = dsel {
                    if (i as i64) < count && count > 1 {
                        self.close_desktop(i as i64 + 1);
                    }
                }
            }
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    fn move_sel(self: &Rc<Self>, delta: i32) {
        let (n, cur) = {
            let st = self.st.borrow();
            (st.cards.len(), st.sel)
        };
        if n == 0 {
            self.enter_desktops();
            return;
        }
        let next = match cur {
            None => 0,
            Some(i) => (i as i32 + delta).clamp(0, n as i32 - 1) as usize,
        };
        self.set_sel(Some(next));
    }

    fn select_first_or_last(self: &Rc<Self>, first: bool) {
        let n = self.st.borrow().cards.len();
        if n > 0 {
            self.set_sel(Some(if first { 0 } else { n - 1 }));
        }
    }

    fn move_vertical(self: &Rc<Self>, dir: i32) {
        let (target, none_selected) = {
            let st = self.st.borrow();
            match st.sel.and_then(|i| st.cards.get(i)) {
                None => (if st.cards.is_empty() { None } else { Some(0) }, true),
                Some(cur) => {
                    let want = cur.row as i32 + dir;
                    let best = st
                        .cards
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| c.row as i32 == want)
                        .min_by(|a, b| {
                            (a.1.cx - cur.cx)
                                .abs()
                                .partial_cmp(&(b.1.cx - cur.cx).abs())
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .map(|(i, _)| i);
                    (best, false)
                }
            }
        };
        match target {
            Some(i) => self.set_sel(Some(i)),
            None if dir > 0 || none_selected => self.enter_desktops(),
            None => {}
        }
    }

    fn enter_desktops(self: &Rc<Self>) {
        self.set_sel(None);
        let idx = {
            let mut st = self.st.borrow_mut();
            st.zone = Zone::Desktops;
            (st.current - 1).max(0) as usize
        };
        self.set_dsel(Some(idx));
    }

    fn enter_windows(self: &Rc<Self>) {
        self.set_dsel(None);
        self.st.borrow_mut().zone = Zone::Windows;
        let has = !self.st.borrow().cards.is_empty();
        if has {
            self.set_sel(Some(0));
        }
    }

    fn move_dsel(self: &Rc<Self>, delta: i32) {
        let (n, cur, count) = {
            let st = self.st.borrow();
            (st.desks.len(), st.dsel, st.count)
        };
        if n == 0 {
            return;
        }
        let next = (cur.unwrap_or(0) as i32 + delta).clamp(0, n as i32 - 1) as usize;
        self.set_dsel(Some(next));
        if (next as i64) < count {
            self.preview(next as i64 + 1);
        }
    }
}
