-- XWayland on/off, controlled by Settings > System > Performance --------------
-- Flag file present => XWayland disabled (saves RAM). Re-evaluated on every
-- `hyprctl reload`, so the Settings toggle just writes/removes the flag and reloads.
local cfg  = os.getenv("XDG_CONFIG_HOME") or ((os.getenv("HOME") or "") .. "/.config")
local f    = io.open(cfg .. "/win11-hypr/xwayland-off", "r")
local off  = f ~= nil
if f then f:close() end

local ok, err = pcall(hl.config, { xwayland = { enabled = not off } })
if not ok then print("win11-hypr: xwayland toggle skipped (" .. tostring(err) .. ")") end
