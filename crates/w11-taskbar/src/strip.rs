//! The app strip: pinned apps first, then running unpinned apps grouped by window class.
//!
//! Two update paths:
//!   * structure changed (window opened/closed, app (un)pinned) -> rebuild the buttons
//!   * only focus / titles changed                              -> flip indicator classes
//! Buttons never capture window lists: click handlers and tooltips read the live state by
//! index, so focus changes cost almost nothing.

use crate::menu;
use crate::resolver::Resolver;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;
use w11_common::apps;
use w11_common::config::Config;
use w11_common::hypr::{self, Client};

#[derive(Clone, PartialEq, Eq)]
enum Key {
    Pinned(String),
    Class(String),
}

struct Group {
    key: Key,
    info: Option<gio::AppInfo>,
    wins: Vec<Client>,
}

impl Group {
    fn name(&self) -> String {
        match (&self.info, &self.key) {
            (Some(i), _) => i.name().to_string(),
            (None, Key::Class(c)) => c.clone(),
            (None, Key::Pinned(id)) => id.clone(),
        }
    }
}

struct Slot {
    indicator: gtk::Box,
}

/// What the right-click menu needs.
pub struct MenuInfo {
    pub name: String,
    pub info: Option<gio::AppInfo>,
    pub pinned: bool,
    pub addrs: Vec<String>,
}

pub struct Strip {
    pub root: gtk::Box,
    cfg: RefCell<Config>,
    resolver: Resolver,
    groups: RefCell<Vec<Group>>,
    slots: RefCell<Vec<Slot>>,
    layout: RefCell<String>,
    active: RefCell<String>,
    menu: RefCell<Option<gtk::Popover>>,
    seq: Cell<u64>,
    pending: Cell<bool>,
}

/// Hyprland events that can change what the strip shows. v1 events are skipped because
/// Hyprland emits the v2 twin for each of them; title events are skipped because tooltips
/// are computed lazily.
fn is_relevant(ev: &str) -> bool {
    matches!(
        ev,
        "openwindow"
            | "closewindow"
            | "activewindowv2"
            | "movewindowv2"
            | "workspacev2"
            | "focusedmon"
    )
}

fn layout_sig(groups: &[Group]) -> String {
    let mut s = String::with_capacity(groups.len() * 48);
    for g in groups {
        match &g.key {
            Key::Pinned(id) => {
                s.push('P');
                s.push_str(id);
            }
            Key::Class(c) => {
                s.push('C');
                s.push_str(c);
            }
        }
        for w in &g.wins {
            s.push(':');
            s.push_str(&w.address);
        }
        s.push('|');
    }
    s
}

impl Strip {
    pub fn new(cfg: Config) -> Rc<Self> {
        Rc::new(Self {
            root: gtk::Box::new(gtk::Orientation::Horizontal, 0),
            cfg: RefCell::new(cfg),
            resolver: Resolver::default(),
            groups: RefCell::default(),
            slots: RefCell::default(),
            layout: RefCell::default(),
            active: RefCell::default(),
            menu: RefCell::default(),
            seq: Cell::new(0),
            pending: Cell::new(false),
        })
    }

    // ------------------------------------------------------------ lifecycle

    pub fn start(self: &Rc<Self>) {
        self.refresh();

        // Hyprland events -> debounced refresh (a burst of events = one refresh)
        let (tx, rx) = async_channel::unbounded::<String>();
        hypr::listen_events(tx);
        let me = self.clone();
        glib::spawn_future_local(async move {
            while let Ok(ev) = rx.recv().await {
                if is_relevant(&ev) && !me.pending.replace(true) {
                    let me = me.clone();
                    glib::timeout_add_local_once(Duration::from_millis(40), move || {
                        me.pending.set(false);
                        me.refresh();
                    });
                }
            }
        });

        // Safety net in case an event is missed (also heals windows that set their class late).
        let me = self.clone();
        glib::timeout_add_seconds_local(120, move || {
            me.refresh();
            glib::ControlFlow::Continue
        });
    }

    pub fn apps_changed(self: &Rc<Self>) {
        self.resolver.clear();
        self.layout.borrow_mut().clear(); // force a rebuild so icons are re-resolved
        self.refresh();
    }

