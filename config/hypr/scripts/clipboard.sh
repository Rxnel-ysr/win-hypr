#!/usr/bin/env bash
# Win+V clipboard history (cliphist + any dmenu-style launcher).
set -euo pipefail
if command -v fuzzel >/dev/null; then pick() { fuzzel --dmenu --prompt "Clipboard  "; }
elif command -v wofi >/dev/null; then pick() { wofi --dmenu --prompt "Clipboard"; }
else pick() { rofi -dmenu -p "Clipboard"; }; fi
cliphist list | pick | cliphist decode | wl-copy
