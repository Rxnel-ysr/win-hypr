//! The toast window and the stack of live toasts.
//!
//! RAM strategy: the layer-shell window, its widgets and its Wayland buffers only exist while at
//! least one toast is visible. When the last toast goes away the window is destroyed and the
//! heap is trimmed, so the resident daemon idles with nothing but the D-Bus thread and history.

use crate::card;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use w11_common::config::StoredNotification;
use w11_common::mem;
use w11_common::popup::{FLYOUT_GAP, TASKBAR_HEIGHT};

/// Oldest toast is evicted beyond this many on screen (bounds widgets and memory).
const MAX_VISIBLE: usize = 4;
const DEFAULT_MS: u64 = 6_000;
const MAX_MS: u64 = 30_000;
const STACK_SPACING: i32 = 8;

struct Ui {
    window: gtk::ApplicationWindow,
    stack: gtk::Box,
}

impl Ui {
    fn new(app: &gtk::Application) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .decorated(false)
            .resizable(false)
            .build();
        window.add_css_class("w11-window");
        window.init_layer_shell();
        window.set_namespace("w11-toast");
        window.set_layer(Layer::Overlay);
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_exclusive_zone(-1);
        window.set_anchor(Edge::Bottom, true);
        window.set_anchor(Edge::Right, true);
        window.set_margin(Edge::Bottom, TASKBAR_HEIGHT + FLYOUT_GAP);
        window.set_margin(Edge::Right, FLYOUT_GAP);

        let stack = gtk::Box::new(gtk::Orientation::Vertical, STACK_SPACING);
        window.set_child(Some(&stack));
        Self { window, stack }
    }
}

struct Live {
    id: u32,
    card: gtk::Box,
    /// `None` once the timer has fired (a fired source must not be removed again).
    timer: Option<glib::SourceId>,
}

pub struct Toasts {
    app: gtk::Application,
    ui: RefCell<Option<Ui>>,
    /// Oldest first; at most `MAX_VISIBLE` entries, so a Vec beats a map.
    live: RefCell<Vec<Live>>,
}

impl Toasts {
    pub fn new(app: &gtk::Application) -> Rc<Self> {
        Rc::new(Self {
            app: app.clone(),
            ui: RefCell::new(None),
            live: RefCell::new(Vec::new()),
        })
    }

    pub fn show(self: &Rc<Self>, n: &StoredNotification, timeout_ms: i32) {
        let id = n.id;
        self.remove_card(id); // a replaced notification swaps in place
        while self.live.borrow().len() >= MAX_VISIBLE {
            let oldest = self.live.borrow()[0].id;
            self.remove_card(oldest);
        }

        let (stack, window) = {
            let mut ui = self.ui.borrow_mut();
            let u = ui.get_or_insert_with(|| Ui::new(&self.app));
            (u.stack.clone(), u.window.clone())
        };

        let card = card::build(n);
        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(self);
        click.connect_released(move |_, _, _, _| {
            if let Some(t) = weak.upgrade() {
                t.close(id);
            }
        });
        card.add_controller(click);
        stack.prepend(&card);

        // expire_timeout: -1 = server default, 0 = "never" in the spec; we still expire it.
        let ms = if timeout_ms > 0 {
            (timeout_ms as u64).min(MAX_MS)
        } else {
            DEFAULT_MS
        };
        let weak = Rc::downgrade(self);
        let timer = glib::timeout_add_local_once(Duration::from_millis(ms), move || {
            if let Some(t) = weak.upgrade() {
                t.expired(id);
            }
        });
        self.live.borrow_mut().push(Live {
            id,
            card,
            timer: Some(timer),
        });

        if !window.is_visible() {
            window.present();
        }
    }

    /// Dismiss a toast (click, `CloseNotification`). Unknown ids are ignored.
    pub fn close(&self, id: u32) {
        if !self.remove_card(id) {
            return;
        }
        if self.live.borrow().is_empty() {
            self.release_window();
        }
    }

    fn expired(&self, id: u32) {
        if let Some(l) = self.live.borrow_mut().iter_mut().find(|l| l.id == id) {
            l.timer = None;
        }
        self.close(id);
    }

    fn remove_card(&self, id: u32) -> bool {
        let live = {
            let mut v = self.live.borrow_mut();
            match v.iter().position(|l| l.id == id) {
                Some(i) => v.remove(i),
                None => return false,
            }
        };
        if let Some(t) = live.timer {
            t.remove();
        }
        if let Some(ui) = self.ui.borrow().as_ref() {
            ui.stack.remove(&live.card);
        }
        true
    }

    /// Last toast gone: hide now, destroy on the next idle (we may be inside one of its own
    /// event handlers), then hand the freed pages back to the kernel.
    fn release_window(&self) {
        if let Some(ui) = self.ui.borrow_mut().take() {
            ui.window.set_visible(false);
            glib::idle_add_local_once(move || ui.window.destroy());
        }
        mem::trim_soon();
    }
}
