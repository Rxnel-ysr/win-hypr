# win-hypr

**A Windows 11 like desktop for Hyprland, built as a lightweight Rust shell.**

win11-hypr turns a bare [Hyprland](https://hyprland.org) session (≥ 0.55, Lua config) into something that looks, feels and *behaves* like Windows 11: centred taskbar, Start menu, Quick Settings, notification center, Task View with virtual desktops, Snap, File Explorer, Settings app, lock screen and the full Windows keyboard-shortcut set.

It is written from scratch in Rust (GTK4 + `gtk4-layer-shell`) and designed around one constraint: **run well on modest hardware**. The reference machine is a ThinkPad T495s (Ryzen 5 PRO 3500U-class iGPU, 8 GB RAM, Arch Linux).

<p align="center">
  <img width="900" alt="win11-hypr desktop and RAM usage" src="https://github.com/user-attachments/assets/0dbc79a9-c7cc-4604-8bcb-0120f9cc36af" />
</p>

---

## Contents

- [Features](#features)
- [Quick start](#quick-start)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Architecture](#architecture)
- [Performance and memory design](#performance-and-memory-design)
- [Configuration](#configuration)
- [Dependencies](#dependencies)
- [Project status](#project-status)
- [Troubleshooting](#troubleshooting)
- [Uninstall](#uninstall)
- [Contributing](#contributing)
- [License](#license)

---

## Features

| Windows feature | win11-hypr |
|---|---|
| **Taskbar** | Centred Start / Search / Task View, pinned + running apps with focus indicators, click to launch / focus / minimise / cycle, middle-click new window, right-click Pin / Unpin / Close, weather widget, tray overflow (StatusNotifier), network-volume-battery cluster, clock + date, show-desktop sliver |
| **Start menu** | Search-as-you-type, Pinned grid, Recommended (recent files), All apps (A–Z), power menu (Lock, Sign out, Sleep, Shut down, Restart). Enter on an unknown query runs it, like Win+R |
| **Quick Settings** | Wi-Fi, Bluetooth, Airplane mode, Battery saver, Night light, Focus assist, brightness and volume sliders, battery status |
| **Notification center** | Own `org.freedesktop.Notifications` daemon: toasts, persistent history, Do-not-disturb, month calendar |
| **Task View** | Window thumbnails, per-desktop miniatures, new / close desktop, hover-preview, full keyboard navigation, middle-click to close |
| **Virtual desktops** | Up to 10, no wrap-around, Windows-style renumbering when a desktop is closed, minimised windows remember their desktop |
| **Snap** | Halves, quarters, maximise, restore to the pre-snap size, minimise, show desktop |
| **File Explorer** | Navigation pane, details list, address bar, search, command bar, cut / copy / paste / rename / Recycle Bin, sort and hidden items, context menu, back / forward / up |
| **Settings** | System, Bluetooth & devices, Network & internet (Wi-Fi list), Personalization (wallpaper, accent colour, taskbar), Apps (default apps), Accounts, Time & language, System update |
| **Lock screen** | `hyprlock` themed like the Windows lock / sign-in screen, `hypridle` timers (lock → screen off → sleep, editable in Settings) |
| **Snipping Tool** | `Win+Shift+S` region capture with markup (`satty`), `PrtSc` full screen |
| **Clipboard history** | `Win+V` through `cliphist` |

---

## Quick start

**Target:** Arch Linux (other distros: the installer skips packages and tells you what is missing).

```bash
git clone https://github.com/Rxnel-ysr/win-hypr.git
cd win-hypr
./install.sh            # interactive
# ./install.sh -y       # accept all defaults
```

Then log out and start Hyprland from a TTY (`Hyprland`), or run `hyprctl reload` inside a running session.

### Installer options

| Flag | Effect |
|---|---|
| `-y`, `--yes` | Non-interactive |
| `--no-deps` | Skip pacman / AUR packages |
| `--no-aur` | Skip AUR packages (icons, cursor, fonts) |
| `--no-build` | Reuse an existing `target/release` build |
| `--no-config` | Install binaries only, leave `~/.config` untouched |
| `--uninstall` | Remove binaries and desktop entries (`--purge` also removes user data) |

The installer backs up everything it replaces as `*.bak-<timestamp>` and keeps your existing `monitors.lua`. On machines with ≤ 8 GB RAM it limits Cargo to 2 jobs, because fat LTO is memory-hungry at link time.

### Manual build

```bash
cargo build --release --workspace
install -Dm755 target/release/w11-* target/release/w11ctl ~/.local/bin/
```

Requires Rust ≥ 1.87, `gtk4`, `gtk4-layer-shell` ≥ 1.0 and `pkgconf`.

---

## Keyboard shortcuts

Windows muscle memory works as-is.

| Keys | Action |
|---|---|
| `Win` (tap) | Start menu |
| `Win+S` / `Win+R` / `Win+X` | Search / Run / Start (power) |
| `Win+E` | File Explorer |
| `Win+I` | Settings |
| `Win+A` | Quick Settings |
| `Win+N` / `Win+W` | Notification center + calendar |
| `Win+Tab` | Task View |
| `Alt+Tab` | Switch to the previous window |
| `Win+D` / `Win+M` | Show desktop / minimise all |
| `Win+L` | Lock |
| `Win+←` / `→` | Snap left / right |
| `Win+↑` / `↓` | Maximise / restore (or minimise) |
| `Win+Alt+←/→/↑/↓` | Snap to quarters |
| `Win+Shift+←/→` | Move window to previous / next display |
| `Win+Ctrl+←/→` | Previous / next virtual desktop |
| `Win+Ctrl+1…4` | Jump to desktop N |
| `Win+Ctrl+Shift+1…4` | Move window to desktop N |
| `Win+Shift+S` | Snipping Tool (region) |
| `PrtSc` / `Win+PrtSc` | Full-screen screenshot |
| `Win+V` | Clipboard history |
| `Ctrl+Shift+Esc` | Task Manager (`btm` / `btop`) |
| `Alt+F4` | Close window |
| `F11` | Fullscreen |
| `Win` + drag (left / right button) | Move / resize window |

Bindings live in `config/hypr/modules/binds.lua`.

---

## Architecture

```
install.sh                     build + install + configure
config/
  hypr/hyprland.lua            entry point, require()s the modules below
  hypr/modules/*.lua           env, monitors, input, look, perf, animations, rules, binds, autostart
  hypr/hyprlock.conf           lock / sign-in screen
  hypr/hypridle.conf           idle timers
  hypr/scripts/                screenshot.sh, clipboard.sh
  gtk-3.0, gtk-4.0/            dark theme, Fluent icons, Selawik, Bibata
  applications/                .desktop entries for Explorer and Settings
  xdg-desktop-portal/          portal preference
crates/
  w11-common    Hyprland IPC, virtual-desktop logic, system helpers, thumbnails,
                popup (layer-shell flyout) framework, Fluent CSS and theming
  w11-taskbar   taskbar, StatusNotifier tray host, weather, clock
  w11-start     Start menu (search, pinned, recommended, all apps, power)
  w11-flyout    one binary, three flyouts:  quick | calendar | taskview
  w11-notify    notification daemon + toasts + history
  w11-explorer  File Explorer
  w11-settings  Settings app
  w11ctl        CLI verbs: snap, restore, minimize, show-desktop, alt-tab,
                close, workspace, lock / sleep / shutdown / restart / sign-out
```

### How it fits together

- **Compositor:** Hyprland. Everything is a floating window (Windows behaviour), new windows open centred, 0 gaps, 8 px rounded corners.
- **IPC:** `w11-common::hypr` talks to Hyprland's JSON socket for queries and to `hyprctl dispatch` for Lua dispatcher expressions (Hyprland ≥ 0.55 syntax). `listen_events` forwards compositor events to the UI.
- **Virtual desktops:** desktop *N* is Hyprland workspace *N*. The desktop count lives in `$XDG_RUNTIME_DIR/w11-<instance>/`, so state never leaks between sessions. "Minimise" parks a window on `special:minimized` and remembers its origin desktop.
- **Snap and restore:** pre-snap geometry is stored per window, so `Win+↓` restores exactly what you had.
- **Flyouts:** every flyout is its own single-instance process on `wlr-layer-shell`. Launching the binary again toggles it. A transparent scrim catches outside clicks, and opening one flyout closes the other via `SIGUSR1`.
- **Thumbnails:** Wayland has no portable "capture this window" call, so a small daemon inside the taskbar grabs the active window's rectangle with `grim` on focus changes and caches a small JPEG. Task View only reads the cache.
- **Theming:** one Fluent dark stylesheet in `w11-common`, plus a small per-flyout stylesheet loaded only by the process that needs it.

---

## Performance and memory design

The goal is a desktop that idles at a fraction of what a typical GNOME / KDE shell uses, which matters on an 8 GB laptop with an iGPU.

**Rendering**
- GTK renderer is forced to `cairo` for the static surfaces (taskbar, Start, flyouts, notifications, Settings). That avoids creating a GL context, which costs tens of MB per process on an iGPU.
- `LITE = true` (default in `look.lua`) disables compositor blur, shadows and animations. Nothing expensive runs in Hyprland.

**Process model**
- Flyouts are launched on demand and **exit after 90 seconds hidden** (`popup.rs`). Only the taskbar and the notification daemon stay resident.
- Task View, Quick Settings and the notification center build their widgets when opened and **drop them when closed**, then call `malloc_trim(0)` so glibc hands freed pages back to the kernel.
- The Start menu's app index, "All apps" list, search results and recent-files list are lazily built and freed on hide.
- Settings builds one page at a time and trims the heap on every navigation.

**CPU and I/O**
- The taskbar rebuilds its app strip only when a *signature* of the window list changes (titles are deliberately excluded). Event bursts are debounced to a single refresh (40 ms).
- Slow probes (`nmcli`, `wpctl`, `bluetoothctl`, `brightnessctl`) run off the UI thread, in parallel, and the result is cached briefly.
- Tray icons are fetched only while the overflow popover is open and dropped when it closes.
- Process discovery reads `/proc` directly instead of forking `pgrep`.
- Sliders are debounced so dragging does not fork a process per pixel.
- Weather refreshes every 30 minutes, tray status every 10 seconds, the safety-net window refresh every 60 seconds.
- Thumbnails are 480 px JPEGs at quality 70, captured only on focus change, skipped while the lock screen or any overlay is up.

**Build profile** (`Cargo.toml`)
```toml
[profile.release]
lto = "fat"
codegen-units = 1
opt-level = 3
panic = "abort"
strip = true
```

**Measure it yourself**
```bash
ps -eo rss,comm --sort=-rss | grep -E 'w11|Hyprland|hypr|swaybg' | head
free -h
```

**Extra RAM savers for 8 GB machines**
- Install `zram-generator` (offered as an optional package) and enable zram swap.
- Keep `LITE = true`.
- Use `swaybg` rather than a heavier wallpaper engine, which is the default.
- Remove other notification daemons (mako / dunst / swaync).

**Turning the eye candy on:** set `LITE = false` in `~/.config/hypr/modules/look.lua` for blur, shadows and Fluent motion curves. It costs GPU and RAM on an iGPU.

---

## Configuration

| File | Purpose |
|---|---|
| `~/.config/win11-hypr/config.json` | Pinned apps (also via right-click → Pin / Unpin), Task View command, weather on / off, weather location |
| `~/.config/win11-hypr/user.css` | Overrides for any Fluent style (accent colour, sizes). Settings → Personalization writes the accent here |
| `~/.config/win11-hypr/wallpaper.txt` | Current wallpaper path (set from Settings) |
| `~/.config/hypr/modules/monitors.lua` | Displays, resolution, scale, layout |
| `~/.config/hypr/modules/input.lua` | Keyboard layout, touchpad behaviour (tap-to-click, click-to-focus) |
| `~/.config/hypr/modules/binds.lua` | Shortcuts |
| `~/.config/hypr/hypridle.conf` | Idle timers (Settings → System → Power & battery rewrites it) |
| `~/.face` | Lock-screen profile picture |
| `~/Pictures/Wallpapers/bloom.jpg` | Default wallpaper |

Task Manager, terminal and launcher defaults are `foot`, `btm` / `btop`, `fuzzel`. The terminal constant is in `crates/w11-common/src/sys.rs` and should match `binds.lua`.

---

## Dependencies

**Build:** Rust ≥ 1.87, `gtk4`, `gtk4-layer-shell` (≥ 1.0), `pkgconf`, `base-devel`.

**Runtime (installed automatically on Arch):**

| Area | Packages |
|---|---|
| Session | `hyprland` (≥ 0.55), `hyprlock`, `hypridle`, `hyprsunset`, `hyprpolkitagent`, `xdg-desktop-portal-hyprland`, `polkit` |
| Screen / clipboard | `grim`, `slurp`, `wl-clipboard`, `cliphist`, `swaybg` |
| System | `wireplumber` (`wpctl`), `brightnessctl`, `networkmanager`, `bluez`, `bluez-utils`, `power-profiles-daemon`, `playerctl`, `pacman-contrib` |
| Tools | `foot`, `fuzzel`, `curl` (weather), `xdg-utils`, `desktop-file-utils` |
| Fonts | `ttf-inter` (fallback) |

**Optional:** `satty` (snip markup), `bottom`, `btop`, `pavucontrol`, `blueman`, `nm-connection-editor`, `mpv`, `imv`, `kitty`, `zram-generator`.

**AUR (look and feel):** `fluent-icon-theme-git`, `bibata-cursor-theme`, `ttf-selawik`, `xdg-terminal-exec`.

> Disable any other notification daemon. `w11-notify` owns the D-Bus name `org.freedesktop.Notifications`.

### What is Rust, what is not

Rust (this repo): taskbar, Start, Quick Settings, notification center, toasts, Task View, File Explorer, Settings, window-management logic.
Recommended Rust extras: `satty`, `bottom`, optionally `wpaperd`, `regreet` / `tuigreet`.
Not Rust and not replaced here: Hyprland, `hyprlock`, `hypridle`, PipeWire, NetworkManager, BlueZ.

---

## Project status

**Working:** everything listed in [Features](#features). The workspace compiles, unit tests cover IPC parsing, snap geometry, desktop renumbering and Task View layout.

**Honest caveat:** the project was first assembled without a live Hyprland ≥ 0.55 session to test against. The Lua API usage follows the 0.55 / 0.56 docs, and rules whose field names could not be confirmed (`center`, `size`, layer-rule `blur` / `ignore_alpha`) are wrapped in `pcall`, so a wrong name logs a message instead of breaking the config. Expect some tuning on first run, and please open an issue with the log line if a rule is skipped.

**Not implemented yet**
- Widgets board (the weather button opens the calendar flyout)
- Snap Layouts hover flyout on the maximise button
- Alt+Tab visual switcher (Alt+Tab is a quick switch to the previous window)
- Taskbar hover previews
- Per-monitor taskbars
- Light theme
- Explorer tabs, thumbnails, drag-and-drop, shell-extension context menus
- Pinned system-tray icons on the taskbar (icons live in the overflow popover)
- Wi-Fi / Bluetooth device lists inside Quick Settings (they exist in Settings)

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| No toasts / `could not own org.freedesktop.Notifications` | Another daemon is running: `sudo pacman -Rns mako dunst swaync` |
| Icons look generic | Install `fluent-icon-theme-git`; check `gtk-icon-theme-name=Fluent-dark` |
| Font is not Segoe-like | Install `ttf-selawik` (falls back to Inter / Noto Sans) |
| Task View shows app icons instead of screenshots | Install `grim`; thumbnails appear after a window has been focused once |
| `w11-*` not found from keybinds | `~/.local/bin` must be in `PATH` (the Hyprland config adds it for the session) |
| A window rule is skipped in the log | Your Hyprland build uses a different field name. Open an issue with the message |
| Taskbar did not pick up a change | Settings → Personalization → *Restart taskbar* |
| Link step runs out of memory | `CARGO_BUILD_JOBS=1 ./install.sh`, or lower LTO in `Cargo.toml` |

---

## Uninstall

```bash
./install.sh --uninstall           # binaries + desktop entries
./install.sh --uninstall --purge   # also config.json, user.css, notification history
```

Your `~/.config/hypr` is left alone. Restore a backup with, for example:
`cp -r ~/.config/hypr/hyprland.lua.bak-<timestamp> ~/.config/hypr/hyprland.lua`

---

## Contributing

Issues and PRs are welcome, especially:

- Testing on real Hyprland ≥ 0.55 builds and reporting broken Lua rules
- Memory and CPU profiling results (`ps`, `heaptrack`, `perf`)
- Missing Windows behaviours from the list above

Before sending a PR:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

Keep new UI code lazy: build on open, drop on close, no polling when nothing changed.

---

## License

MIT

*Not affiliated with or endorsed by Microsoft. "Windows" and "Windows 11" are trademarks of Microsoft Corporation. This project only reproduces the look and workflow.*
