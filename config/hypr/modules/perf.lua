local function set(t)
  local ok, err = pcall(hl.config, t)
  if not ok then print("win11-hypr perf: skipped (" .. tostring(err) .. ")") end
end

-- xray = blur only the wallpaper, not every window underneath: the biggest iGPU win
local lite = (LITE ~= false)  -- LITE comes from look.lua; nil counts as lite

if lite then
    -- nothing expensive runs in the compositor
    set({ decoration = { blur = { enabled = false }, shadow = { enabled = false } } })
else
    -- full look, cheapest acrylic: xray = (all-floating) windows blur only the wallpaper
    set({ decoration = { blur = { enabled = true, xray = true, passes = 2, size = 5, noise = 0, vibrancy = 0, popups = false }, shadow = { enabled = true, range = 14, render_power = 2 } } })
end

set({ misc = { vfr = true, animate_manual_resizes = false, animate_mouse_windowdragging = false } })
set({ ecosystem = { no_update_news = true,  no_donation_nag = true } })
