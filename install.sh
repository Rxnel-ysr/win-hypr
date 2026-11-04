#!/usr/bin/env bash
# win11-hypr installer: builds the Rust shell and installs the Hyprland Lua config.
set -euo pipefail
cd "$(dirname "$0")"

BIN="$HOME/.local/bin"
CFG="${XDG_CONFIG_HOME:-$HOME/.config}"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
STAMP="$(date +%Y%m%d-%H%M%S)"

say() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null || die "cargo not found (install rustup or your distro's rust package; Rust >= 1.87 needed)"
pkg-config --exists gtk4 || die "GTK4 development files missing (Arch: gtk4, Debian/Ubuntu: libgtk-4-dev)"
pkg-config --exists gtk4-layer-shell-0 || die "gtk4-layer-shell missing (Arch: gtk4-layer-shell, Fedora: gtk4-layer-shell-devel)"

if command -v Hyprland >/dev/null; then
  ver="$(Hyprland --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+' | head -1 || true)"
  say "Hyprland ${ver:-?} detected (Lua config needs >= 0.55)"
else
  say "warning: Hyprland not found in PATH"
fi

say "Building the Rust shell (release)…"
cargo build --release --workspace

say "Installing binaries to $BIN"
mkdir -p "$BIN"
for b in w11-taskbar w11-start w11-flyout w11-notify w11-explorer w11-settings w11ctl; do
  install -m755 "target/release/$b" "$BIN/$b"
done

say "Installing File Explorer desktop entry + making it the default folder handler"
mkdir -p "$APPS"
install -m644 config/applications/dev.win11hypr.Explorer.desktop "$APPS/"
update-desktop-database "$APPS" 2>/dev/null || true
xdg-mime default dev.win11hypr.Explorer.desktop inode/directory 2>/dev/null || true

say "Installing Hyprland config to $CFG/hypr (existing config backed up)"
mkdir -p "$CFG/hypr"
for f in hyprland.lua hyprland.conf hyprlock.conf hypridle.conf modules scripts; do
  if [ -e "$CFG/hypr/$f" ] && [ ! -L "$CFG/hypr/$f" ]; then
    mv "$CFG/hypr/$f" "$CFG/hypr/$f.bak-$STAMP"
    say "  backed up $f -> $f.bak-$STAMP"
  fi
done
cp -r config/hypr/hyprland.lua config/hypr/hyprlock.conf config/hypr/hypridle.conf \
      config/hypr/modules config/hypr/scripts "$CFG/hypr/"
chmod +x "$CFG/hypr/scripts/"*.sh

say "Installing GTK settings (dark, Fluent icons)"
for v in gtk-3.0 gtk-4.0; do
  mkdir -p "$CFG/$v"
  [ -e "$CFG/$v/settings.ini" ] && cp "$CFG/$v/settings.ini" "$CFG/$v/settings.ini.bak-$STAMP"
  cp "config/$v/settings.ini" "$CFG/$v/settings.ini"
done

cat <<MSG

Done. Next steps:
  1. Make sure ~/.local/bin is in your PATH (the Lua config also adds it for the session).
  2. Install the runtime tools you want (see README.md "Dependencies").
  3. Log out and start Hyprland (or run:  hyprctl reload).
  4. Tweak pinned apps:  $CFG/win11-hypr/config.json   Custom CSS: $CFG/win11-hypr/user.css
MSG
