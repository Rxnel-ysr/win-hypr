//! D-Bus side: implements `org.freedesktop.Notifications` on its own small thread.
//! No GTK in here; everything is forwarded to the UI thread through a bounded channel.

use crate::text;
use async_channel::Sender;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use w11_common::config::StoredNotification;

const MAX_APP: usize = 64;
const MAX_SUMMARY: usize = 160;
const MAX_BODY: usize = 600;
/// zbus' blocking connection needs very little stack; the default 8 MiB is only address space,
/// but a small cap keeps the thread's footprint predictable.
const THREAD_STACK: usize = 512 * 1024;

pub enum Msg {
    Show(StoredNotification, i32),
    Close(u32),
}

struct Server {
    tx: Sender<Msg>,
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
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        _actions: Vec<String>,
        _hints: HashMap<String, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.next_id = self.next_id.wrapping_add(1).max(1);
            self.next_id
        };
        let n = StoredNotification {
            id,
            app: text::clip(app_name, MAX_APP),
            summary: text::clean(summary, MAX_SUMMARY),
            body: text::clean(body, MAX_BODY),
            icon: app_icon.to_string(),
            time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64),
        };
        let _ = self.tx.send_blocking(Msg::Show(n, expire_timeout));
        id
    }

    fn close_notification(&self, id: u32) {
        let _ = self.tx.send_blocking(Msg::Close(id));
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "w11-notify".into(),
            "win11-hypr".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }
}

/// Start the D-Bus thread. If the bus name is taken the process exits: a daemon that can never
/// receive anything is just wasted RAM.
pub fn spawn(tx: Sender<Msg>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("w11-notify-dbus".into())
        .stack_size(THREAD_STACK)
        .spawn(move || {
            let server = Server { tx, next_id: 0 };
            let conn = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.name("org.freedesktop.Notifications"))
                .and_then(|b| b.serve_at("/org/freedesktop/Notifications", server))
                .and_then(|b| b.build());
            match conn {
                Ok(_conn) => loop {
                    std::thread::park(); // the connection must stay alive
                },
                Err(e) => {
                    eprintln!(
                        "w11-notify: could not own org.freedesktop.Notifications: {e}\n\
                         (is another notification daemon such as mako/dunst/swaync running?)"
                    );
                    std::process::exit(1);
                }
            }
        })
        .map(|_| ())
}
