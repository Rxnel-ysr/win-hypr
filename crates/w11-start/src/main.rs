//! The Windows 11 Start menu: search, pinned grid, recommended files, all-apps list,
//! user + power footer.

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::rc::Rc;
use w11_common::config::Config;
use w11_common::popup::{run_popup, Placement, Popup};
use w11_common::{apps, sys};

fn main() -> glib::ExitCode {
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "cairo");
    }
    run_popup("dev.win11hypr.Start", build)
}

fn build(app: &gtk::Application) -> Rc<Popup> {
    let popup = Popup::new(app, "w11-start", 640, 720, Placement::BottomCenter);
    let hide = {
        let w = popup.clone();
        Rc::new(move || w.hide())
    };

    let flyout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    flyout.add_css_class("flyout");
    flyout.set_size_request(640, 720);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
    body.add_css_class("flyout-body");
    body.set_vexpand(true);

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Type here to search"));
    search.add_css_class("start-search");
    body.append(&search);

    let stack = gtk::Stack::new();
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(120);
    body.append(&stack);

    // ------------------------------------------------------------- home
    let cfg = Config::load();
    let home = gtk::Box::new(gtk::Orientation::Vertical, 14);

    let pin_head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let pin_title = gtk::Label::new(Some("Pinned"));
    pin_title.add_css_class("section-title");
    pin_title.set_hexpand(true);
    pin_title.set_xalign(0.0);
    let all_btn = gtk::Button::with_label("All apps  ›");
    all_btn.add_css_class("link-button");
    pin_head.append(&pin_title);
    pin_head.append(&all_btn);
    home.append(&pin_head);

    let pins = gtk::FlowBox::new();
    pins.set_selection_mode(gtk::SelectionMode::None);
    pins.set_min_children_per_line(6);
    pins.set_max_children_per_line(6);
    pins.set_homogeneous(true);
    pins.set_row_spacing(0);
    pins.set_column_spacing(0);
    let mut pinned_count = 0;
    for id in &cfg.pinned {
        if pinned_count >= 18 {
            break;
        }
        let Some(info) = apps::by_id(id) else {
            continue;
        };
        pins.insert(&pin_tile(&info, hide.clone()), -1);
        pinned_count += 1;
    }
    home.append(&pins);

    let rec_head = gtk::Label::new(Some("Recommended"));
    rec_head.add_css_class("section-title");
    rec_head.set_xalign(0.0);
    rec_head.set_margin_top(8);
    home.append(&rec_head);

    let rec = gtk::FlowBox::new();
    rec.set_selection_mode(gtk::SelectionMode::None);
    rec.set_min_children_per_line(2);
    rec.set_max_children_per_line(2);
    rec.set_homogeneous(true);
    home.append(&rec);
    stack.add_named(&home, Some("home"));

    // --------------------------------------------------------- all apps
    let all_page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let all_head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let all_title = gtk::Label::new(Some("All apps"));
    all_title.add_css_class("section-title");
    all_title.set_hexpand(true);
    all_title.set_xalign(0.0);
    let back_btn = gtk::Button::with_label("‹  Back");
    back_btn.add_css_class("link-button");
    all_head.append(&all_title);
    all_head.append(&back_btn);
    all_page.append(&all_head);
    let all_scroll = gtk::ScrolledWindow::new();
    all_scroll.set_vexpand(true);
    all_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let all_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let all_built = Rc::new(std::cell::Cell::new(false));
    let fill_all = {
        let (all_list, hide) = (all_list.clone(), hide.clone());
        Rc::new(move || {
            let mut last_letter = String::new();
            for info in apps::all() {
                let name = info.name().to_string();
                let letter = name
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default();
                if letter != last_letter {
                    let l = gtk::Label::new(Some(&letter));
                    l.add_css_class("section-title");
                    l.set_xalign(0.0);
                    l.set_margin_top(10);
                    l.set_margin_start(10);
                    all_list.append(&l);
                    last_letter = letter;
                }
                all_list.append(&app_row(&info, hide.clone()));
            }
        })
    };
    all_scroll.set_child(Some(&all_list));
    all_page.append(&all_scroll);
    stack.add_named(&all_page, Some("all"));

    // ----------------------------------------------------------- search
    let results_scroll = gtk::ScrolledWindow::new();
    results_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let results = gtk::Box::new(gtk::Orientation::Vertical, 0);
    results_scroll.set_child(Some(&results));
    stack.add_named(&results_scroll, Some("search"));

    {
        let (st, built, fill) = (stack.clone(), all_built.clone(), fill_all.clone());
        all_btn.connect_clicked(move |_| {
            if !built.replace(true) {
                fill();
            }
            st.set_visible_child_name("all");
        });
        let st = stack.clone();
        back_btn.connect_clicked(move |_| st.set_visible_child_name("home"));
    }

    let all_apps = Rc::new(apps::all());
    {
        let (stack, results, all_apps, hide) = (
            stack.clone(),
            results.clone(),
            all_apps.clone(),
            hide.clone(),
        );
        search.connect_search_changed(move |e| {
            let q = e.text().to_lowercase();
            while let Some(c) = results.first_child() {
                results.remove(&c);
            }
            if q.is_empty() {
                stack.set_visible_child_name("home");
                return;
            }
            let mut matches: Vec<&gio::AppInfo> = all_apps
                .iter()
                .filter(|a| {
                    a.name().to_lowercase().contains(&q)
                        || a.description()
                            .map(|d| d.to_lowercase().contains(&q))
                            .unwrap_or(false)
                })
                .collect();
            // prefix matches first
            matches.sort_by_key(|a| !a.name().to_lowercase().starts_with(&q));
            let head = gtk::Label::new(Some(if matches.is_empty() {
                "No apps found"
            } else {
                "Best match"
            }));
            head.add_css_class("section-title");
            head.set_xalign(0.0);
            head.set_margin_bottom(6);
            results.append(&head);
            for a in matches.into_iter().take(12) {
                results.append(&app_row(a, hide.clone()));
            }
            stack.set_visible_child_name("search");
        });
    }
    {
        let (all_apps, hide) = (all_apps.clone(), hide.clone());
        search.connect_activate(move |e| {
            let q = e.text().to_lowercase();
            if q.is_empty() {
                return;
            }
            let mut m: Vec<&gio::AppInfo> = all_apps
                .iter()
                .filter(|a| a.name().to_lowercase().contains(&q))
                .collect();
            m.sort_by_key(|a| !a.name().to_lowercase().starts_with(&q));
            if let Some(a) = m.first() {
                apps::launch(a);
                hide();
            } else {
                // Not an app: treat it like Win+R and run it.
                sys::spawn(&e.text());
                hide();
            }
        });
    }

    flyout.append(&body);

    // ----------------------------------------------------------- footer
    let footer = gtk::CenterBox::new();
    footer.add_css_class("start-footer");
    let user = gtk::Button::new();
    user.add_css_class("user-pill");
    let name = sys::user_name();
    let ub = gtk::Box::new(gtk::Orientation::Horizontal, 10);
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
    ub.append(&avatar);
    ub.append(&gtk::Label::new(Some(&name)));
    user.set_child(Some(&ub));
    user.set_valign(gtk::Align::Center);
    // No accounts page in a Hyprland-only session: the pill is a pure label.
    user.set_can_target(false);
    user.set_focusable(false);
    footer.set_start_widget(Some(&user));

    let power = gtk::MenuButton::new();
    power.add_css_class("power-button");
    power.set_icon_name("system-shutdown-symbolic");
    power.set_valign(gtk::Align::Center);
    power.set_direction(gtk::ArrowType::Up);
    let pop = gtk::Popover::new();
    pop.set_has_arrow(false);
    let pm = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let actions: [(&str, fn()); 5] = [
        ("Lock", sys::lock),
        ("Sign out", sys::sign_out),
        ("Sleep", sys::sleep),
        ("Shut down", sys::shutdown),
        ("Restart", sys::restart),
    ];
    for (label, f) in actions {
        let b = gtk::Button::with_label(label);
        b.add_css_class("menu-item");
        if let Some(l) = b.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
            l.set_xalign(0.0);
        }
        let (pop, hide) = (pop.clone(), hide.clone());
        b.connect_clicked(move |_| {
            pop.popdown();
            hide();
            f();
        });
        pm.append(&b);
    }
    pop.set_child(Some(&pm));
    power.set_popover(Some(&pop));
    footer.set_end_widget(Some(&power));
    flyout.append(&footer);

    popup.window.set_child(Some(&flyout));

    // Reset state every time the menu opens.
    {
        let (stack, search, rec) = (stack.clone(), search.clone(), rec.clone());
        let hide = hide.clone();
        popup.window.connect_show(move |_| {
            search.set_text("");
            stack.set_visible_child_name("home");
            search.grab_focus();
            fill_recommended(&rec, hide.clone());
        });
    }

    // Free the large All-apps list when the menu closes; rebuilt on demand.
    {
        let (all_list, all_built) = (all_list.clone(), all_built.clone());
        popup.window.connect_hide(move |_| {
            while let Some(c) = all_list.first_child() {
                all_list.remove(&c);
            }
            all_built.set(false);
        });
    }
    popup
}