    /// Query Hyprland off the UI thread. Stale answers are discarded.
    pub fn refresh(self: &Rc<Self>) {
        let seq = self.seq.get().wrapping_add(1);
        self.seq.set(seq);
        let me = self.clone();
        glib::spawn_future_local(async move {
            let Ok((clients, active)) = gio::spawn_blocking(|| {
                (
                    hypr::clients(),
                    hypr::active_window().map(|c| c.address).unwrap_or_default(),
                )
            })
            .await
            else {
                return;
            };
            if me.seq.get() == seq {
                me.apply(clients, active);
            }
        });
    }

    fn apply(self: &Rc<Self>, clients: Vec<Client>, active: String) {
        let groups = self.group(clients);
        self.resolver.release();
        let layout = layout_sig(&groups);
        let rebuild = *self.layout.borrow() != layout;
        *self.groups.borrow_mut() = groups;
        *self.active.borrow_mut() = active;
        if rebuild {
            *self.layout.borrow_mut() = layout;
            self.rebuild();
        } else {
            self.sync();
        }
    }

    // ------------------------------------------------------------- grouping

    fn group(&self, clients: Vec<Client>) -> Vec<Group> {
        let cfg = self.cfg.borrow();
        let mut used = vec![false; clients.len()];
        let mut out: Vec<Group> = Vec::with_capacity(cfg.pinned.len() + 4);

        for id in &cfg.pinned {
            let Some(info) = self.resolver.by_id(id) else {
                continue;
            };
            let mut wins = Vec::new();
            for (i, c) in clients.iter().enumerate() {
                if !used[i] && apps::class_matches(&info, &c.class) {
                    used[i] = true;
                    wins.push(c.clone());
                }
            }
            out.push(Group {
                key: Key::Pinned(id.clone()),
                info: Some(info),
                wins,
            });
        }

        let mut order: Vec<&str> = Vec::new();
        for (i, c) in clients.iter().enumerate() {
            if !used[i] && !order.contains(&c.class.as_str()) {
                order.push(c.class.as_str());
            }
        }
        for class in order {
            let wins: Vec<Client> = clients
                .iter()
                .enumerate()
                .filter(|(i, c)| !used[*i] && c.class == class)
                .map(|(_, c)| c.clone())
                .collect();
            out.push(Group {
                key: Key::Class(class.to_string()),
                info: self.resolver.for_class(class),
                wins,
            });
        }
        out
    }

    // ------------------------------------------------------------- widgets

    fn rebuild(self: &Rc<Self>) {
        self.drop_menu();
        while let Some(c) = self.root.first_child() {
            self.root.remove(&c);
        }
        let n = self.groups.borrow().len();
        let mut slots = Vec::with_capacity(n);
        for idx in 0..n {
            let slot = {
                let groups = self.groups.borrow();
                self.make_slot(idx, &groups[idx])
            };
            slots.push(slot);
        }
        *self.slots.borrow_mut() = slots;
        self.sync();
        w11_common::mem::trim_soon();
    }

    fn make_slot(self: &Rc<Self>, idx: usize, g: &Group) -> Slot {
        let overlay = gtk::Overlay::new();
        let button = gtk::Button::new();
        button.add_css_class("tb-button");
        button.set_width_request(44);

        let image = match &g.info {
            Some(i) => gtk::Image::from_gicon(&apps::icon_of(i)),
            None => {
                let name = g.name().to_lowercase();
                let theme = gtk::IconTheme::for_display(&button.display());
                let icon = if theme.has_icon(&name) {
                    name.as_str()
                } else {
                    "application-x-executable"
                };
                gtk::Image::from_icon_name(icon)
            }
        };
        image.set_pixel_size(24);
        button.set_child(Some(&image));
        overlay.set_child(Some(&button));

        // Tooltip text is computed on hover from live state (titles change constantly).
        button.set_has_tooltip(true);
        {
            let me = self.clone();
            button.connect_query_tooltip(move |_, _, _, _, tip| match me.tooltip(idx) {
                Some(t) => {
                    tip.set_text(Some(&t));
                    true
                }
                None => false,
            });
        }

        let indicator = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        indicator.add_css_class("tb-indicator");
        indicator.set_halign(gtk::Align::Center);
        indicator.set_valign(gtk::Align::End);
        indicator.set_can_target(false);
        indicator.set_visible(false);
        overlay.add_overlay(&indicator);

        // left click
        {
            let me = self.clone();
            button.connect_clicked(move |_| me.click(idx));
        }
        // middle click: new window
        {
            let me = self.clone();
            let mid = gtk::GestureClick::builder().button(2).build();
            mid.connect_pressed(move |_, _, _, _| me.launch(idx));
            button.add_controller(mid);
        }
        // right click: jump menu (built on demand)
        {
            let me = self.clone();
            let anchor = overlay.downgrade();
            let right = gtk::GestureClick::builder().button(3).build();
            right.connect_pressed(move |_, _, _, _| {
                if let Some(a) = anchor.upgrade() {
                    menu::show(&me, idx, &a);
                }
            });
            button.add_controller(right);
        }

        self.root.append(&overlay);
        Slot { indicator }
    }

