-- Keybindings: the Windows 11 shortcut set ----------------------------------
local Win = "SUPER"

local function exec(keys, cmd, opts)
  hl.bind(keys, hl.dsp.exec_cmd(cmd), opts)
end

-- ---------------------------------------------------------------- Start & shell
-- Tap the Windows key -> Start menu (fires on release, like Windows)
hl.bind(Win .. " + SUPER_L", hl.dsp.exec_cmd("w11-start"), { release = true })

exec(Win .. " + S",         "w11-start")                 -- Search
exec(Win .. " + R",         "w11-start")                 -- Run (type a command + Enter)
exec(Win .. " + A",         "w11-flyout quick")          -- Quick Settings
exec(Win .. " + N",         "w11-flyout calendar")       -- Notification center + calendar
exec(Win .. " + W",         "w11-flyout calendar")       -- Widgets
exec(Win .. " + Tab",       "w11-flyout taskview")       -- Task View
exec("ALT + Tab",           "w11ctl alt-tab")            -- Quick switch
exec(Win .. " + E",         "w11-explorer")              -- File Explorer
exec(Win .. " + I",         "gnome-control-center || systemsettings")  -- Settings
exec(Win .. " + L",         "w11ctl lock")               -- Lock
exec(Win .. " + D",         "w11ctl show-desktop")       -- Show / restore desktop
exec(Win .. " + M",         "w11ctl show-desktop")       -- Minimize all
exec("CTRL + SHIFT + Escape", "xdg-terminal-exec btm || kitty btm || btop")  -- Task Manager
exec(Win .. " + V",         "~/.config/hypr/scripts/clipboard.sh")           -- Clipboard history
exec(Win .. " + SHIFT + S", "~/.config/hypr/scripts/screenshot.sh region")   -- Snipping tool
exec("Print",               "~/.config/hypr/scripts/screenshot.sh full")
exec(Win .. " + Print",     "~/.config/hypr/scripts/screenshot.sh full")
exec(Win .. " + X",         "w11-start")                 -- (Power-user menu lives in Start's power button)

-- ---------------------------------------------------------------- Window management
hl.bind("ALT + F4", hl.dsp.window.close())
hl.bind("F11",      hl.dsp.window.fullscreen({ mode = "fullscreen", action = "toggle" }))

-- Snap layouts: Win + arrows (and Win+Z style quarters with Alt)
exec(Win .. " + Up",                   "w11ctl snap maximize")
exec(Win .. " + Down",                 "w11ctl restore")
exec(Win .. " + Left",                 "w11ctl snap left")
exec(Win .. " + Right",                "w11ctl snap right")
exec(Win .. " + ALT + Left",           "w11ctl snap top-left")
exec(Win .. " + ALT + Right",          "w11ctl snap top-right")
exec(Win .. " + ALT + Up",             "w11ctl snap maximize")
exec(Win .. " + ALT + Down",           "w11ctl snap bottom-right")
exec(Win .. " + CTRL + ALT + Left",    "w11ctl snap bottom-left")

-- Move window to the next / previous display
hl.bind(Win .. " + SHIFT + Left",  hl.dsp.window.move({ monitor = "l" }))
hl.bind(Win .. " + SHIFT + Right", hl.dsp.window.move({ monitor = "r" }))

-- Drag / resize with the Windows key held (titlebars and borders work too)
hl.bind(Win .. " + mouse:272", hl.dsp.window.drag(),   { mouse = true })
hl.bind(Win .. " + mouse:273", hl.dsp.window.resize(), { mouse = true })

-- ---------------------------------------------------------------- Virtual desktops
hl.bind(Win .. " + CTRL + Left",  hl.dsp.focus({ workspace = "e-1" }))
hl.bind(Win .. " + CTRL + Right", hl.dsp.focus({ workspace = "e+1" }))
for i = 1, 4 do
  hl.bind(Win .. " + CTRL + " .. i,           hl.dsp.focus({ workspace = i }))
  hl.bind(Win .. " + CTRL + SHIFT + " .. i,   hl.dsp.window.move({ workspace = i }))
end

-- ---------------------------------------------------------------- Media / hardware keys
local locked = { locked = true, repeating = true }
exec("XF86AudioRaiseVolume",  "wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+", locked)
exec("XF86AudioLowerVolume",  "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-",        locked)
exec("XF86AudioMute",         "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle",       locked)
exec("XF86AudioMicMute",      "wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle",     locked)
exec("XF86MonBrightnessUp",   "brightnessctl set 5%+",                            locked)
exec("XF86MonBrightnessDown", "brightnessctl set 5%-",                            locked)
exec("XF86AudioNext",  "playerctl next",       { locked = true })
exec("XF86AudioPrev",  "playerctl previous",   { locked = true })
exec("XF86AudioPlay",  "playerctl play-pause", { locked = true })
exec("XF86AudioPause", "playerctl play-pause", { locked = true })
