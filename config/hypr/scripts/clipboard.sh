#!/usr/bin/env bash
# Win+V clipboard history (cliphist + any dmenu-style launcher).
set -euo pipefail
if command -v fuzzel >/dev/null; then pick() { fuzzel --dmenu --prompt "Clipboard  "; }
elif command -v wofi >/dev/null; then pick() { wofi --dmenu --prompt "Clipboard"; }
else pick() { rofi -dmenu -p "Clipboard"; }; fi
sel="$(cliphist list | pick)" || exit 0
[ -n "$sel" ] || exit 0
printf '%s\n' "$sel" | cliphist decode | wl-copy
