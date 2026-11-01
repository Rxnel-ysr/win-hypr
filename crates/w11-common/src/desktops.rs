//! Virtual desktops with Windows 11 semantics, built on Hyprland's numbered workspaces.
//!
//! * desktop N == Hyprland workspace N
//! * the number of desktops lives in the session runtime dir (a fresh session starts with one)
//! * "minimise" parks a window on `special:minimized`; `origin.json` remembers which desktop it
//!   came from so Task View can still show it there and restoring it brings you back to it.

use crate::hypr::{self, Client};
use std::collections::HashMap;
use std::path::PathBuf;

pub const MAX_DESKTOPS: i64 = 10;

/// window address -> desktop it was minimised from
pub type Origins = HashMap<String, i64>;

/// Per-session scratch dir (`$XDG_RUNTIME_DIR/w11-<instance>`), so stale state can never leak
/// from one Hyprland session into the next.
pub fn runtime_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let sig: String = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .unwrap_or_default()
        .chars()
        .take(12)
        .collect();
    let d = base.join(format!("w11-{sig}"));
    let _ = std::fs::create_dir_all(&d);
    d
}

fn origin_file() -> PathBuf {
    runtime_dir().join("origin.json")
}

fn count_file() -> PathBuf {
    runtime_dir().join("desktops")
}

pub fn origins() -> Origins {
    std::fs::read(origin_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_origins(m: &Origins) {
    let _ = std::fs::write(origin_file(), serde_json::to_vec(m).unwrap_or_default());
}

/// Call right before minimising: remember which desktop the window is on.
pub fn remember_origin(addr: &str) {
    let found = hypr::clients().into_iter().find(|c| c.address == addr);
    if let Some(c) = found {
        if !c.minimized() && c.workspace.id > 0 {
            let mut m = origins();
            m.insert(addr.to_string(), c.workspace.id);
            save_origins(&m);
        }
    }
}

/// Call when restoring: returns (and forgets) the desktop the window was minimised from.
pub fn take_origin(addr: &str) -> Option<i64> {
    let mut m = origins();
    let v = m.remove(addr);
    if v.is_some() {
        save_origins(&m);
    }
    v
}

/// Which desktop does this window belong to?
pub fn desktop_of(c: &Client, origin: &Origins, current: i64) -> i64 {
    if c.minimized() {
        origin.get(&c.address).copied().unwrap_or(current)
    } else {
        c.workspace.id
    }
}

pub fn current() -> i64 {
    hypr::active_workspace_id().filter(|i| *i > 0).unwrap_or(1)
}

fn stored_count() -> i64 {
    std::fs::read_to_string(count_file())
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(1)
}

pub fn set_count(n: i64) {
    let _ = std::fs::write(count_file(), n.clamp(1, MAX_DESKTOPS).to_string());
}

/// Number of desktops, given data the caller already fetched.
pub fn count_for(clients: &[Client], origin: &Origins, current: i64) -> i64 {
    let used = clients
        .iter()
        .map(|c| desktop_of(c, origin, current))
        .filter(|d| *d > 0)
        .max()
        .unwrap_or(1);
    stored_count().max(used).max(current).clamp(1, MAX_DESKTOPS)
}

pub fn count() -> i64 {
    count_for(&hypr::clients(), &origins(), current())
}

fn move_to(addr: &str, ws: i64) -> bool {
    hypr::dispatch(&format!(
        "hl.dsp.window.move({{ workspace = {ws}, follow = false, window = \"address:{addr}\" }})"
    ))
}

/// Create a desktop after the last one and switch to it. Returns its number.
pub fn new_desktop() -> i64 {
    let n = count();
    if n >= MAX_DESKTOPS {
        hypr::switch_workspace(n);
        return n;
    }
    let id = n + 1;
    set_count(id);
    hypr::switch_workspace(id);
    id
}

/// Win+Ctrl+Left/Right: no wrap-around, like Windows.
pub fn switch_relative(delta: i64) {
    let cur = current();
    let target = (cur + delta).clamp(1, count());
    if target != cur {
        hypr::switch_workspace(target);
    }
}

/// Jump to an existing desktop.
pub fn goto(n: i64) {
    if n >= 1 && n <= count() {
        hypr::switch_workspace(n);
    }
}

/// Where does desktop `old` end up once desktop `closed` is removed?
/// Windows of the closed desktop move to its left neighbour (or the right one for desktop 1)
/// and every desktop to the right shifts down by one.
pub fn renumber(old: i64, closed: i64) -> i64 {
    if old < closed {
        old
    } else if old == closed {
        (closed - 1).max(1)
    } else {
        old - 1
    }
}

pub fn close_desktop(d: i64) {
    let clients = hypr::clients();
    let mut origin = origins();
    let cur = current();
    let n = count_for(&clients, &origin, cur);
    if n <= 1 || d < 1 || d > n {
        return;
    }
    for c in clients.iter().filter(|c| !c.minimized() && c.workspace.id > 0) {
        let target = renumber(c.workspace.id, d);
        if target != c.workspace.id {
            move_to(&c.address, target);
        }
    }
    for v in origin.values_mut() {
        *v = renumber(*v, d);
    }
    save_origins(&origin);
    set_count(n - 1);
    hypr::switch_workspace(renumber(cur, d));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renumber_middle() {
        // desktops 1,2,3,4 and desktop 2 is closed
        assert_eq!(renumber(1, 2), 1);
        assert_eq!(renumber(2, 2), 1);
        assert_eq!(renumber(3, 2), 2);
        assert_eq!(renumber(4, 2), 3);
    }

    #[test]
    fn renumber_first() {
        assert_eq!(renumber(1, 1), 1);
        assert_eq!(renumber(2, 1), 1);
        assert_eq!(renumber(3, 1), 2);
    }

    #[test]
    fn renumber_last() {
        assert_eq!(renumber(3, 3), 2);
        assert_eq!(renumber(2, 3), 2);
    }
}
