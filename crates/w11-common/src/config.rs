use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Desktop-file IDs pinned to the taskbar / Start (in order).
    pub pinned: Vec<String>,
    /// Command run by the Task View button / Win+Tab.
    pub task_view_cmd: String,
    /// Show a wttr.in weather button on the left of the taskbar.
    pub weather: bool,
    /// Location for wttr.in (empty = auto).
    pub weather_location: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pinned: [
                "dev.win11hypr.Explorer.desktop",
                "firefox.desktop",
                "org.mozilla.firefox.desktop",
                "chromium.desktop",
                "google-chrome.desktop",
                "kitty.desktop",
                "Alacritty.desktop",
                "org.wezfurlong.wezterm.desktop",
                "code.desktop",
                "visual-studio-code.desktop",
                "org.gnome.TextEditor.desktop",
                "org.gnome.Calculator.desktop",
                "discord.desktop",
                "spotify.desktop",
                "steam.desktop",
                "org.kde.kate.desktop",
                "org.gnome.Settings.desktop",
                "pavucontrol.desktop",
                "nm-connection-editor.desktop",
                "blueman-manager.desktop",
                "mpv.desktop",
                "org.gnome.Loupe.desktop",
            ]
            .map(String::from)
            .to_vec(),
            task_view_cmd: "w11-flyout taskview".into(),
            weather: true,
            weather_location: String::new(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let d = base.join("win11-hypr");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn state_dir() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let d = base.join("win11-hypr");
    let _ = std::fs::create_dir_all(&d);
    d
}

impl Config {
    pub fn load() -> Self {
        let p = config_dir().join("config.json");
        match std::fs::read(&p) {
            Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
            Err(_) => {
                let c = Config::default();
                c.save();
                c
            }
        }
    }

    pub fn save(&self) {
        let p = config_dir().join("config.json");
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, s);
        }
    }
}

/// A single notification as stored in the history file shared by `w11-notify` and the flyout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredNotification {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    pub icon: String,
    pub time: i64,
}

pub fn notifications_path() -> PathBuf {
    state_dir().join("notifications.json")
}

pub fn load_notifications() -> Vec<StoredNotification> {
    std::fs::read(notifications_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_notifications(n: &[StoredNotification]) {
    if let Ok(s) = serde_json::to_vec(n) {
        let _ = std::fs::write(notifications_path(), s);
    }
}

pub fn dnd_flag() -> PathBuf {
    state_dir().join("dnd")
}

pub fn dnd_enabled() -> bool {
    dnd_flag().exists()
}

pub fn set_dnd(on: bool) {
    if on {
        let _ = std::fs::write(dnd_flag(), b"1");
    } else {
        let _ = std::fs::remove_file(dnd_flag());
    }
}
