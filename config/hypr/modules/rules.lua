-- Window & layer rules --------------------------------------------------------
-- Windows behaviour: every window floats, new windows open centred.

hl.window_rule({
  name  = "w11-float-everything",
  match = { class = ".*" },
  float = true,
})

-- Rules whose effect names I could not verify against every 0.55.x build are
-- wrapped in pcall, so an unknown field can never take the whole config down.
local function try(fn, arg)
  local ok, err = pcall(fn, arg)
  if not ok then
    print("win11-hypr: skipped rule (" .. tostring(err) .. ")")
  end
end

try(hl.window_rule, {
  name  = "w11-center-new-windows",
  match = { class = ".*", float = true },
  center = true,
})

try(hl.window_rule, {
  name  = "w11-explorer-size",
  match = { class = "dev.win11hypr.Explorer" },
  size  = "1100 680",
})

-- Small utility dialogs stay small and centred
try(hl.window_rule, {
  name  = "w11-dialogs",
  match = { class = "(pavucontrol|nm-connection-editor|blueman-manager|org.gnome.Calculator)" },
  center = true,
})

-- Frosted-glass ("acrylic") for the Rust shell's layer surfaces.
-- The click-catcher (w11-scrim) is deliberately NOT blurred.
if LITE == false then
  try(hl.layer_rule, {
    name  = "w11-shell-blur",
    match = { namespace = "w11-(taskbar|start|quick|calendar|toast|taskview)" },
    blur  = true,
    ignore_alpha = 0.25,
  })
end
