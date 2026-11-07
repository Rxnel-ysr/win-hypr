//! System integration through the usual Linux CLI tools (wpctl, brightnessctl, nmcli, ...).

use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;

/// Keep in sync with config/hypr/modules/binds.lua.
pub const TERMINAL: &str = "foot";
pub const SETTINGS_CMD: &str = "w11-settings";

/// Cache for bluetooth_enabled(): (when checked, value)
static BT: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// Run a shell command and capture trimmed stdout.
pub fn sh(cmd: &str) -> String {
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Run a program directly (no `sh -c`): one fork/exec instead of two.
pub fn run(prog: &str, args: &[&str]) -> String {
    Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Is a process with this `comm` running? Reads /proc, forks nothing (comm is max 15 chars).
pub fn process_running(comm: &str) -> bool {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| {
            e.file_name()
                .to_string_lossy()
                .chars()
                .all(|c| c.is_ascii_digit())
                && std::fs::read_to_string(e.path().join("comm"))
                    .map(|c| c.trim() == comm)
                    .unwrap_or(false)
        })
}

/// Spawn a command detached; a helper thread reaps it so we never leave zombies.
pub fn spawn(cmd: &str) {
    if let Ok(mut child) = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

// ---------------------------------------------------------------- audio

pub fn volume() -> (f64, bool) {
    let out = run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]);
    let v = out
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    (v.clamp(0.0, 1.5), out.contains("[MUTED]"))
}

pub fn set_volume(v: f64) {
    spawn(&format!(
        "wpctl set-volume @DEFAULT_AUDIO_SINK@ {:.2}",
        v.clamp(0.0, 1.0)
    ));
}

pub fn toggle_mute() {
    spawn("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle");
}

pub fn volume_icon(v: f64, muted: bool) -> &'static str {
    if muted || v <= 0.001 {
        "audio-volume-muted-symbolic"
    } else if v < 0.34 {
        "audio-volume-low-symbolic"
    } else if v < 0.67 {
        "audio-volume-medium-symbolic"
    } else {
        "audio-volume-high-symbolic"
    }
}

// ---------------------------------------------------------- brightness

/// Brightness in percent (0..=100), if a backlight exists.
pub fn brightness() -> Option<f64> {
    let out = sh("brightnessctl -m");
    out.split(',')
        .nth(3)
        .and_then(|p| p.trim_end_matches('%').parse::<f64>().ok())
}

pub fn set_brightness(pct: f64) {
    spawn(&format!(
        "brightnessctl set {}%",
        pct.clamp(1.0, 100.0) as u32
    ));
}

// -------------------------------------------------------------- battery

#[derive(Debug, Clone, Copy)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

pub fn battery() -> Option<Battery> {
    let dir = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for e in dir.flatten() {
        let p = e.path();
        let is_bat = std::fs::read_to_string(p.join("type"))
            .map(|t| t.trim() == "Battery")
            .unwrap_or(false);
        if !is_bat {
            continue;
        }
        let percent = std::fs::read_to_string(p.join("capacity"))
            .ok()?
            .trim()
            .parse::<u8>()
            .ok()?;
        let status = std::fs::read_to_string(p.join("status")).unwrap_or_default();
        return Some(Battery {
            percent,
            charging: matches!(status.trim(), "Charging" | "Full"),
        });
    }
    None
}

pub fn battery_icon(b: Battery) -> String {
    let level = (b.percent as u32 / 10) * 10;
    if b.charging {
        format!("battery-level-{level}-charging-symbolic")
    } else {
        format!("battery-level-{level}-symbolic")
    }
}

// ------------------------------------------------------------- network

pub fn set_wifi(on: bool) {
    spawn(&format!(
        "nmcli radio wifi {}",
        if on { "on" } else { "off" }
    ));
}

pub fn wifi_ssid() -> Option<String> {
    // `--rescan no`: don't trigger a radio scan every time Quick Settings opens
    run(
        "nmcli",
        &[
            "-t",
            "-f",
            "ACTIVE,SSID",
            "dev",
            "wifi",
            "list",
            "--rescan",
            "no",
        ],
    )
    .lines()
    .find_map(|l| l.strip_prefix("yes:").map(str::to_string))
    .filter(|s| !s.is_empty())
}

fn rd(p: &std::path::Path) -> String {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn iface_up(prefixes: &[&str]) -> bool {
    std::fs::read_dir("/sys/class/net")
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            prefixes.iter().any(|p| n.starts_with(p)) && rd(&e.path().join("operstate")) == "up"
        })
}

