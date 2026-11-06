//! Search entry + results page.

use crate::ctx::Ctx;
use crate::widgets;
use gtk::prelude::*;
use w11_common::{apps, sys};

const MAX_RESULTS: usize = 12;

#[derive(Clone)]
pub struct Search {
    pub entry: gtk::SearchEntry,
    pub page: gtk::ScrolledWindow,
    results: gtk::Box,
}

impl Search {
    pub fn new() -> Self {
        let entry = gtk::SearchEntry::new();
        entry.set_placeholder_text(Some("Type here to search"));
        entry.add_css_class("start-search");

        let page = gtk::ScrolledWindow::new();
        page.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        let results = gtk::Box::new(gtk::Orientation::Vertical, 0);
        page.set_child(Some(&results));

        Self {
            entry,
            page,
            results,
        }
    }

    pub fn wire(&self, stack: &gtk::Stack, ctx: &Ctx) {
        // Live results
        {
            let (stack, results, ctx) = (stack.clone(), self.results.clone(), ctx.clone());
            self.entry.connect_search_changed(move |e| {
                let q = e.text().trim().to_lowercase();
                widgets::clear_box(&results);
                if q.is_empty() {
                    stack.set_visible_child_name("home");
                    return;
                }
                let index = ctx.cache.get();
                let hits = index.search(&q, MAX_RESULTS);

                let head = widgets::section_title(if hits.is_empty() {
                    "No apps found"
                } else {
                    "Best match"
                });
                head.set_margin_bottom(6);
                results.append(&head);
                for a in hits {
                    results.append(&widgets::app_row(a, &ctx.hide));
                }
                stack.set_visible_child_name("search");
            });
        }

        // Enter: launch best match, otherwise run it like Win+R
        {
            let ctx = ctx.clone();
            self.entry.connect_activate(move |e| {
                let text = e.text();
                let q = text.trim().to_lowercase();
                if q.is_empty() {
                    return;
                }
                let index = ctx.cache.get();
                match index.search(&q, 1).first() {
                    Some(a) => apps::launch(a),
                    None => sys::spawn(&text),
                }
                (ctx.hide)();
            });
        }
    }

    pub fn clear(&self) {
        widgets::clear_box(&self.results);
    }
}
