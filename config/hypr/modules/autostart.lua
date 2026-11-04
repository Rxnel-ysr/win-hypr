-- Autostart -----------------------------------------------------------------
hl.on("hyprland.start", function()
    -- Session plumbing
    hl.exec_cmd(
    "dbus-update-activation-environment --systemd WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE")
    hl.exec_cmd("systemctl --user start hyprpolkitagent")
    -- Theme
    hl.exec_cmd("gsettings set org.gnome.desktop.interface color-scheme prefer-dark")
    hl.exec_cmd("gsettings set org.gnome.desktop.interface font-name 'Selawik 10'")

    -- The Rust shell
    hl.exec_cmd("w11-notify") -- toasts + notification history
    hl.exec_cmd("w11-taskbar") -- taskbar, tray, clock

    -- Wallpaper (wpaperd is Rust; swaybg is the fallback)
    -- hl.exec_cmd("command -v f >/dev/null && wpaperd || swaybg -m fill -i ~/Pictures/Wallpapers/bloom.jpg")
    hl.exec_cmd([[sh -c 'wp=$(cat ~/.config/win11-hypr/wallpaper.txt 2>/dev/null); [ -f "$wp" ] || wp=$HOME/Pictures/Wallpapers/bloom.jpg; exec swaybg -m fill -i "$wp"']])

    -- Clipboard history (Win+V) and idle/lock
    hl.exec_cmd("wl-paste --type text --watch cliphist store")
    hl.exec_cmd("wl-paste --type image --watch cliphist store")
    hl.exec_cmd("hypridle")
end)
