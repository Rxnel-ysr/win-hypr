//! Thin Hyprland IPC layer.
//!
//! Hyprland >= 0.55 changed `hyprctl dispatch` to take a *Lua expression* that
//! evaluates to a dispatcher table (e.g. `hl.dsp.focus({ window = "address:0x1" })`).
//! Everything that mutates the compositor goes through [`dispatch`].

use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::time::Duration;

pub const MINIMIZED_WS: &str = "special:minimized";

/// Number of Windows-style virtual desktops.
pub const DESKTOPS: i64 = 4;

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceRef {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Client {
    pub address: String,
    #[serde(default)]
    pub mapped: bool,
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub pid: i64,
    pub workspace: WorkspaceRef,
    #[serde(default)]
    pub floating: bool,
    #[serde(default)]
    pub at: [i64; 2],
    #[serde(default)]
    pub size: [i64; 2],
    #[serde(rename = "focusHistoryID", default)]
    pub focus_history_id: i64,
}

impl Client {
    pub fn minimized(&self) -> bool {
        self.workspace.name == MINIMIZED_WS
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Monitor {
    pub id: i64,
    pub name: String,
    pub width: i64,
    pub height: i64,
    pub x: i64,
    pub y: i64,
    #[serde(default = "one")]
    pub scale: f64,
    #[serde(default)]
    pub focused: bool,
    /// [left, top, right, bottom] areas reserved by layer-shell surfaces (the taskbar).
    #[serde(default)]
    pub reserved: [i64; 4],
}

fn one() -> f64 {
    1.0
}

impl Monitor {
    /// Usable (logical) rectangle: x, y, w, h – i.e. everything except the taskbar.
    pub fn usable(&self) -> (i64, i64, i64, i64) {
        let w = (self.width as f64 / self.scale).round() as i64;
        let h = (self.height as f64 / self.scale).round() as i64;
        let [l, t, r, b] = self.reserved;
        (self.x + l, self.y + t, w - l - r, h - t - b)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ActiveWorkspace {
    id: i64,
}

fn json<T: for<'de> Deserialize<'de>>(what: &str) -> Option<T> {
    serde_json::from_slice(&raw_json(what)).ok()
}

/// Per-session scratch file. Lives in `XDG_RUNTIME_DIR` (tmpfs, wiped on logout/reboot) so
/// saved window addresses never go stale across sessions.
fn runtime_file(name: &str) -> std::path::PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(crate::config::state_dir)
        .join(name)
}

pub fn clients() -> Vec<Client> {
    json::<Vec<Client>>("clients")
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.mapped && !c.class.is_empty())
        .collect()
}

pub fn monitors() -> Vec<Monitor> {
    json("monitors").unwrap_or_default()
}

pub fn focused_monitor() -> Option<Monitor> {
    let m = monitors();
    m.iter()
        .find(|m| m.focused)
        .cloned()
        .or_else(|| m.into_iter().next())
}

pub fn active_window() -> Option<Client> {
    let c: Option<Client> = json("activewindow");
    c.filter(|c| c.mapped)
}

pub fn active_workspace_id() -> Option<i64> {
    json::<ActiveWorkspace>("activeworkspace").map(|w| w.id)
}

/// Run a Lua dispatcher expression, e.g. `hl.dsp.window.close()`.
pub fn dispatch(lua: &str) -> bool {
    Command::new("hyprctl")
        .args(["dispatch", lua])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn win(addr: &str) -> String {
    format!("\"address:{addr}\"")
}

pub fn focus(addr: &str) -> bool {
    dispatch(&format!("hl.dsp.focus({{ window = {} }})", win(addr)))
}

pub fn raise(addr: &str) -> bool {
    dispatch(&format!(
        "hl.dsp.window.alter_zorder({{ mode = \"top\", window = {} }})",
        win(addr)
    ))
}

pub fn close(addr: &str) -> bool {
    dispatch(&format!(
        "hl.dsp.window.close({{ window = {} }})",
        win(addr)
    ))
}

/// Switch to virtual desktop `id`, keeping it on the monitor you are currently using.
pub fn switch_workspace(id: i64) -> bool {
    dispatch(&format!(
        "hl.dsp.focus({{ workspace = {id}, on_current_monitor = true }})"
    ))
}

/// Win+Ctrl+Left/Right: previous / next virtual desktop (1..=DESKTOPS, wrapping).
/// Unlike `e-1`/`e+1` this works even when the neighbouring desktop is still empty.
pub fn switch_workspace_relative(delta: i64) {
    let cur = active_workspace_id().unwrap_or(1).clamp(1, DESKTOPS);
    switch_workspace((cur - 1 + delta).rem_euclid(DESKTOPS) + 1);
}

/// "Minimise" = park the window on a hidden special workspace.
pub fn minimize(addr: &str) -> bool {
    dispatch(&format!(
        "hl.dsp.window.move({{ workspace = \"{MINIMIZED_WS}\", follow = false, window = {} }})",
        win(addr)
    ))
}

/// Bring a (possibly minimised) window back to the current workspace and focus it.
pub fn restore_and_focus(c: &Client) -> bool {
    if c.minimized() {
        let ws = active_workspace_id().unwrap_or(1);
        dispatch(&format!(
            "hl.dsp.window.move({{ workspace = {ws}, follow = false, window = {} }})",
            win(&c.address)
        ));
    } else if c.workspace.id > 0 {
        switch_workspace(c.workspace.id);
    }
    let ok = focus(&c.address);
    raise(&c.address);
    ok
}

pub fn set_geometry(addr: &str, x: i64, y: i64, w: i64, h: i64) {
    // `relative` defaults to false => exact size / position.
    dispatch(&format!(
        "hl.dsp.window.resize({{ x = {w}, y = {h}, window = {} }})",
        win(addr)
    ));
    dispatch(&format!(
        "hl.dsp.window.move({{ x = {x}, y = {y}, window = {} }})",
        win(addr)
    ));
}

/// Win+D behaviour: minimise everything, or restore what we minimised last time.
pub fn toggle_show_desktop() {
    let path = runtime_file("w11-show-desktop.json");
    let all = clients();
    // Only trust saved addresses that still exist.
    let saved: Vec<String> = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|a| all.iter().any(|c| &c.address == a))
        .collect();
    if saved.is_empty() {
        let addrs: Vec<String> = all
            .iter()
            .filter(|c| !c.minimized())
            .map(|c| c.address.clone())
            .collect();
        for a in &addrs {
            minimize(a);
        }
        let _ = std::fs::write(&path, serde_json::to_vec(&addrs).unwrap_or_default());
    } else {
        for c in all.iter().filter(|c| saved.contains(&c.address)) {
            restore_and_focus(c);
        }
        let _ = std::fs::remove_file(&path);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snap {
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Maximize,
}

impl Snap {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "left" => Self::Left,
            "right" => Self::Right,
            "top-left" => Self::TopLeft,
            "top-right" => Self::TopRight,
            "bottom-left" => Self::BottomLeft,
            "bottom-right" => Self::BottomRight,
            "maximize" => Self::Maximize,
            _ => return None,
        })
    }
}

fn geom_file() -> std::path::PathBuf {
    runtime_file("w11-restore-geometry.json")
}

type GeomMap = std::collections::HashMap<String, [i64; 4]>;

fn load_geoms() -> GeomMap {
    std::fs::read(geom_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_geoms(m: &GeomMap) {
    let _ = std::fs::write(geom_file(), serde_json::to_vec(m).unwrap_or_default());
}

/// Snap Layouts: Win+Arrows. Remembers the pre-snap geometry so `restore` works.
pub fn snap_active(mode: Snap) {
    let (Some(c), Some(m)) = (active_window(), focused_monitor()) else {
        return;
    };
    let (ux, uy, uw, uh) = m.usable();
    let (hw, hh) = (uw / 2, uh / 2);
    let (x, y, w, h) = match mode {
        Snap::Left => (ux, uy, hw, uh),
        Snap::Right => (ux + hw, uy, uw - hw, uh),
        Snap::TopLeft => (ux, uy, hw, hh),
        Snap::TopRight => (ux + hw, uy, uw - hw, hh),
        Snap::BottomLeft => (ux, uy + hh, hw, uh - hh),
        Snap::BottomRight => (ux + hw, uy + hh, uw - hw, uh - hh),
        Snap::Maximize => (ux, uy, uw, uh),
    };
    let mut g = load_geoms();
    // Forget windows that no longer exist so the map can't grow forever.
    let live = clients();
    g.retain(|addr, _| live.iter().any(|l| &l.address == addr));
    g.entry(c.address.clone())
        .or_insert([c.at[0], c.at[1], c.size[0], c.size[1]]);
    save_geoms(&g);
    set_geometry(&c.address, x, y, w, h);
}

/// Restore the active window to the geometry it had before snapping/maximising.
/// Returns false if there was nothing to restore.
pub fn restore_active() -> bool {
    let Some(c) = active_window() else {
        return false;
    };
    let mut g = load_geoms();
    if let Some([x, y, w, h]) = g.remove(&c.address) {
        save_geoms(&g);
        set_geometry(&c.address, x, y, w, h);
        true
    } else {
        false
    }
}

pub fn is_snapped(addr: &str) -> bool {
    load_geoms().contains_key(addr)
}

pub fn toggle_maximize_active() {
    if let Some(c) = active_window() {
        if is_snapped(&c.address) {
            restore_active();
        } else {
            snap_active(Snap::Maximize);
        }
    }
}

pub fn minimize_active() {
    if let Some(c) = active_window() {
        minimize(&c.address);
    }
}

/// Alt+Tab (quick switch): focus the previously used window.
pub fn alt_tab() {
    let mut all = clients();
    all.sort_by_key(|c| c.focus_history_id);
    if let Some(c) = all
        .iter()
        .find(|c| c.focus_history_id == 1)
        .or_else(|| all.get(1))
    {
        restore_and_focus(c);
    }
}

fn query(what: &str) -> Option<Vec<u8>> {
    let rt = std::env::var_os("XDG_RUNTIME_DIR")?;
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let path = std::path::Path::new(&rt)
        .join("hypr")
        .join(sig)
        .join(".socket.sock");
    let mut s = UnixStream::connect(path).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(800))).ok()?;
    s.write_all(format!("j/{what}").as_bytes()).ok()?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).ok()?;
    Some(buf)
}

pub fn raw_json(what: &str) -> Vec<u8> {
    query(what).unwrap_or_else(|| {
        // fallback: old path
        Command::new("hyprctl")
            .args(["-j", what])
            .output()
            .map(|o| o.stdout)
            .unwrap_or_default()
    })
}

/// Spawn a background thread that forwards the event name of every Hyprland IPC event.
pub fn listen_events(tx: async_channel::Sender<String>) {
    std::thread::spawn(move || {
        let (Ok(rt), Ok(sig)) = (
            std::env::var("XDG_RUNTIME_DIR"),
            std::env::var("HYPRLAND_INSTANCE_SIGNATURE"),
        ) else {
            return;
        };
        let path = format!("{rt}/hypr/{sig}/.socket2.sock");
        let Ok(stream) = UnixStream::connect(path) else {
            return;
        };
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let name = line.split(">>").next().unwrap_or("").to_string();
            if tx.send_blocking(name).is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_client_json() {
        let raw = r#"[{"address":"0x55d1","mapped":true,"hidden":false,"at":[10,20],"size":[800,600],
            "workspace":{"id":2,"name":"2"},"floating":true,"pid":42,"class":"firefox",
            "title":"Mozilla Firefox","focusHistoryID":1,"initialClass":"firefox"}]"#;
        let v: Vec<Client> = serde_json::from_str(raw).unwrap();
        assert_eq!(v[0].class, "firefox");
        assert_eq!(v[0].size, [800, 600]);
        assert!(!v[0].minimized());
    }

    #[test]
    fn minimized_detected() {
        let raw = r#"{"address":"0x1","mapped":true,"workspace":{"id":-99,"name":"special:minimized"},"class":"x"}"#;
        let c: Client = serde_json::from_str(raw).unwrap();
        assert!(c.minimized());
    }

    #[test]
    fn usable_area_excludes_taskbar_and_scale() {
        let raw = r#"{"id":0,"name":"eDP-1","width":2880,"height":1800,"x":0,"y":0,"scale":2.0,
            "focused":true,"reserved":[0,0,0,48]}"#;
        let m: Monitor = serde_json::from_str(raw).unwrap();
        assert_eq!(m.usable(), (0, 0, 1440, 900 - 48));
    }

    #[test]
    fn snap_parse() {
        assert_eq!(Snap::parse("top-left"), Some(Snap::TopLeft));
        assert_eq!(Snap::parse("nope"), None);
    }

    #[test]
    fn desktop_wraparound() {
        let next = |cur: i64, d: i64| (cur - 1 + d).rem_euclid(DESKTOPS) + 1;
        assert_eq!(next(4, 1), 1);
        assert_eq!(next(1, -1), 4);
        assert_eq!(next(2, 1), 3);
    }
}
