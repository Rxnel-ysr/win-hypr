#!/usr/bin/env bash
# win11-hypr installer
#
#   ./install.sh                 interactive: install dependencies, build, install, configure
#   ./install.sh -y              non-interactive (accept defaults)
#   ./install.sh --no-deps       skip dependency installation
#   ./install.sh --no-aur        skip AUR packages (icons, cursor, fonts)
#   ./install.sh --no-build      reuse an existing target/release build
#   ./install.sh --no-config     install binaries only, leave ~/.config alone
#   ./install.sh --uninstall     remove binaries and desktop entries (add --purge for user data)
#
# Targets Arch Linux (pacman). On other distros it skips package installation,
# checks what is missing and tells you what to install.

set -Eeuo pipefail
cd "$(dirname "$(readlink -f "$0")")"

# ------------------------------------------------------------------ settings
BIN="${XDG_BIN_HOME:-$HOME/.local/bin}"
CFG="${XDG_CONFIG_HOME:-$HOME/.config}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS="$DATA/applications"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}"
LOG="$CACHE/win11-hypr-install.log"
STAMP="$(date +%Y%m%d-%H%M%S)"

BINARIES=(w11-taskbar w11-start w11-flyout w11-notify w11-explorer w11-settings w11ctl)
DESKTOP_FILES=(dev.win11hypr.Explorer.desktop dev.win11hypr.Settings.desktop)
MIN_RUST="1.87.0"
MIN_HYPRLAND="0.55.0"

# Packages in the official repos. Everything the shell shells out to.
REQUIRED_PKGS=(
  base-devel git gtk4 gtk4-layer-shell pkgconf
  hyprland hyprlock hypridle hyprsunset hyprpolkitagent xdg-desktop-portal-hyprland
  grim slurp wl-clipboard cliphist swaybg foot fuzzel
  wireplumber brightnessctl networkmanager bluez bluez-utils power-profiles-daemon
  playerctl curl pacman-contrib xdg-utils desktop-file-utils polkit ttf-inter
)
OPTIONAL_PKGS=(satty bottom pavucontrol blueman nm-connection-editor mpv imv btop kitty zram-generator)
# AUR (best effort; the shell works without them, it just looks less like Windows)
AUR_PKGS=(fluent-icon-theme-git bibata-cursor-theme ttf-selawik xdg-terminal-exec)

# -------------------------------------------------------------------- flags
ASSUME_YES=0 DO_DEPS=1 DO_AUR=1 DO_BUILD=1 DO_CONFIG=1 DO_UNINSTALL=0 DO_PURGE=0

usage() { sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

for a in "$@"; do
  case "$a" in
    -y|--yes)      ASSUME_YES=1 ;;
    --no-deps)     DO_DEPS=0 ;;
    --no-aur)      DO_AUR=0 ;;
    --no-build)    DO_BUILD=0 ;;
    --no-config)   DO_CONFIG=0 ;;
    --uninstall)   DO_UNINSTALL=1 ;;
    --purge)       DO_PURGE=1 ;;
    -h|--help)     usage 0 ;;
    *) printf 'unknown option: %s\n\n' "$a" >&2; usage 2 ;;
  esac
done

# ------------------------------------------------------------------ helpers
if [ -t 1 ]; then
  C_B=$'\033[1m' C_C=$'\033[1;36m' C_G=$'\033[1;32m' C_Y=$'\033[1;33m' C_R=$'\033[1;31m' C_0=$'\033[0m'
else
  C_B='' C_C='' C_G='' C_Y='' C_R='' C_0=''
fi
step() { printf '\n%s==>%s %s%s%s\n' "$C_C" "$C_0" "$C_B" "$*" "$C_0"; }
say()  { printf '    %s\n' "$*"; }
ok()   { printf '  %s✔%s %s\n' "$C_G" "$C_0" "$*"; }
warn() { printf '  %s!%s %s\n' "$C_Y" "$C_0" "$*" >&2; }
die()  { printf '%serror:%s %s\n' "$C_R" "$C_0" "$*" >&2; exit 1; }

mkdir -p "$CACHE"
: > "$LOG"
trap 'printf "%serror:%s step failed at line %s (log: %s)\n" "$C_R" "$C_0" "$LINENO" "$LOG" >&2' ERR

