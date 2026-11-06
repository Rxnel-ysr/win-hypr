//! "All apps" page. The list is built on demand and freed when the menu hides.

use crate::ctx::Ctx;
use crate::widgets;
use gtk::prelude::*;
use std::cell::Cell;

pub struct AllApps {
    pub page: gtk::Box,
    pub back_btn: gtk::Button,
    list: gtk::Box,
    built: Cell<bool>,
}

impl AllApps {
    pub fn new(_ctx: &Ctx) -> Self {
        let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let (head, back_btn) = widgets::header_row("All apps", "‹  Back");
        page.append(&head);

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        scroll.set_child(Some(&list));
        page.append(&scroll);

        Self {
            page,
            back_btn,
            list,
            built: Cell::new(false),
        }
    }

    pub fn ensure_built(&self, ctx: &Ctx) {
        if self.built.replace(true) {
            return;
        }
        let index = ctx.cache.get();
        let mut last_letter = String::new();
        for info in index.apps() {
            let letter = info
                .name()
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default();
            if letter != last_letter {
                let l = widgets::section_title(&letter);
                l.set_margin_top(10);
                l.set_margin_start(10);
                self.list.append(&l);
                last_letter = letter;
            }
            self.list.append(&widgets::app_row(info, &ctx.hide));
        }
    }

    pub fn clear(&self) {
        widgets::clear_box(&self.list);
        self.built.set(false);
    }
}
