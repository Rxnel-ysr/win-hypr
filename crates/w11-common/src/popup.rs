//! Windows-style flyouts on top of wlr-layer-shell: one primary instance per flyout,
//! every re-launch of the binary toggles it (just like pressing the Start button twice).

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;

pub const TASKBAR_HEIGHT: i32 = 48;
/// Gap between the taskbar and a flyout.
pub const FLYOUT_GAP: i32 = 12;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    BottomCenter,
    BottomRight,
    Fullscreen,
}

pub struct Popup {
    pub window: gtk::ApplicationWindow,
    scrim: Option<gtk::ApplicationWindow>,
}

impl Popup {
    pub fn new(
        app: &gtk::Application,
        namespace: &str,
        width: i32,
        height: i32,
        placement: Placement,
    ) -> Rc<Self> {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .decorated(false)
            .resizable(false)
            .build();
        window.add_css_class("w11-window");
        window.init_layer_shell();
        window.set_namespace(namespace);
        window.set_layer(Layer::Overlay);
        window.set_keyboard_mode(KeyboardMode::Exclusive);

        match placement {
            Placement::BottomCenter => {
                window.set_anchor(Edge::Bottom, true);
                window.set_margin(Edge::Bottom, TASKBAR_HEIGHT + FLYOUT_GAP);
                window.set_default_size(width, height);
            }
            Placement::BottomRight => {
                window.set_anchor(Edge::Bottom, true);
                window.set_anchor(Edge::Right, true);
                window.set_margin(Edge::Bottom, TASKBAR_HEIGHT + FLYOUT_GAP);
                window.set_margin(Edge::Right, FLYOUT_GAP);
                window.set_default_size(width, height);
            }
            Placement::Fullscreen => {
                for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
                    window.set_anchor(e, true);
                }
                window.set_margin(Edge::Bottom, TASKBAR_HEIGHT);
                window.set_exclusive_zone(-1);
            }
        }

        // Transparent click-catcher behind the flyout: clicking outside closes it.
        let scrim = if placement == Placement::Fullscreen {
            None
        } else {
            let s = gtk::ApplicationWindow::builder()
                .application(app)
                .decorated(false)
                .build();
            s.add_css_class("w11-window");
            s.init_layer_shell();
            s.set_namespace("w11-scrim");
            s.set_layer(Layer::Top);
            s.set_keyboard_mode(KeyboardMode::None);
            s.set_exclusive_zone(-1);
            for e in [Edge::Top, Edge::Left, Edge::Right, Edge::Bottom] {
                s.set_anchor(e, true);
            }
            s.set_margin(Edge::Bottom, TASKBAR_HEIGHT);
            let filler = gtk::Box::new(gtk::Orientation::Vertical, 0);
            filler.add_css_class("scrim");
            s.set_child(Some(&filler));
            let click = gtk::GestureClick::new();
            let (w, sc) = (window.clone(), s.clone());
            click.connect_pressed(move |_, _, _, _| {
                w.set_visible(false);
                sc.set_visible(false);
            });
            s.add_controller(click);
            Some(s)
        };

        let key = gtk::EventControllerKey::new();
        let (w, sc) = (window.clone(), scrim.clone());
        key.connect_key_pressed(move |_, k, _, _| {
            if k == gtk::gdk::Key::Escape {
                w.set_visible(false);
                if let Some(s) = &sc {
                    s.set_visible(false);
                }
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        window.add_controller(key);

        Rc::new(Self { window, scrim })
    }

    pub fn show(&self) {
        if let Some(s) = &self.scrim {
            s.present();
        }
        self.window.present();
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
        if let Some(s) = &self.scrim {
            s.set_visible(false);
        }
    }

    pub fn toggle(&self) {
        if self.window.is_visible() {
            self.hide();
        } else {
            self.show();
        }
    }
}

/// Run a single-instance flyout application. `build` is called once, on first activation.
pub fn run_popup<F>(app_id: &str, build: F) -> gtk::glib::ExitCode
where
    F: Fn(&gtk::Application) -> Rc<Popup> + 'static,
{
    let app = gtk::Application::builder().application_id(app_id).build();
    app.connect_startup(|_| crate::theme::apply());
    let slot: Rc<RefCell<Option<Rc<Popup>>>> = Rc::new(RefCell::new(None));
    app.connect_activate(move |app| {
        let existing = slot.borrow().clone();
        let popup = match existing {
            Some(p) => p,
            None => {
                let p = build(app);
                *slot.borrow_mut() = Some(p.clone());
                std::mem::forget(app.hold());
                p
            }
        };
        popup.toggle();
    });
    let prog = std::env::args().next().unwrap_or_else(|| app_id.to_string());
    app.run_with_args(&[prog.as_str()])
}