# ask "question" [default y|n]
ask() {
  local q="$1" def="${2:-y}" reply hint="[Y/n]"
  [ "$def" = n ] && hint="[y/N]"
  if [ "$ASSUME_YES" = 1 ]; then return 0; fi
  if [ ! -t 0 ]; then [ "$def" = y ]; return; fi
  read -rp "    $q $hint " reply || reply=""
  reply="${reply:-$def}"
  [[ "$reply" =~ ^[Yy] ]]
}

have() { command -v "$1" >/dev/null 2>&1; }

# ver_ge A B  -> true when A >= B
ver_ge() { [ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" = "$2" ]; }

is_arch() {
  [ -r /etc/os-release ] || return 1
  # shellcheck disable=SC1091
  . /etc/os-release
  [ "${ID:-}" = arch ] || [[ " ${ID_LIKE:-} " == *" arch "* ]]
}

missing_pkgs() { # prints packages from args that are not installed
  local p
  for p in "$@"; do pacman -Qq "$p" >/dev/null 2>&1 || printf '%s\n' "$p"; done
}

# --------------------------------------------------------------- uninstall
if [ "$DO_UNINSTALL" = 1 ]; then
  step "Uninstalling win11-hypr"
  for b in "${BINARIES[@]}"; do
    [ -e "$BIN/$b" ] && rm -f "$BIN/$b" && ok "removed $BIN/$b"
  done
  for d in "${DESKTOP_FILES[@]}"; do
    [ -e "$APPS/$d" ] && rm -f "$APPS/$d" && ok "removed $APPS/$d"
  done
  update-desktop-database "$APPS" 2>/dev/null || true
  pkill -x w11-taskbar 2>/dev/null || true
  pkill -x w11-notify 2>/dev/null || true
  if [ "$DO_PURGE" = 1 ]; then
    rm -rf "$CFG/win11-hypr" "$HOME/.local/state/win11-hypr"
    ok "removed user data (config.json, user.css, notification history)"
  else
    say "kept $CFG/win11-hypr (use --purge to delete it)"
  fi
  say "Hyprland config in $CFG/hypr was left untouched."
  say "Restore a backup with e.g.: cp -r $CFG/hypr/hyprland.lua.bak-<timestamp> $CFG/hypr/hyprland.lua"
  exit 0
fi

# --------------------------------------------------------------- preflight
step "Preflight"
[ "$(id -u)" -ne 0 ] || die "do not run as root; sudo is used only when needed"
[ -f Cargo.toml ] && [ -d crates ] || die "run this script from the win11-hypr repository"

if is_arch; then ok "Arch Linux detected"; else
  warn "not Arch: package installation is skipped, you must install the dependencies listed in README.md yourself"
  DO_DEPS=0
fi

mem_kb="$(awk '/MemTotal/ {print $2}' /proc/meminfo)"
say "RAM: $((mem_kb / 1024)) MiB, CPU threads: $(nproc)"

# ------------------------------------------------------------ dependencies
if [ "$DO_DEPS" = 1 ]; then
  step "Installing dependencies (pacman)"
  have sudo || die "sudo is required to install packages (or re-run with --no-deps)"

  pkgs=("${REQUIRED_PKGS[@]}")
  have cargo || pkgs+=(rust)
  mapfile -t todo < <(missing_pkgs "${pkgs[@]}")

  if [ "${#todo[@]}" -eq 0 ]; then
    ok "all required packages are already installed"
  else
    say "missing: ${todo[*]}"
    if ask "Install ${#todo[@]} required package(s) with pacman?" y; then
      sudo pacman -S --needed ${ASSUME_YES:+--noconfirm} "${todo[@]}" 2>&1 | tee -a "$LOG"
      ok "required packages installed"
    else
      warn "skipped; the build or runtime may fail"
    fi
  fi

  mapfile -t opt < <(missing_pkgs "${OPTIONAL_PKGS[@]}")
  if [ "${#opt[@]}" -gt 0 ]; then
    say "optional extras not installed: ${opt[*]}"
    say "(satty = snip markup, bottom = Task Manager, pavucontrol/blueman/nm-connection-editor = advanced panels, zram-generator = RAM saver)"
    if ask "Install optional extras?" n; then
      sudo pacman -S --needed ${ASSUME_YES:+--noconfirm} "${opt[@]}" 2>&1 | tee -a "$LOG" || warn "some optional packages failed"
    fi
  fi

  if [ "$DO_AUR" = 1 ]; then
    step "Look & feel packages (AUR)"
    mapfile -t aur_todo < <(missing_pkgs "${AUR_PKGS[@]}")
    if [ "${#aur_todo[@]}" -eq 0 ]; then
      ok "icon theme, cursor and fonts already installed"
    else
      helper=""
      for h in paru yay; do have "$h" && helper="$h" && break; done
      say "AUR packages: ${aur_todo[*]}"
      if [ -z "$helper" ]; then
        warn "no AUR helper (paru/yay) found. Install one, then run:  paru -S ${aur_todo[*]}"
      elif ask "Install them with $helper?" y; then
        # Installed one by one so a single failing package does not abort the rest.
        for p in "${aur_todo[@]}"; do
          if "$helper" -S --needed ${ASSUME_YES:+--noconfirm} "$p" 2>&1 | tee -a "$LOG"; then
            ok "$p"
          else
            warn "$p failed (the shell still works without it)"
          fi
        done
      fi
    fi
  fi

  step "System services"
  want=()
  for s in NetworkManager bluetooth power-profiles-daemon; do
    systemctl is-enabled "$s" >/dev/null 2>&1 || want+=("$s")
  done
  if [ "${#want[@]}" -eq 0 ]; then
    ok "NetworkManager, bluetooth and power-profiles-daemon are enabled"
  elif ask "Enable and start: ${want[*]}?" y; then
    sudo systemctl enable --now "${want[@]}" 2>&1 | tee -a "$LOG" || warn "could not enable some services"
  fi
  systemctl --user enable --now hyprpolkitagent.service >/dev/null 2>&1 || true
fi

# ------------------------------------------------------------- toolchain
step "Checking toolchain"
have cargo || die "cargo not found (install 'rust' or rustup)"
rv="$(rustc --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -n1)"
ver_ge "$rv" "$MIN_RUST" || die "Rust $rv is too old, need >= $MIN_RUST (run: rustup update)"
ok "Rust $rv"
have pkg-config || die "pkg-config not found"
pkg-config --exists gtk4 || die "GTK4 development files missing (Arch: gtk4)"
pkg-config --exists gtk4-layer-shell-0 || die "gtk4-layer-shell missing (Arch: gtk4-layer-shell)"
ok "GTK4 and gtk4-layer-shell found"

if have Hyprland; then
  hv="$(Hyprland --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -n1 || true)"
  if [ -n "$hv" ] && ver_ge "$hv" "$MIN_HYPRLAND"; then
    ok "Hyprland $hv"
  else
    warn "Hyprland ${hv:-unknown} detected; the Lua config needs >= $MIN_HYPRLAND"
  fi
else
  warn "Hyprland not found in PATH"
fi

# ------------------------------------------------------------------ build
if [ "$DO_BUILD" = 1 ]; then
  step "Building the Rust shell (release, fat LTO)"
  # fat LTO + codegen-units=1 is memory hungry at link time; keep parallelism modest on small machines
  if [ -z "${CARGO_BUILD_JOBS:-}" ] && [ "$mem_kb" -lt 10000000 ]; then
    export CARGO_BUILD_JOBS=2
    say "≤ 8 GB RAM: limiting cargo to 2 parallel jobs"
  fi
  say "this takes a few minutes the first time"
  lock_flag=()
  [ -f Cargo.lock ] && lock_flag=(--locked)
  cargo build --release --workspace "${lock_flag[@]}" 2>&1 | tee -a "$LOG"
  ok "build finished"
else
  step "Skipping build (--no-build)"
fi

for b in "${BINARIES[@]}"; do
  [ -x "target/release/$b" ] || die "target/release/$b missing; build first or drop --no-build"
done

# ------------------------------------------------------------ install bins
step "Installing binaries to $BIN"
mkdir -p "$BIN"
# stop running shell parts so the files can be replaced cleanly
for p in w11-taskbar w11-notify; do pkill -x "$p" 2>/dev/null || true; done
for b in "${BINARIES[@]}"; do
  install -Dm755 "target/release/$b" "$BIN/$b"
done
ok "${#BINARIES[@]} binaries installed"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) warn "$BIN is not in your PATH. The Hyprland config adds it for the session, but add it to your shell profile too." ;;
esac

step "Desktop entries and default apps"
mkdir -p "$APPS"
for d in "${DESKTOP_FILES[@]}"; do
  install -Dm644 "config/applications/$d" "$APPS/$d"
done
update-desktop-database "$APPS" 2>/dev/null || true
xdg-mime default dev.win11hypr.Explorer.desktop inode/directory 2>/dev/null || true
ok "File Explorer is now the default folder handler"

# ----------------------------------------------------------------- config
if [ "$DO_CONFIG" = 1 ]; then
  step "Installing Hyprland config to $CFG/hypr"
  mkdir -p "$CFG/hypr"
  kept_monitors=""
  for f in hyprland.lua hyprland.conf hyprlock.conf hypridle.conf modules scripts; do
    if [ -e "$CFG/hypr/$f" ] && [ ! -L "$CFG/hypr/$f" ]; then
      mv "$CFG/hypr/$f" "$CFG/hypr/$f.bak-$STAMP"
      say "backed up $f -> $f.bak-$STAMP"
    fi
  done
  cp -r config/hypr/hyprland.lua config/hypr/hyprlock.conf config/hypr/hypridle.conf \
        config/hypr/modules config/hypr/scripts "$CFG/hypr/"
  chmod +x "$CFG/hypr/scripts/"*.sh
  # keep the user's display layout across re-installs
  if [ -f "$CFG/hypr/modules.bak-$STAMP/monitors.lua" ]; then
    cp "$CFG/hypr/modules.bak-$STAMP/monitors.lua" "$CFG/hypr/modules/monitors.lua"
    kept_monitors=1
  fi
  ok "Hyprland config installed${kept_monitors:+ (your previous monitors.lua was kept)}"

  step "GTK settings (dark, Fluent icons, Selawik, Bibata)"
  for v in gtk-3.0 gtk-4.0; do
    mkdir -p "$CFG/$v"
    [ -e "$CFG/$v/settings.ini" ] && cp "$CFG/$v/settings.ini" "$CFG/$v/settings.ini.bak-$STAMP"
    cp "config/$v/settings.ini" "$CFG/$v/settings.ini"
  done
  ok "GTK 3 and 4 settings installed"

  step "User directories"
  mkdir -p "$CFG/win11-hypr" "$HOME/Pictures/Wallpapers" "$HOME/Pictures/Screenshots"
  ok "created $CFG/win11-hypr and ~/Pictures/{Wallpapers,Screenshots}"
  [ -f "$HOME/Pictures/Wallpapers/bloom.jpg" ] || say "put a wallpaper at ~/Pictures/Wallpapers/bloom.jpg, or pick one later in Settings > Personalization"
  [ -f "$HOME/.face" ] || say "put a profile picture at ~/.face for the lock screen, or set it in Settings > Accounts"
else
  step "Skipping configuration (--no-config)"
fi

# ---------------------------------------------------------- health checks
step "Post-install checks"
[ -d /usr/share/icons/Fluent-dark ] || [ -d "$DATA/icons/Fluent-dark" ] || warn "Fluent-dark icon theme not found; icons will fall back to Adwaita"
fc-list 2>/dev/null | grep -qi selawik || warn "Selawik font not found; falling back to Inter / Noto Sans"
for n in mako dunst swaync; do
  if pgrep -x "$n" >/dev/null 2>&1 || { is_arch && pacman -Qq "$n" >/dev/null 2>&1; }; then
    warn "$n is installed/running and will fight w11-notify for org.freedesktop.Notifications. Remove it: sudo pacman -Rns $n"
  fi
done
have xdg-terminal-exec || say "xdg-terminal-exec not found: Ctrl+Shift+Esc (Task Manager) falls back to 'kitty btop'"
have btm || have btop || say "install 'bottom' or 'btop' for the Task Manager shortcut"

cat <<MSG

${C_G}Done.${C_0}

  Next steps
    1. Log out and start Hyprland (from a TTY: ${C_B}Hyprland${C_0}), or run ${C_B}hyprctl reload${C_0} in an existing session.
    2. Tweak pinned apps:  $CFG/win11-hypr/config.json
       Custom CSS:         $CFG/win11-hypr/user.css
       Displays:           $CFG/hypr/modules/monitors.lua
       Blur/animations:    set LITE = false in $CFG/hypr/modules/look.lua (costs RAM/GPU)
    3. Low on RAM? See "Performance and memory design" in the README.

  Re-run this script any time to update. Existing config is backed up as *.bak-$STAMP.
  Uninstall:  ./install.sh --uninstall [--purge]
  Install log: $LOG
MSG
