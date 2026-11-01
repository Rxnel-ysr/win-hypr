//! Minimal StatusNotifier host: watcher service + icon fetching + click actions.
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};
use zbus::message::Header;

/// (bus name, object path)
pub type Items = Arc<Mutex<Vec<(String, String)>>>;

struct Watcher {
    items: Items,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(&self, service: &str, #[zbus(header)] hdr: Header<'_>) {
        let (svc, path) = if service.starts_with('/') {
            (
                hdr.sender().map(|s| s.to_string()).unwrap_or_default(),
                service.to_string(),
            )
        } else {
            (service.to_string(), "/StatusNotifierItem".to_string())
        };
        if svc.is_empty() {
            return;
        }
        let mut v = self.items.lock().unwrap();
        if !v.iter().any(|(s, p)| *s == svc && *p == path) {
            v.push((svc, path));
        }
    }

    fn register_status_notifier_host(&self, _service: &str) {}

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .map(|(s, p)| format!("{s}{p}"))
            .collect()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

/// Claim the watcher name on a background thread and return the shared item list.
pub fn start() -> Items {
    let items: Items = Arc::default();
    let w = Watcher {
        items: items.clone(),
    };
    std::thread::spawn(move || {
        let conn = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name("org.kde.StatusNotifierWatcher"))
            .and_then(|b| b.name(format!("org.kde.StatusNotifierHost-{}", std::process::id())))
            .and_then(|b| b.serve_at("/StatusNotifierWatcher", w))
            .and_then(|b| b.build());
        match conn {
            Ok(_c) => loop {
                std::thread::park();
            },
            Err(e) => {
                eprintln!("w11-taskbar: tray watcher unavailable: {e} (another bar running?)")
            }
        }
    });
    items
}

fn conn() -> Option<zbus::blocking::Connection> {
    static C: OnceLock<Option<zbus::blocking::Connection>> = OnceLock::new();
    C.get_or_init(|| zbus::blocking::Connection::session().ok())
        .clone()
}

#[derive(Clone)]
pub struct ItemInfo {
    pub service: String,
    pub path: String,
    pub title: String,
    pub icon_name: String,
    /// width, height, RGBA bytes
    pub pixmap: Option<(i32, i32, Vec<u8>)>,
}

fn fetch(service: &str, path: &str) -> Option<ItemInfo> {
    let c = conn()?;
    let p = zbus::blocking::Proxy::new(
        &c,
        service.to_string(),
        path.to_string(),
        "org.kde.StatusNotifierItem",
    )
    .ok()?;
    let id: String = p.get_property("Id").ok()?; // fails => the app is gone
    let title: String = p.get_property("Title").unwrap_or_default();
    let title = if title.is_empty() { id } else { title };
    let icon_name: String = p.get_property("IconName").unwrap_or_default();
    let px: Vec<(i32, i32, Vec<u8>)> = p.get_property("IconPixmap").unwrap_or_default();
    let pixmap = px
        .into_iter()
        .max_by_key(|(w, h, _)| w * h)
        .and_then(|(w, h, d)| {
            if w <= 0 || h <= 0 || d.len() < (w * h * 4) as usize {
                return None;
            }
            // ARGB32 (network order) -> RGBA
            let mut out = Vec::with_capacity(d.len());
            for px in d.chunks_exact(4) {
                out.extend_from_slice(&[px[1], px[2], px[3], px[0]]);
            }
            Some((w, h, out))
        });
    Some(ItemInfo {
        service: service.into(),
        path: path.into(),
        title,
        icon_name,
        pixmap,
    })
}

fn call(info: &ItemInfo, method: &'static str) {
    let (s, p) = (info.service.clone(), info.path.clone());
    std::thread::spawn(move || {
        if let Some(c) = conn() {
            let _ = c.call_method(
                Some(s.as_str()),
                p.as_str(),
                Some("org.kde.StatusNotifierItem"),
                method,
                &(0i32, 0i32),
            );
        }
    });
}

fn make_button(info: &ItemInfo, pop: &gtk::Popover) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("tray-button");
    b.set_tooltip_text(Some(&info.title));
    let img = match &info.pixmap {
        Some((w, h, d)) if info.icon_name.is_empty() => {
            let tex = gdk::MemoryTexture::new(
                *w,
                *h,
                gdk::MemoryFormat::R8g8b8a8,
                &glib::Bytes::from(d.as_slice()),
                (*w * 4) as usize,
            );
            gtk::Image::from_paintable(Some(&tex))
        }
        _ if !info.icon_name.is_empty() => gtk::Image::from_icon_name(&info.icon_name),
        _ => gtk::Image::from_icon_name("application-x-executable"),
    };
    img.set_pixel_size(20);
    b.set_child(Some(&img));

    let (i, p) = (info.clone(), pop.clone());
    b.connect_clicked(move |_| {
        p.popdown();
        call(&i, "Activate");
    });
    for (btn, method) in [(2u32, "SecondaryActivate"), (3, "ContextMenu")] {
        let (i, p) = (info.clone(), pop.clone());
        let g = gtk::GestureClick::builder().button(btn).build();
        g.connect_pressed(move |_, _, _, _| {
            p.popdown();
            call(&i, method);
        });
        b.add_controller(g);
    }
    b
}

fn refresh(items: Items, flow: gtk::FlowBox, pop: gtk::Popover, sig: Rc<RefCell<String>>) {
    let list = items.lock().unwrap().clone();
    glib::spawn_future_local(async move {
        let l2 = list.clone();
        let Ok(res) =
            gio::spawn_blocking(move || l2.iter().map(|(s, p)| fetch(s, p)).collect::<Vec<_>>())
                .await
        else {
            return;
        };

        let dead: Vec<_> = list
            .iter()
            .zip(res.iter())
            .filter(|(_, r)| r.is_none())
            .map(|(k, _)| k.clone())
            .collect();
        if !dead.is_empty() {
            items.lock().unwrap().retain(|k| !dead.contains(k));
        }
        let infos: Vec<ItemInfo> = res.into_iter().flatten().collect();
        let new_sig: String = infos
            .iter()
            .map(|i| {
                format!(
                    "{}{}{}{}|",
                    i.service,
                    i.title,
                    i.icon_name,
                    i.pixmap.as_ref().map(|p| p.2.len()).unwrap_or(0)
                )
            })
            .collect();
        if *sig.borrow() == new_sig {
            return;
        }
        *sig.borrow_mut() = new_sig;
        while let Some(c) = flow.first_child() {
            flow.remove(&c);
        }
        if infos.is_empty() {
            let l = gtk::Label::new(Some("No background apps"));
            l.add_css_class("notif-app");
            l.set_margin_start(12);
            l.set_margin_end(12);
            l.set_margin_top(8);
            l.set_margin_bottom(8);
            flow.insert(&l, -1);
        }
        for i in &infos {
            flow.insert(&make_button(i, &pop), -1);
        }
    });
}

/// Fill `flow` (inside `pop`) with tray icons and keep it fresh.
pub fn attach(items: Items, flow: &gtk::FlowBox, pop: &gtk::Popover) {
    let sig = Rc::new(RefCell::new(String::from("?")));
    let (i, f, p, s) = (items.clone(), flow.clone(), pop.clone(), sig.clone());
    refresh(i, f, p, s);
    let (f, p) = (flow.clone(), pop.clone());
    glib::timeout_add_seconds_local(10, move || {
        refresh(items.clone(), f.clone(), p.clone(), sig.clone());
        glib::ControlFlow::Continue
    });
}
