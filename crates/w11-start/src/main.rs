//! The Windows 11 Start menu: search, pinned grid, recommended files, all-apps list,
//! user + power footer.

mod all_apps;
mod ctx;
mod footer;
mod home;
mod index;
mod search;
mod widgets;
use gtk::glib;
use gtk::prelude::*;
use std::rc::Rc;
use w11_common::popup::{run_popup, Placement, Popup};

use crate::ctx::{Ctx, HideFn};
use crate::index::AppCache;

fn main() -> glib::ExitCode {
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "cairo");
    }
    run_popup("dev.win11hypr.Start", build)
}

fn build(app: &gtk::Application) -> Rc<Popup> {
    w11_common::load_css!("css/start.css");

    let popup = Popup::new(app, "w11-start", 640, 720, Placement::BottomCenter);
    let hide: HideFn = {
        let w = popup.clone();
        Rc::new(move || w.hide())
    };
    let ctx = Ctx {
        hide,
        cache: Rc::new(AppCache::default()),
    };

    let flyout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    flyout.add_css_class("flyout");
    flyout.set_size_request(640, 720);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
    body.add_css_class("flyout-body");
    body.set_vexpand(true);

    // Pages
    let search = search::Search::new();
    let home = Rc::new(home::Home::new(&ctx));
    let all = Rc::new(all_apps::AllApps::new(&ctx));

    let stack = gtk::Stack::new();
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::None);
    stack.add_named(&home.root, Some("home"));
    stack.add_named(&all.page, Some("all"));
    stack.add_named(&search.page, Some("search"));

    body.append(&search.entry);
    body.append(&stack);

    // Navigation: Home <-> All apps
    {
        let st = stack.clone();
        all.back_btn
            .connect_clicked(move |_| st.set_visible_child_name("home"));
    }
    {
        let (st, all_page, ctx) = (stack.clone(), all.clone(), ctx.clone());
        home.all_btn.connect_clicked(move |_| {
            all_page.ensure_built(&ctx);
            st.set_visible_child_name("all");
        });
    }
    search.wire(&stack, &ctx);

    flyout.append(&body);
    flyout.append(&footer::build(&ctx));
    popup.window.set_child(Some(&flyout));

    // Fresh state every time the menu opens.
    {
        let (stack, search, home, ctx) = (stack.clone(), search.clone(), home.clone(), ctx.clone());
        popup.window.connect_show(move |_| {
            search.entry.set_text("");
            stack.set_visible_child_name("home");
            search.entry.grab_focus();
            home.refresh(&ctx);
        });
    }

    // Free everything transient when the menu closes.
    {
        let (search, home, all, ctx) = (search.clone(), home.clone(), all.clone(), ctx.clone());
        popup.window.connect_hide(move |_| {
            all.clear();
            search.clear();
            home.clear_recommended();
            ctx.cache.clear();
        });
    }

    popup
}
