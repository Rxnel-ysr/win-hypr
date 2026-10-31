-- Monitors ------------------------------------------------------------------
-- Run `hyprctl monitors` to find output names; add one hl.monitor per display.
hl.monitor({
  output   = "",
  mode     = "preferred",
  position = "auto",
  scale    = "auto",
})

-- Example (Windows-style "extend these displays"):
-- hl.monitor({ output = "DP-1",     mode = "2560x1440@144", position = "0x0",    scale = 1 })
-- hl.monitor({ output = "HDMI-A-1", mode = "1920x1080@60",  position = "2560x0", scale = 1 })