fn pin_tile(info: &gio::AppInfo, hide: Rc<dyn Fn()>) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("pin-tile");
    let v = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let img = gtk::Image::from_gicon(&apps::icon_of(info));
    img.set_pixel_size(32);
    let label = gtk::Label::new(Some(&info.name()));
    label.add_css_class("pin-label");
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_max_width_chars(12);
    v.append(&img);
    v.append(&label);
    b.set_child(Some(&v));
    let info = info.clone();
    b.connect_clicked(move |_| {
        apps::launch(&info);
        hide();
    });
    b
}

fn app_row(info: &gio::AppInfo, hide: Rc<dyn Fn()>) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("app-row");
    let h = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let img = gtk::Image::from_gicon(&apps::icon_of(info));
    img.set_pixel_size(24);
    let l = gtk::Label::new(Some(&info.name()));
    l.set_xalign(0.0);
    h.append(&img);
    h.append(&l);
    b.set_child(Some(&h));
    let info = info.clone();
    b.connect_clicked(move |_| {
        apps::launch(&info);
        hide();
    });
    b
}

/// "Recommended" = most recently used files from `recently-used.xbel`.
fn fill_recommended(flow: &gtk::FlowBox, hide: Rc<dyn Fn()>) {
    while let Some(c) = flow.first_child() {
        flow.remove(&c);
    }
    let mut items: Vec<(i64, String)> = gtk::RecentManager::default()
        .items()
        .into_iter()
        .map(|i| (i.modified().to_unix(), i.uri().to_string()))
        .filter(|(_, u)| u.starts_with("file://"))
        .collect();
    items.sort_by(|a, b| b.0.cmp(&a.0));
    let mut shown = 0;
    for (modified, uri) in items.into_iter().take(12) {
        if shown >= 6 {
            break;
        }
        let file = gio::File::for_uri(&uri);
        if !file.query_exists(gio::Cancellable::NONE) {
            continue;
        }
        shown += 1;
        let b = gtk::Button::new();
        b.add_css_class("rec-row");
        let h = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let ct = file
            .query_info(
                "standard::icon",
                gio::FileQueryInfoFlags::NONE,
                gio::Cancellable::NONE,
            )
            .ok()
            .and_then(|i| i.icon());
        let img = match ct {
            Some(i) => gtk::Image::from_gicon(&i),
            None => gtk::Image::from_icon_name("text-x-generic"),
        };
        img.set_pixel_size(28);
        let v = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let title = gtk::Label::new(Some(
            &file
                .basename()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        ));
        title.add_css_class("rec-title");
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(24);
        let when = glib::DateTime::from_unix_local(modified)
            .ok()
            .and_then(|d| d.format("%b %-d, %-I:%M %p").ok())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let sub = gtk::Label::new(Some(&when));
        sub.add_css_class("rec-sub");
        sub.set_xalign(0.0);
        v.append(&title);
        v.append(&sub);
        h.append(&img);
        h.append(&v);
        b.set_child(Some(&h));
        let hide = hide.clone();
        b.connect_clicked(move |_| {
            let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
            hide();
        });
        flow.insert(&b, -1);
    }
}
