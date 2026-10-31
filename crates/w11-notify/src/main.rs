//! Windows 11 style toast notifications.
//!
//! Owns `org.freedesktop.Notifications` on the session bus (zbus), shows toasts above the
//! taskbar (bottom-right), and appends everything to the history that the notification
//! center in `w11-flyout calendar` displays.

use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use w11_common::config::{self, StoredNotification};
use w11_common::popup::{FLYOUT_GAP, TASKBAR_HEIGHT};
use w11_common::theme;

enum Msg {
    Show(StoredNotification, i32),
    Close(u32),
}

struct Server {
    tx: async_channel::Sender<Msg>,
    next_id: u32,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".into(), "icon-static".into(), "persistence".into()]
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &mut self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        _actions: Vec<String>,
        _hints: HashMap<String, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.next_id += 1;
            self.next_id
        };
        let time = glib::real_time() / 1_000_000;
        let n = StoredNotification { id, app: app_name, summary, body, icon: app_icon, time };
        let _ = self.tx.send_blocking(Msg::Show(n, expire_timeout));
        id
    }

    fn close_notification(&self, id: u32) {
        let _ = self.tx.send_blocking(Msg::Close(id));
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        ("w11-notify".into(), "win11-hypr".into(), env!("CARGO_PKG_VERSION").into(), "1.2".into())
    }
}

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("dev.win11hypr.Notify")
        .build();
    app.connect_startup(|_| theme::apply());
    app.connect_activate(build);
    app.run()
}

fn build(app: &gtk::Application) {
    if !app.windows().is_empty() {
        return;
    }
    std::mem::forget(app.hold());

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
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Right, true);
    window.set_margin(Edge::Bottom, TASKBAR_HEIGHT + FLYOUT_GAP);
    window.set_margin(Edge::Right, FLYOUT_GAP);

    let stack = gtk::Box::new(gtk::Orientation::Vertical, 8);
    window.set_child(Some(&stack));

    let (tx, rx) = async_channel::unbounded::<Msg>();
    let server = Server { tx, next_id: 0 };
    std::thread::spawn(move || {
        let conn = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name("org.freedesktop.Notifications"))
            .and_then(|b| b.serve_at("/org/freedesktop/Notifications", server))
            .and_then(|b| b.build());
        match conn {
            Ok(_c) => loop {
                std::thread::park();
            },
            Err(e) => eprintln!("w11-notify: could not own org.freedesktop.Notifications: {e}\n(another notification daemon such as mako/dunst running?)"),
        }
    });

    let toasts: Rc<RefCell<HashMap<u32, gtk::Widget>>> = Rc::new(RefCell::new(HashMap::new()));

    glib::spawn_future_local(async move {
        while let Ok(msg) = rx.recv().await {
            match msg {
                Msg::Show(n, timeout) => {
                    // history (always, even in do-not-disturb)
                    let mut hist = config::load_notifications();
                    hist.retain(|h| h.id != n.id);
                    hist.push(n.clone());
                    let len = hist.len();
                    if len > 60 {
                        hist.drain(0..len - 60);
                    }
                    config::save_notifications(&hist);

                    if config::dnd_enabled() {
                        continue;
                    }
                    if let Some(old) = toasts.borrow_mut().remove(&n.id) {
                        stack.remove(&old);
                    }
                    let card = toast_card(&n);
                    stack.prepend(&card);
                    toasts.borrow_mut().insert(n.id, card.clone().upcast());
                    window.present();

                    let ms = match timeout {
                        t if t > 0 => t as u64,
                        _ => 6000,
                    };
                    let (toasts2, stack2, window2, id) =
                        (toasts.clone(), stack.clone(), window.clone(), n.id);
                    let card_w: gtk::Widget = card.upcast();
                    let click_toasts = toasts.clone();
                    let click_stack = stack.clone();
                    let click_window = window.clone();
                    let click_widget = card_w.clone();
                    let gesture = gtk::GestureClick::new();
                    gesture.connect_released(move |_, _, _, _| {
                        dismiss(&click_toasts, &click_stack, &click_window, id, &click_widget);
                    });
                    card_w.add_controller(gesture);
                    glib::timeout_add_local_once(Duration::from_millis(ms), move || {
                        dismiss(&toasts2, &stack2, &window2, id, &card_w);
                    });
                }
                Msg::Close(id) => {
                    let w = toasts.borrow().get(&id).cloned();
                    if let Some(w) = w {
                        dismiss(&toasts, &stack, &window, id, &w);
                    }
                }
            }
        }
    });
}

fn dismiss(
    toasts: &Rc<RefCell<HashMap<u32, gtk::Widget>>>,
    stack: &gtk::Box,
    window: &gtk::ApplicationWindow,
    id: u32,
    widget: &gtk::Widget,
) {
    // only remove if this widget is still the live toast for that id (it may have been replaced)
    let live = toasts.borrow().get(&id).map(|w| w == widget).unwrap_or(false);
    if live {
        toasts.borrow_mut().remove(&id);
    }
    if widget.parent().is_some() {
        stack.remove(widget);
    }
    if toasts.borrow().is_empty() {
        window.set_visible(false);
    }
}

fn toast_card(n: &StoredNotification) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    card.add_css_class("toast");
    let icon = if n.icon.is_empty() {
        gtk::Image::from_icon_name("dialog-information-symbolic")
    } else if n.icon.starts_with('/') {
        gtk::Image::from_file(&n.icon)
    } else {
        gtk::Image::from_icon_name(&n.icon)
    };
    icon.set_pixel_size(32);
    icon.set_valign(gtk::Align::Start);
    let v = gtk::Box::new(gtk::Orientation::Vertical, 3);
    v.set_hexpand(true);
    let app = gtk::Label::new(Some(&n.app));
    app.add_css_class("notif-app");
    app.set_xalign(0.0);
    let sum = gtk::Label::new(Some(&n.summary));
    sum.add_css_class("notif-summary");
    sum.set_xalign(0.0);
    sum.set_wrap(true);
    sum.set_max_width_chars(36);
    v.append(&app);
    v.append(&sum);
    if !n.body.is_empty() {
        let b = gtk::Label::new(Some(&n.body));
        b.add_css_class("notif-body");
        b.set_xalign(0.0);
        b.set_wrap(true);
        b.set_max_width_chars(40);
        b.set_lines(4);
        b.set_ellipsize(gtk::pango::EllipsizeMode::End);
        v.append(&b);
    }
    card.append(&icon);
    card.append(&v);
    card
}
