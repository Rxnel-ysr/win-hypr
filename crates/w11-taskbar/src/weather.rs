//! wttr.in temperature button (left side of the taskbar).

use crate::widgets;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::rc::Rc;
use w11_common::config::Config;
use w11_common::sys;

const REFRESH_SECS: u32 = 1800;
const RETRY_SECS: u32 = 30;
const MAX_RETRIES: u32 = 5;

pub fn build(cfg: &Config) -> Option<gtk::Button> {
    if !cfg.weather {
        return None;
    }
    let button = widgets::tray_button();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&widgets::symbolic("weather-few-clouds-symbolic", 22));
    let label = gtk::Label::new(Some("Widgets"));
    label.add_css_class("tb-weather-temp");
    row.append(&label);
    button.set_child(Some(&row));
    widgets::opens(&button, "w11-flyout calendar");

    let loc: Rc<str> = cfg.weather_location.replace(' ', "+").into();
    refresh(label.clone(), loc.clone(), 0);
    glib::timeout_add_seconds_local(REFRESH_SECS, move || {
        refresh(label.clone(), loc.clone(), 0);
        glib::ControlFlow::Continue
    });
    Some(button)
}

/// Fetch off-thread. If the network isn't up yet (typical right after login), retry shortly
/// instead of waiting for the next 30-minute tick.
fn refresh(label: gtk::Label, loc: Rc<str>, attempt: u32) {
    glib::spawn_future_local(async move {
        let url = format!("https://wttr.in/{loc}?format=%t");
        let res = gio::spawn_blocking(move || {
            sys::run("curl", &["-sf", "--max-time", "6", url.as_str()])
        })
        .await;
        // wttr returns e.g. "+23°C"; ignore HTML/error responses
        let ok = match &res {
            Ok(t) if !t.is_empty() && t.len() < 12 && !t.contains('<') => {
                label.set_text(t.trim_start_matches('+'));
                true
            }
            _ => false,
        };
        if ok {
            w11_common::mem::trim_soon();
        } else if attempt < MAX_RETRIES {
            glib::timeout_add_seconds_local_once(RETRY_SECS, move || {
                refresh(label, loc, attempt + 1)
            });
        }
    });
}
