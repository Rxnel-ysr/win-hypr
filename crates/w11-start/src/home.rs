//! Home page: Pinned grid + Recommended files.

use crate::ctx::Ctx;
use crate::widgets;
use gtk::prelude::*;
use w11_common::apps;
use w11_common::config::Config;

const MAX_PINNED: usize = 18;
const MAX_RECOMMENDED: usize = 6;

pub struct Home {
    pub root: gtk::Box,
    pub all_btn: gtk::Button,
    rec: gtk::FlowBox,
}

impl Home {
    pub fn new(ctx: &Ctx) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 14);

        let (head, all_btn) = widgets::header_row("Pinned", "All apps  ›");
        root.append(&head);

        let pins = gtk::FlowBox::new();
        pins.set_selection_mode(gtk::SelectionMode::None);
        pins.set_min_children_per_line(6);
        pins.set_max_children_per_line(6);
        pins.set_homogeneous(true);
        pins.set_row_spacing(0);
        pins.set_column_spacing(0);

        let cfg = Config::load();
        for info in cfg
            .pinned
            .iter()
            .filter_map(|id| apps::by_id(id))
            .take(MAX_PINNED)
        {
            pins.insert(&widgets::pin_tile(&info, &ctx.hide), -1);
        }
        drop(cfg);
        root.append(&pins);

        let rec_head = widgets::section_title("Recommended");
        rec_head.set_margin_top(8);
        root.append(&rec_head);

        let rec = gtk::FlowBox::new();
        rec.set_selection_mode(gtk::SelectionMode::None);
        rec.set_min_children_per_line(2);
        rec.set_max_children_per_line(2);
        rec.set_homogeneous(true);
        root.append(&rec);

        Self { root, all_btn, rec }
    }

    /// Rebuild the Recommended list (called every time the menu opens).
    pub fn refresh(&self, ctx: &Ctx) {
        self.clear_recommended();

        let mut items: Vec<(i64, String)> = gtk::RecentManager::default()
            .items()
            .into_iter()
            .map(|i| (i.modified().to_unix(), i.uri().to_string()))
            .filter(|(_, u)| u.starts_with("file://"))
            .collect();
        items.sort_by(|a, b| b.0.cmp(&a.0));

        let mut shown = 0;
        for (modified, uri) in items.into_iter().take(12) {
            if shown >= MAX_RECOMMENDED {
                break;
            }
            if let Some(row) = widgets::recent_row(uri, modified, &ctx.hide) {
                self.rec.insert(&row, -1);
                shown += 1;
            }
        }
    }

    pub fn clear_recommended(&self) {
        widgets::clear_flow(&self.rec);
    }
}
