//! Start menu footer: user pill + power menu.

use crate::ctx::Ctx;
use gtk::prelude::*;
use w11_common::sys;

pub fn build(ctx: &Ctx) -> gtk::CenterBox {
    let footer = gtk::CenterBox::new();
    footer.add_css_class("start-footer");
    footer.set_start_widget(Some(&user_pill()));
    footer.set_end_widget(Some(&power_button(ctx)));
    footer
}

fn user_pill() -> gtk::Button {
    let name = sys::user_name();
    let user = gtk::Button::new();
    user.add_css_class("user-pill");

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let avatar = gtk::Label::new(Some(
        &name
            .chars()
            .next()
            .unwrap_or('U')
            .to_uppercase()
            .to_string(),
    ));
    avatar.add_css_class("avatar");
    avatar.set_size_request(32, 32);
    row.append(&avatar);
    row.append(&gtk::Label::new(Some(&name)));

    user.set_child(Some(&row));
    user.set_valign(gtk::Align::Center);
    // No accounts page in a Hyprland-only session: the pill is a pure label.
    user.set_can_target(false);
    user.set_focusable(false);
    user
}

fn power_button(ctx: &Ctx) -> gtk::MenuButton {
    let power = gtk::MenuButton::new();
    power.add_css_class("power-button");
    power.set_icon_name("system-shutdown-symbolic");
    power.set_valign(gtk::Align::Center);
    power.set_direction(gtk::ArrowType::Up);

    let pop = gtk::Popover::new();
    pop.set_has_arrow(false);
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let actions: [(&str, fn()); 5] = [
        ("Lock", sys::lock),
        ("Sign out", sys::sign_out),
        ("Sleep", sys::sleep),
        ("Shut down", sys::shutdown),
        ("Restart", sys::restart),
    ];
    for (label, action) in actions {
        let b = gtk::Button::with_label(label);
        b.add_css_class("menu-item");
        if let Some(l) = b.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
            l.set_xalign(0.0);
        }
        let (pop, hide) = (pop.clone(), ctx.hide.clone());
        b.connect_clicked(move |_| {
            pop.popdown();
            hide();
            action();
        });
        menu.append(&b);
    }
    pop.set_child(Some(&menu));
    power.set_popover(Some(&pop));
    power
}
