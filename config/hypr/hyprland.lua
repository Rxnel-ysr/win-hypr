-- ~/.config/hypr/hyprland.lua
-- win11-hypr: a Windows 11 desktop on Hyprland (>= 0.55, Lua config) with a Rust shell.
-- Everything is split into modules; edit modules/*.lua to taste.

require("modules.env")
require("modules.monitors")
require("modules.input")
require("modules.look")
require("modules.perf")
require("modules.animations")
require("modules.rules")
require("modules.binds")
require("modules.autostart")
