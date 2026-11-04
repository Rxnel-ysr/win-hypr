local function set(t)
  local ok, err = pcall(hl.config, t)
  if not ok then print("win11-hypr perf: skipped (" .. tostring(err) .. ")") end
end

-- xray = blur only the wallpaper, not every window underneath: the biggest iGPU win
set({ decoration = { blur = { enabled = false, xray = false, passes = 2, size = 5, noise = 0, vibrancy = 0, popups = false }, shadow = { enabled = false , range = 14, render_power = 2 } } })
set({ misc = { animate_manual_resizes = false, animate_mouse_windowdragging = false } })
set({ ecosystem = { no_update_news = true } })