    /// Cheap path: only running/active indicators.
    fn sync(&self) {
        let active = self.active.borrow();
        let groups = self.groups.borrow();
        let slots = self.slots.borrow();
        for (g, s) in groups.iter().zip(slots.iter()) {
            let running = !g.wins.is_empty();
            s.indicator.set_visible(running);
            if running && g.wins.iter().any(|w| w.address == *active) {
                s.indicator.add_css_class("active");
            } else {
                s.indicator.remove_css_class("active");
            }
        }
    }

    // ------------------------------------------------------------- actions

    fn tooltip(&self, idx: usize) -> Option<String> {
        let groups = self.groups.borrow();
        let g = groups.get(idx)?;
        Some(match g.wins.as_slice() {
            [w] if !w.title.is_empty() => w.title.clone(),
            [] | [_] => g.name(),
            ws => format!("{} ({} windows)", g.name(), ws.len()),
        })
    }

    fn click(&self, idx: usize) {
        let (wins, info) = {
            let groups = self.groups.borrow();
            let Some(g) = groups.get(idx) else { return };
            (g.wins.clone(), g.info.clone())
        };
        if wins.is_empty() {
            if let Some(i) = &info {
                apps::launch(i);
            }
            return;
        }
        let active = self.active.borrow().clone();
        std::thread::spawn(move || click_windows(&wins, &active));
    }

    fn launch(&self, idx: usize) {
        let info = self.groups.borrow().get(idx).and_then(|g| g.info.clone());
        if let Some(i) = info {
            apps::launch(&i);
        }
    }

    // ---------------------------------------------------------------- menu

    pub fn menu_info(&self, idx: usize) -> Option<MenuInfo> {
        let groups = self.groups.borrow();
        let g = groups.get(idx)?;
        Some(MenuInfo {
            name: g.name(),
            info: g.info.clone(),
            pinned: matches!(g.key, Key::Pinned(_)),
            addrs: g.wins.iter().map(|w| w.address.clone()).collect(),
        })
    }

    pub fn toggle_pin(self: &Rc<Self>, info: &gio::AppInfo) {
        let id = apps::desktop_id(info);
        {
            let mut cfg = self.cfg.borrow_mut();
            if let Some(pos) = cfg.pinned.iter().position(|x| *x == id) {
                cfg.pinned.remove(pos);
            } else {
                cfg.pinned.push(id);
            }
            cfg.save();
        }
        self.refresh();
    }

    pub fn track_menu(&self, p: gtk::Popover) {
        *self.menu.borrow_mut() = Some(p);
    }

    /// Detach and forget the open menu (if any). Safe to call repeatedly.
    pub fn drop_menu(&self) {
        if let Some(p) = self.menu.borrow_mut().take() {
            p.popdown();
            p.unparent();
        }
    }
}

/// Windows semantics for clicking a taskbar button that has open windows.
fn click_windows(wins: &[Client], active: &str) {
    let mut sorted = wins.to_vec();
    sorted.sort_by_key(|w| w.focus_history_id);
    match sorted.iter().position(|w| w.address == active) {
        // one window and it's focused -> minimise; several -> cycle to the next
        Some(_) if sorted.len() == 1 => {
            hypr::minimize(&sorted[0].address);
        }
        Some(i) => {
            hypr::restore_and_focus(&sorted[(i + 1) % sorted.len()]);
        }
        None => {
            hypr::restore_and_focus(&sorted[0]);
        }
    }
}
