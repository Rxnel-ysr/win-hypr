//! Window thumbnails for Task View.
//!
//! Wayland has no portable "give me this window's pixels" call, so we cache them: whenever a
//! window becomes the active (= top-most, fully visible) window we grab its rectangle with
//! `grim` and store a small JPEG in the session runtime dir. Task View just reads that cache.

use crate::{desktops, hypr};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn dir() -> PathBuf {
    let d = desktops::runtime_dir().join("thumbs");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn path_for(addr: &str) -> PathBuf {
    dir().join(format!("{}.jpg", addr.trim_start_matches("0x")))
}

/// Layer-shell surfaces that would end up *in* the screenshot.
const OVERLAYS: [&str; 5] = [
    "w11-start",
    "w11-quick",
    "w11-calendar",
    "w11-taskview",
    "w11-scrim",
];

fn overlay_open() -> bool {
    let s = String::from_utf8_lossy(&hypr::raw_json("layers")).into_owned();
    OVERLAYS.iter().any(|ns| s.contains(ns))
}

/// Grab the active window. Returns its address when a fresh thumbnail was stored.
pub fn capture_active() -> Option<String> {
    let c = hypr::active_window()?;
    if c.minimized() || c.workspace.id <= 0 || c.size[0] < 48 || c.size[1] < 48 {
        return None;
    }
    let (cx, cy) = (c.at[0] + c.size[0] / 2, c.at[1] + c.size[1] / 2);
    let mon = hypr::monitors().into_iter().find(|m| {
        let w = (m.width as f64 / m.scale).round() as i64;
        let h = (m.height as f64 / m.scale).round() as i64;
        cx >= m.x && cx < m.x + w && cy >= m.y && cy < m.y + h
    })?;
    // clip to the part of the monitor that isn't the taskbar
    let (ux, uy, uw, uh) = mon.usable();
    let x0 = c.at[0].max(ux);
    let y0 = c.at[1].max(uy);
    let x1 = (c.at[0] + c.size[0]).min(ux + uw);
    let y1 = (c.at[1] + c.size[1]).min(uy + uh);
    let (w, h) = (x1 - x0, y1 - y0);
    if w < 48 || h < 48 || overlay_open() {
        return None;
    }
    let scale = (640.0 / w as f64).clamp(0.05, 1.0);
    let dest = path_for(&c.address);
    let tmp = dest.with_extension("tmp.jpg");
    let ok = Command::new("grim")
        .args(["-t", "jpeg", "-q", "85", "-s", &format!("{scale:.3}"), "-g"])
        .arg(format!("{x0},{y0} {w}x{h}"))
        .arg(&tmp)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok && std::fs::rename(&tmp, &dest).is_ok() {
        Some(c.address)
    } else {
        let _ = std::fs::remove_file(&tmp);
        None
    }
}

/// Drop thumbnails of windows that no longer exist.
pub fn gc() {
    let live: Vec<String> = hypr::clients()
        .iter()
        .map(|c| c.address.trim_start_matches("0x").to_string())
        .collect();
    let Ok(rd) = std::fs::read_dir(dir()) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let stem = stem.trim_end_matches(".tmp");
        if !live.iter().any(|a| a == stem) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Background thread: keeps the cache fresh. Cheap (one `grim` of one window per focus change).
pub fn spawn_daemon() {
    std::thread::spawn(|| {
        let (tx, rx) = async_channel::unbounded::<String>();
        hypr::listen_events(tx.clone());
        let ticker = tx.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(45));
            if ticker.send_blocking("tick".to_string()).is_err() {
                break;
            }
        });
        let mut last: Option<(String, Instant)> = None;
        while let Ok(ev) = rx.recv_blocking() {
            match ev.as_str() {
                "closewindow" => gc(),
                "activewindowv2" | "openwindow" | "movewindowv2" | "workspacev2" | "tick" => {
                    // let the focus/raise animation finish and coalesce event bursts
                    std::thread::sleep(Duration::from_millis(350));
                    while rx.try_recv().is_ok() {}
                    let stale = match (&last, hypr::active_window()) {
                        (Some((a, t)), Some(c)) => {
                            *a != c.address || t.elapsed() > Duration::from_secs(8)
                        }
                        _ => true,
                    };
                    if stale {
                        if let Some(addr) = capture_active() {
                            last = Some((addr, Instant::now()));
                        }
                    }
                }
                _ => {}
            }
        }
    });
}
