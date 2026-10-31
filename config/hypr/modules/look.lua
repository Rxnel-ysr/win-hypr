-- Look & feel: Windows 11 (dark) -------------------------------------------
-- Rounded 8px windows, hairline border, big soft shadow, "acrylic" blur.
hl.config({
  general = {
    gaps_in = 0,
    gaps_out = 0,
    border_size = 1,
    col = {
      active_border   = { colors = { "rgba(60cdff55)" }, angle = 45 },
      inactive_border = { colors = { "rgba(ffffff14)" }, angle = 45 },
    },
    resize_on_border = true,
    extend_border_grab_area = 10,
    allow_tearing = false,
    layout = "dwindle",
  },

  decoration = {
    rounding = 8,
    rounding_power = 2,
    active_opacity = 1.0,
    inactive_opacity = 1.0,

    shadow = {
      enabled = true,
      range = 28,
      render_power = 3,
      color = 0x66000000,
    },

    blur = {
      enabled = true,
      size = 8,
      passes = 3,
      noise = 0.02,
      vibrancy = 0.17,
      brightness = 0.9,
    },
  },

  misc = {
    force_default_wallpaper = 0,
    disable_hyprland_logo = true,
    disable_splash_rendering = true,
    background_color = 0xff0b2a5b,   -- Windows 11 "Bloom" blue until a wallpaper loads
    focus_on_activate = true,
    mouse_move_enables_dpms = true,
    key_press_enables_dpms = true,
  },

  cursor = {
    no_hardware_cursors = false,
  },
})