pub fn has_ethernet() -> bool {
    iface_up(&["en", "eth"])
}

pub fn wifi_enabled() -> bool {
    let mut found = false;
    for e in std::fs::read_dir("/sys/class/rfkill")
        .into_iter()
        .flatten()
        .flatten()
    {
        let p = e.path();
        if rd(&p.join("type")) == "wlan" {
            found = true;
            if rd(&p.join("soft")) == "0" && rd(&p.join("hard")) == "0" {
                return true;
            }
        }
    }
    if found {
        false
    } else {
        sh("nmcli radio wifi") == "enabled"
    }
}

pub fn network_icon() -> &'static str {
    if has_ethernet() {
        "network-wired-symbolic"
    } else if iface_up(&["wl"]) {
        "network-wireless-signal-good-symbolic"
    } else if wifi_enabled() {
        "network-wireless-offline-symbolic"
    } else {
        "network-wireless-disabled-symbolic"
    }
}

pub fn bluetooth_enabled() -> bool {
    if let Some((t, v)) = *BT.lock().unwrap() {
        if t.elapsed() < std::time::Duration::from_secs(2) {
            return v;
        }
    }
    let v = run("bluetoothctl", &["show"]).contains("Powered: yes");
    *BT.lock().unwrap() = Some((std::time::Instant::now(), v));
    v
}

pub fn set_bluetooth(on: bool) {
    *BT.lock().unwrap() = None;
    spawn(&format!(
        "bluetoothctl power {}",
        if on { "on" } else { "off" }
    ));
}

pub fn airplane_mode() -> bool {
    !wifi_enabled() && !bluetooth_enabled()
}

pub fn set_airplane_mode(on: bool) {
    set_wifi(!on);
    set_bluetooth(!on);
}

// --------------------------------------------------------------- power

pub fn battery_saver() -> bool {
    sh("powerprofilesctl get") == "power-saver"
}

pub fn set_battery_saver(on: bool) {
    spawn(&format!(
        "powerprofilesctl set {}",
        if on { "power-saver" } else { "balanced" }
    ));
}

pub fn night_light() -> bool {
    process_running("hyprsunset")
}

pub fn set_night_light(on: bool) {
    if on {
        spawn("hyprsunset -t 4000");
    } else {
        spawn("pkill -x hyprsunset");
    }
}

pub fn lock() {
    spawn("hyprlock");
}
pub fn sleep() {
    spawn("systemctl suspend");
}
pub fn shutdown() {
    spawn("systemctl poweroff");
}
pub fn restart() {
    spawn("systemctl reboot");
}
pub fn sign_out() {
    spawn("hyprctl dispatch 'hl.dsp.exit()'");
}

pub fn user_name() -> String {
    let n = sh("getent passwd \"$USER\" | cut -d: -f5 | cut -d, -f1");
    if n.is_empty() {
        std::env::var("USER").unwrap_or_else(|_| "User".into())
    } else {
        n
    }
}

// ------------------------------------------------------------- xwayland

fn xwayland_flag() -> std::path::PathBuf {
    crate::config::config_dir().join("xwayland-off")
}

/// The user's choice (flag file absent = enabled). This is what modules/xwayland.lua applies.
pub fn xwayland_enabled() -> bool {
    !xwayland_flag().exists()
}

/// Persist the choice and reload the Hyprland config so it takes effect now.
/// Persist the choice and apply it. Enabling also (re)starts the X11 compatibility layer
pub fn set_xwayland(on: bool) {
    let flag = xwayland_flag();
    if on {
        let _ = std::fs::remove_file(&flag);
    } else {
        let _ = std::fs::write(&flag, b"1");
    }
    let notify =
        |m: &str| format!("notify-send -a Xwayland -i dialog-information-symbolic Xwayland '{m}'");
    let script = if on {
        let fail = notify("XWayland did not start. Log out and back in.");
        let ok = notify("X11 compatibility enabled");
        format!(
            r#"
            hyprctl reload
            for _ in $(seq 40); do
                d=$(ps -o args= -C Xwayland | grep -om1 ':[0-9]\+')
                [ -n "$d" ] && break
                sleep 0.25
            done
            [ -z "$d" ] && {{ {fail}; exit 0; }}
            dbus-update-activation-environment --systemd DISPLAY="$d"
            pkill -x w11-taskbar
            pkill -x w11-start
            DISPLAY="$d" setsid -f w11-taskbar
            {ok}
            "#
        )
    } else {
        format!("hyprctl reload; {}", notify("X11 compatibility disabled"))
    };
    spawn(&script);
}
