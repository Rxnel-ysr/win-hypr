#!/usr/bin/env bash
# Snipping Tool: region (Win+Shift+S) or full screen (PrtSc).
# Prefers the Rust tools (wayshot + satty); falls back to grim/slurp/swappy.
set -euo pipefail
mode="${1:-region}"
dir="${XDG_PICTURES_DIR:-$HOME/Pictures}/Screenshots"
mkdir -p "$dir"
file="$dir/Screenshot $(date +'%Y-%m-%d %H%M%S').png"

capture() {
  if [ "$mode" = region ]; then
    geom="$(slurp -d)" || exit 0
    if command -v wayshot >/dev/null; then wayshot -s "$geom" --stdout
    else grim -g "$geom" -; fi
  else
    if command -v wayshot >/dev/null; then wayshot --stdout
    else grim -; fi
  fi
}

tmp="$(mktemp --suffix=.png)"
trap 'rm -f "$tmp"' EXIT
capture > "$tmp"
wl-copy < "$tmp"

if [ "$mode" = region ]; then
  # Like Snipping Tool: open the markup editor, which saves/copies on exit
  if command -v satty >/dev/null; then
    satty --filename "$tmp" --output-filename "$file" --copy-command wl-copy --early-exit
  elif command -v swappy >/dev/null; then
    swappy -f "$tmp" -o "$file"
  else
    cp "$tmp" "$file"
  fi
else
  cp "$tmp" "$file"
fi

notify-send -a "Snipping Tool" -i camera-photo-symbolic "Screenshot copied to clipboard" "Saved to $(basename "$file")" || true
