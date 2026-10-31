-- Input ---------------------------------------------------------------------
hl.config({
  input = {
    kb_layout = "us",
    numlock_by_default = true,

    -- Windows is click-to-focus
    follow_mouse = 0,
    float_switch_override_focus = 0,

    -- Windows' default touchpad behaviour
    touchpad = {
      natural_scroll = false,
      tap_to_click = true,
      clickfinger_behavior = true,
      disable_while_typing = true,
    },
  },
})
