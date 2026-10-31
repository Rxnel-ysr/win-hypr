# win11-hypr — Windows 11 on Hyprland, with a Rust shell

A Hyprland **Lua config** (Hyprland ≥ 0.55) plus a from-scratch Rust desktop shell (GTK4 + `gtk4-layer-shell`) that recreates the Windows 11 experience.

```
install.sh                    build + install everything
config/hypr/hyprland.lua      entry point (require()s modules/*)
config/hypr/modules/*.lua     env, monitors, input, look, animations, rules, binds, autostart
config/hypr/hyprlock.conf     Windows-style lock / sign-in screen
config/hypr/hypridle.conf     lock → screen off → sleep timers
crates/
  w11-common    Hyprland IPC, system helpers, Fluent CSS, flyout/popup framework
  w11-taskbar   taskbar: Start, Search, Task View, pinned+running apps, tray, clock, show-desktop
  w11-start     Start menu: search, pinned grid, recommended files, All apps, power menu
  w11-flyout    `quick` (Quick Settings), `calendar` (notifications + calendar), `taskview`
  w11-notify    org.freedesktop.Notifications daemon: toasts + history + Focus assist
  w11-explorer  File Explorer: nav pane, details view, command bar, copy/cut/paste/rename/trash
  w11ctl        Snap layouts, minimise/restore, show desktop, Alt+Tab, lock/power verbs
```

## What is Rust, what isn't
Rust (this repo): taskbar, Start, Quick Settings, notification center, toasts, Task View, File Explorer, snap/minimise logic.
Rust (recommended extras): `wpaperd` (wallpaper), `satty` + `wayshot` (Snipping Tool), `regreet`/`tuigreet` (login screen), `bottom` (Task Manager).
Not Rust and not replaceable here: the compositor (Hyprland, C++), `hyprlock`, `hypridle`, PipeWire, NetworkManager. Rewriting those would be separate multi-year projects.

## Dependencies
Build: Rust ≥ 1.87, `gtk4`, `gtk4-layer-shell` (≥ 1.0), `pkg-config`.
Runtime: `wireplumber` (wpctl), `brightnessctl`, `networkmanager` (nmcli), `bluez-utils`, `power-profiles-daemon`, `playerctl`, `hyprsunset`, `hyprlock`, `hypridle`, `cliphist`, `wl-clipboard`, `slurp`, `grim` (or `wayshot`), `curl` (weather).
Look: `Fluent-icon-theme` (icon theme name `Fluent-dark`), Selawik or Inter font, Bibata or Windows cursor.
Disable other notification daemons (mako/dunst) — `w11-notify` owns the D-Bus name.

## Keys (Windows muscle memory)
Win (tap) Start · Win+S/R search/run · Win+E Explorer · Win+D show desktop · Win+L lock · Win+A Quick Settings · Win+N notifications/calendar · Win+Tab Task View · Alt+Tab quick switch · Win+←/→/↑/↓ snap/maximise/restore · Win+Alt+arrows quarters · Win+Ctrl+←/→ virtual desktops · Win+Ctrl+1‑4 jump · Win+Shift+S snip · Win+V clipboard · Ctrl+Shift+Esc Task Manager · Alt+F4 close.

## Customising
`~/.config/win11-hypr/config.json` pinned apps (also right-click → Pin/Unpin), Task View command, weather location.
`~/.config/win11-hypr/user.css` overrides any Fluent style (accent colour, sizes).

## Honest status
Verified here: the whole workspace compiles with zero warnings (GTK 4.14, gtk4-layer-shell 1.0.4, Rust 1.91), unit tests for IPC parsing and snap geometry pass, the Fluent CSS parses with 0 errors, the File Explorer runs without panics under a virtual display, and the Lua config executes against a mock `hl` API.
**Not verified:** a live Hyprland ≥ 0.55 session — I had no compositor available. The Lua API usage follows the 0.55/0.56 docs; rules whose field names I couldn't confirm (`center`, `size`, layer-rule `blur`/`ignore_alpha`) are wrapped in `pcall` so a wrong name logs a message instead of breaking the config. Expect some tuning on first run.

Not implemented (yet): a full Settings app, Widgets board, window thumbnails in Task View/taskbar hover, Snap Layouts hover flyout, Alt+Tab visual switcher, system-tray (StatusNotifier) icons, Wi-Fi/Bluetooth device lists, per-monitor taskbars, light theme, Explorer tabs/thumbnails/context-menu extensions.
