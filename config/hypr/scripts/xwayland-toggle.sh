
#!/usr/bin/env bash

current=$(hyprctl getoption xwayland:enabled -j | jq -r '.int')

if [[ "$current" == "1" ]]; then
    hyprctl keyword xwayland:enabled false
    notify-send "Xwayland" "X11 compatibility disabled"
else
    hyprctl keyword xwayland:enabled true
    notify-send "Xwayland" "X11 compatibility enabled"
fi
