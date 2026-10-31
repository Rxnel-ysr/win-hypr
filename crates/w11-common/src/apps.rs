//! Installed-application discovery and matching of Hyprland window classes to `.desktop` files.

use gtk::gio;
use gtk::prelude::*;

/// Every launchable application, sorted by name.
pub fn all() -> Vec<gio::AppInfo> {
    let mut v: Vec<_> = gio::AppInfo::all()
        .into_iter()
        .filter(|a| a.should_show())
        .collect();
    v.sort_by_key(|a| a.name().to_lowercase());
    v
}

pub fn by_id(id: &str) -> Option<gio::AppInfo> {
    gio::DesktopAppInfo::new(id).map(|d| d.upcast::<gio::AppInfo>())
}

pub fn desktop_id(a: &gio::AppInfo) -> String {
    a.id().map(|s| s.to_string()).unwrap_or_default()
}

/// Does this application own windows of the given Hyprland class?
pub fn class_matches(a: &gio::AppInfo, class: &str) -> bool {
    let class = class.to_lowercase();
    if class.is_empty() {
        return false;
    }
    if let Some(id) = a.id() {
        let base = id.trim_end_matches(".desktop").to_lowercase();
        if base == class || base.ends_with(&format!(".{class}")) {
            return true;
        }
    }
    if let Some(d) = a.downcast_ref::<gio::DesktopAppInfo>() {
        if let Some(w) = d.startup_wm_class() {
            if w.to_lowercase() == class {
                return true;
            }
        }
    }
    a.executable()
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase() == class)
        .unwrap_or(false)
}

pub fn for_class(class: &str) -> Option<gio::AppInfo> {
    all().into_iter().find(|a| class_matches(a, class))
}

pub fn launch(a: &gio::AppInfo) {
    let _ = a.launch(&[], gio::AppLaunchContext::NONE);
}

pub fn icon_of(a: &gio::AppInfo) -> gio::Icon {
    a.icon()
        .unwrap_or_else(|| gio::ThemedIcon::new("application-x-executable").upcast())
}
