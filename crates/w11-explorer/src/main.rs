//! File Explorer, Windows 11 style: tab pill + caption buttons, navigation row with
//! address/search boxes, command bar, navigation pane, details list, status bar.

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use w11_common::{hypr, sys, theme};

#[derive(Clone)]
struct Entry {
    file: gio::File,
    name: String,
    is_dir: bool,
    size: i64,
    modified: i64,
    kind: String,
    icon: Option<gio::Icon>,
    hidden: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortKey {
    Name,
    Modified,
    Kind,
    Size,
}

struct Nav {
    cwd: gio::File,
    back: Vec<gio::File>,
    fwd: Vec<gio::File>,
    entries: Vec<Entry>,
    visible: Vec<Entry>,
    sort: SortKey,
    ascending: bool,
    show_hidden: bool,
    clipboard: Vec<gio::File>,
    cut: bool,
    filter: String,
    monitor: Option<gio::FileMonitor>,
    reload_pending: bool,
}

struct Explorer {
    win: gtk::ApplicationWindow,
    list: gtk::ListBox,
    address: gtk::Entry,
    search: gtk::SearchEntry,
    status: gtk::Label,
    tab: gtk::Label,
    btn_back: gtk::Button,
    btn_fwd: gtk::Button,
    btn_up: gtk::Button,
    header_labels: Vec<(SortKey, gtk::Label, &'static str)>,
    nav: RefCell<Nav>,
}

fn main() -> glib::ExitCode {
    w11_common::mem::tune(w11_common::mem::Profile::Transient);
    let app = gtk::Application::builder()
        .application_id("dev.win11hypr.Explorer")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let start = std::env::args().nth(1);
    app.connect_startup(|_| theme::apply());
    app.connect_activate(move |app| {
        let f = match &start {
            Some(s) if s.contains("://") => gio::File::for_uri(s),
            Some(s) => gio::File::for_path(s),
            None => gio::File::for_path(glib::home_dir()),
        };
        let ex = Explorer::new(app, f);
        ex.win.present();
    });
    let prog = std::env::args().next().unwrap_or_default();
    app.run_with_args(&[prog.as_str()])
}

fn flat_button(icon: &str, label: Option<&str>, class: &str) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class(class);
    let h = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let img = gtk::Image::from_icon_name(icon);
    h.append(&img);
    if let Some(l) = label {
        h.append(&gtk::Label::new(Some(l)));
    }
    b.set_child(Some(&h));
    b
}

fn fmt_size(n: i64) -> String {
    let kb = (n + 1023) / 1024;
    let s = kb.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{} KB", out.chars().rev().collect::<String>())
}

fn quote(p: &str) -> String {
    format!("'{}'", p.replace('\'', "'\\''"))
}

fn unique_child(dir: &gio::File, name: &str, suffix: &str) -> gio::File {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let first = dir.child(format!("{stem}{suffix}{ext}"));
    if !first.query_exists(gio::Cancellable::NONE) {
        return first;
    }
    for n in 2.. {
        let f = dir.child(format!("{stem}{suffix} ({n}){ext}"));
        if !f.query_exists(gio::Cancellable::NONE) {
            return f;
        }
    }
    unreachable!()
}

fn copy_recursive(src: &gio::File, dst: &gio::File) -> Result<(), glib::Error> {
    let t = src.query_file_type(
        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
        gio::Cancellable::NONE,
    );
    if t == gio::FileType::Directory {
        dst.make_directory_with_parents(gio::Cancellable::NONE)?;
        let en = src.enumerate_children(
            "standard::name",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::Cancellable::NONE,
        )?;
        for info in en.flatten() {
            let name = info.name();
            copy_recursive(&src.child(&name), &dst.child(&name))?;
        }
        Ok(())
    } else {
        src.copy(
            dst,
            gio::FileCopyFlags::NOFOLLOW_SYMLINKS,
            gio::Cancellable::NONE,
            None,
        )
    }
}

impl Explorer {
    fn new(app: &gtk::Application, start: gio::File) -> Rc<Self> {
        let win = gtk::ApplicationWindow::builder()
            .application(app)
            .default_width(1100)
            .default_height(680)
            .title("File Explorer")
            .build();
        win.add_css_class("explorer");

        // ------------------------------------------------ title bar
        let header = gtk::HeaderBar::new();
        header.set_show_title_buttons(false);
        let tab = gtk::Label::new(Some("Home"));
        let tab_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        tab_box.add_css_class("tab-pill");
        tab_box.append(&gtk::Image::from_icon_name("folder-symbolic"));
        tab_box.append(&tab);
        header.set_title_widget(Some(&tab_box));
        let caption = |icon: &str, extra: &str| {
            let b = gtk::Button::from_icon_name(icon);
            b.add_css_class("caption-button");
            if !extra.is_empty() {
                b.add_css_class(extra);
            }
            b
        };
        let (b_min, b_max, b_close) = (
            caption("window-minimize-symbolic", ""),
            caption("window-maximize-symbolic", ""),
            caption("window-close-symbolic", "close"),
        );
        header.pack_end(&b_close);
        header.pack_end(&b_max);
        header.pack_end(&b_min);
        win.set_titlebar(Some(&header));
        b_min.connect_clicked(|_| {
            std::thread::spawn(hypr::minimize_active);
        });
        b_max.connect_clicked(|_| {
            std::thread::spawn(hypr::toggle_maximize_active);
        });
        let w = win.clone();
        b_close.connect_clicked(move |_| w.close());

        // ---------------------------------------------- navigation row
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        toolbar.add_css_class("explorer-toolbar");
        let btn_back = flat_button("go-previous-symbolic", None, "nav-button");
        let btn_fwd = flat_button("go-next-symbolic", None, "nav-button");
        let btn_up = flat_button("go-up-symbolic", None, "nav-button");
        let btn_refresh = flat_button("view-refresh-symbolic", None, "nav-button");
        for b in [&btn_back, &btn_fwd, &btn_up, &btn_refresh] {
            toolbar.append(b);
        }
        let address = gtk::Entry::new();
        address.add_css_class("address-bar");
        address.set_hexpand(true);
        address.set_margin_start(6);
        address.set_margin_end(6);
        let search = gtk::SearchEntry::new();
        search.add_css_class("search-box");
        search.set_placeholder_text(Some("Search this folder"));
        search.set_width_request(260);
        toolbar.append(&address);
        toolbar.append(&search);

        // ------------------------------------------------ command bar
        let cmd = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        cmd.add_css_class("command-bar");
        let c_new = flat_button("folder-new-symbolic", Some("New folder"), "command-button");
        let c_cut = flat_button("edit-cut-symbolic", None, "command-button");
        let c_copy = flat_button("edit-copy-symbolic", None, "command-button");
        let c_paste = flat_button("edit-paste-symbolic", None, "command-button");
        let c_rename = flat_button("document-edit-symbolic", None, "command-button");
        let c_delete = flat_button("user-trash-symbolic", None, "command-button");
        let c_term = flat_button(
            "utilities-terminal-symbolic",
            Some("Terminal"),
            "command-button",
        );
        for (b, tip) in [
            (&c_cut, "Cut (Ctrl+X)"),
            (&c_copy, "Copy (Ctrl+C)"),
            (&c_paste, "Paste (Ctrl+V)"),
            (&c_rename, "Rename (F2)"),
            (&c_delete, "Delete (Del)"),
        ] {
            b.set_tooltip_text(Some(tip));
        }
        let sep = || {
            let s = gtk::Box::new(gtk::Orientation::Vertical, 0);
            s.add_css_class("command-sep");
            s
        };
        cmd.append(&c_new);
        cmd.append(&sep());
        for b in [&c_cut, &c_copy, &c_paste, &c_rename, &c_delete] {
            cmd.append(b);
        }
        cmd.append(&sep());

        // Sort / View popovers
        let sort_menu = gtk::MenuButton::new();
        sort_menu.add_css_class("command-button");
        sort_menu.set_label("Sort");
        let sort_pop = gtk::Popover::new();
        sort_pop.set_has_arrow(false);
        let sort_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sort_pop.set_child(Some(&sort_box));
        sort_menu.set_popover(Some(&sort_pop));
        let view_menu = gtk::MenuButton::new();
        view_menu.add_css_class("command-button");
        view_menu.set_label("View");
        let view_pop = gtk::Popover::new();
        view_pop.set_has_arrow(false);
        let hidden_check = gtk::CheckButton::with_label("Hidden items");
        hidden_check.set_margin_start(8);
        hidden_check.set_margin_end(8);
        hidden_check.set_margin_top(6);
        hidden_check.set_margin_bottom(6);
        view_pop.set_child(Some(&hidden_check));
        view_menu.set_popover(Some(&view_pop));
        cmd.append(&sort_menu);
        cmd.append(&view_menu);
        cmd.append(&sep());
        cmd.append(&c_term);

        // ---------------------------------------------- navigation pane
        let sidebar = gtk::ListBox::new();
        sidebar.add_css_class("nav-sidebar");
        sidebar.set_selection_mode(gtk::SelectionMode::Single);
        let side_scroll = gtk::ScrolledWindow::new();
        side_scroll.set_child(Some(&sidebar));
        side_scroll.set_size_request(230, -1);
        side_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        let side_targets: Rc<RefCell<Vec<Option<gio::File>>>> = Rc::new(RefCell::new(Vec::new()));
        build_sidebar(&sidebar, &side_targets);

        // ------------------------------------------------- details list
        let pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
        pane.add_css_class("file-pane");
        pane.set_hexpand(true);
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        head.add_css_class("file-header");
        let mut header_labels = Vec::new();
        let cols: [(SortKey, &'static str, i32); 4] = [
            (SortKey::Name, "Name", -1),
            (SortKey::Modified, "Date modified", 170),
            (SortKey::Kind, "Type", 150),
            (SortKey::Size, "Size", 90),
        ];
        let head_buttons: Vec<(SortKey, gtk::Button)> = cols
            .iter()
            .map(|(k, t, w)| {
                let b = gtk::Button::new();
                let l = gtk::Label::new(Some(t));
                l.set_xalign(if *k == SortKey::Size { 1.0 } else { 0.0 });
                b.set_child(Some(&l));
                if *w > 0 {
                    b.set_width_request(*w);
                } else {
                    b.set_hexpand(true);
                    b.set_margin_start(34);
                }
                header_labels.push((*k, l, *t));
                head.append(&b);
                (*k, b)
            })
            .collect();
        pane.append(&head);

        let list = gtk::ListBox::new();
        list.add_css_class("file-list");
        list.set_selection_mode(gtk::SelectionMode::Multiple);
        list.set_activate_on_single_click(false);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_child(Some(&list));
        pane.append(&scroll);

        let status = gtk::Label::new(None);
        status.add_css_class("statusbar");
        status.set_xalign(0.0);

        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_start_child(Some(&side_scroll));
        paned.set_end_child(Some(&pane));
        paned.set_position(230);
        paned.set_vexpand(true);
        paned.set_shrink_start_child(false);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&toolbar);
        root.append(&cmd);
        root.append(&paned);
        root.append(&status);
        win.set_child(Some(&root));

        let ex = Rc::new(Explorer {
            win: win.clone(),
            list: list.clone(),
            address: address.clone(),
            search: search.clone(),
            status,
            tab,
            btn_back: btn_back.clone(),
            btn_fwd: btn_fwd.clone(),
            btn_up: btn_up.clone(),
            header_labels,
            nav: RefCell::new(Nav {
                cwd: start.clone(),
                back: vec![],
                fwd: vec![],
                entries: vec![],
                visible: vec![],
                sort: SortKey::Name,
                ascending: true,
                show_hidden: false,
                clipboard: vec![],
                cut: false,
                filter: String::new(),
                monitor: None,
                reload_pending: false,
            }),
        });

        // ------------------------------------------------- wiring
        {
            let e = ex.clone();
            btn_back.connect_clicked(move |_| e.go_back());
            let e = ex.clone();
            btn_fwd.connect_clicked(move |_| e.go_forward());
            let e = ex.clone();
            btn_up.connect_clicked(move |_| e.go_up());
            let e = ex.clone();
            btn_refresh.connect_clicked(move |_| e.reload());
            let e = ex.clone();
            address.connect_activate(move |a| {
                let t = a.text().to_string();
                let f = if t.contains("://") {
                    gio::File::for_uri(&t)
                } else if let Some(rest) = t.strip_prefix('~') {
                    gio::File::for_path(glib::home_dir().join(rest.trim_start_matches('/')))
                } else {
                    gio::File::for_path(&t)
                };
                e.navigate(f, true);
            });
            let e = ex.clone();
            search.connect_search_changed(move |s| {
                e.nav.borrow_mut().filter = s.text().to_lowercase();
                e.apply_view();
            });

            let e = ex.clone();
            c_new.connect_clicked(move |_| e.new_folder());
            let e = ex.clone();
            c_cut.connect_clicked(move |_| e.set_clipboard(true));
            let e = ex.clone();
            c_copy.connect_clicked(move |_| e.set_clipboard(false));
            let e = ex.clone();
            c_paste.connect_clicked(move |_| e.paste());
            let e = ex.clone();
            c_rename.connect_clicked(move |_| e.rename_dialog());
            let e = ex.clone();
            c_delete.connect_clicked(move |_| e.delete_selected());
            let e = ex.clone();
            c_term.connect_clicked(move |_| {
                let path = e
                    .nav
                    .borrow()
                    .cwd
                    .path()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| glib::home_dir().to_string_lossy().to_string());
                sys::spawn(&format!("cd {} && exec {}", quote(&path), sys::TERMINAL));
            });

            let e = ex.clone();
            hidden_check.connect_toggled(move |c| {
                e.nav.borrow_mut().show_hidden = c.is_active();
                e.apply_view();
            });

            // sort menu entries
            for (label, key) in [
                ("Name", SortKey::Name),
                ("Date modified", SortKey::Modified),
                ("Type", SortKey::Kind),
                ("Size", SortKey::Size),
            ] {
                let b = gtk::Button::with_label(label);
                b.add_css_class("menu-item");
                let (e, p) = (ex.clone(), sort_pop.clone());
                b.connect_clicked(move |_| {
                    e.nav.borrow_mut().sort = key;
                    p.popdown();
                    e.apply_view();
                });
                sort_box.append(&b);
            }
            for (label, asc) in [("Ascending", true), ("Descending", false)] {
                let b = gtk::Button::with_label(label);
                b.add_css_class("menu-item");
                let (e, p) = (ex.clone(), sort_pop.clone());
                b.connect_clicked(move |_| {
                    e.nav.borrow_mut().ascending = asc;
                    p.popdown();
                    e.apply_view();
                });
                sort_box.append(&b);
            }

            // column headers
            for (key, b) in head_buttons {
                let e = ex.clone();
                b.connect_clicked(move |_| {
                    {
                        let mut n = e.nav.borrow_mut();
                        if n.sort == key {
                            n.ascending = !n.ascending;
                        } else {
                            n.sort = key;
                            n.ascending = true;
                        }
                    }
                    e.apply_view();
                });
            }

            // sidebar
            let (e, t) = (ex.clone(), side_targets.clone());
            sidebar.connect_row_activated(move |_, row| {
                if let Some(Some(f)) = t.borrow().get(row.index() as usize) {
                    e.navigate(f.clone(), true);
                }
            });

            // open on double-click / Enter
            let e = ex.clone();
            list.connect_row_activated(move |_, row| {
                let entry = e.nav.borrow().visible.get(row.index() as usize).cloned();
                if let Some(en) = entry {
                    e.open(&en);
                }
            });
            let e = ex.clone();
            list.connect_selected_rows_changed(move |_| e.update_status());

            // right-click on a row -> context menu
            let ctx = gtk::GestureClick::builder().button(3).build();
            let e = ex.clone();
            let l = list.clone();
            ctx.connect_pressed(move |g, _, x, y| {
                if let Some(row) = l.row_at_y(y as i32) {
                    if !row.is_selected() {
                        l.unselect_all();
                        l.select_row(Some(&row));
                    }
                }
                e.context_menu(&l, x, y);
                g.set_state(gtk::EventSequenceState::Claimed);
            });
            list.add_controller(ctx);

            // keyboard shortcuts
            let key = gtk::EventControllerKey::new();
            key.set_propagation_phase(gtk::PropagationPhase::Capture);
            let e = ex.clone();
            key.connect_key_pressed(move |_, k, _, state| e.on_key(k, state));
            win.add_controller(key);
        }

        ex.navigate(start, true);
        ex
    }

    // ------------------------------------------------------- navigation

    fn navigate(self: &Rc<Self>, f: gio::File, push: bool) {
        {
            let mut n = self.nav.borrow_mut();
            if push && n.cwd.uri() != f.uri() {
                let old = n.cwd.clone();
                n.back.push(old);
                n.fwd.clear();
            }
            n.cwd = f;
            n.filter.clear();
        }
        self.search.set_text("");
        self.reload();
        self.watch();
    }

    fn go_back(self: &Rc<Self>) {
        let prev = {
            let mut n = self.nav.borrow_mut();
            let p = n.back.pop();
            if let Some(_) = &p {
                let cur = n.cwd.clone();
                n.fwd.push(cur);
            }
            p
        };
        if let Some(p) = prev {
            self.navigate(p, false);
        }
    }

    fn go_forward(self: &Rc<Self>) {
        let next = {
            let mut n = self.nav.borrow_mut();
            let p = n.fwd.pop();
            if let Some(_) = &p {
                let cur = n.cwd.clone();
                n.back.push(cur);
            }
            p
        };
        if let Some(p) = next {
            self.navigate(p, false);
        }
    }

    fn go_up(self: &Rc<Self>) {
        let parent = self.nav.borrow().cwd.parent();
        if let Some(p) = parent {
            self.navigate(p, true);
        }
    }

    fn watch(self: &Rc<Self>) {
        let cwd = self.nav.borrow().cwd.clone();
        let mon = cwd.monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE);
        if let Ok(m) = &mon {
            let e = self.clone();
            m.connect_changed(move |_, _, _, _| {
                if e.nav.borrow().reload_pending {
                    return;
                }
                e.nav.borrow_mut().reload_pending = true;
                let e2 = e.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
                    e2.nav.borrow_mut().reload_pending = false;
                    e2.reload();
                });
            });
        }
        self.nav.borrow_mut().monitor = mon.ok();
    }

    fn open(self: &Rc<Self>, en: &Entry) {
        if en.is_dir {
            self.navigate(en.file.clone(), true);
        } else {
            let _ =
                gio::AppInfo::launch_default_for_uri(&en.file.uri(), gio::AppLaunchContext::NONE);
        }
    }

    // --------------------------------------------------------- listing

    fn reload(self: &Rc<Self>) {
        let cwd = self.nav.borrow().cwd.clone();
        let mut entries = Vec::new();
        let mut kinds: std::collections::HashMap<String, String> = Default::default();
        let attrs = "standard::name,standard::display-name,standard::type,standard::size,\
                     standard::icon,standard::content-type,standard::is-hidden,time::modified";
        match cwd.enumerate_children(attrs, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) {
            Ok(en) => {
                for info in en.flatten() {
                    let kind = if info.file_type() == gio::FileType::Directory {
                        "File folder".to_string()
                    } else {
                        info.content_type()
                            .map(|c| {
                                kinds
                                    .entry(c.to_string())
                                    .or_insert_with(|| {
                                        gio::content_type_get_description(&c).to_string()
                                    })
                                    .clone()
                            })
                            .unwrap_or_else(|| "File".into())
                    };
                    entries.push(Entry {
                        file: cwd.child(info.name()),
                        name: info.display_name().to_string(),
                        is_dir: info.file_type() == gio::FileType::Directory,
                        size: info.size(),
                        modified: info
                            .modification_date_time()
                            .map(|d| d.to_unix())
                            .unwrap_or(0),
                        kind,
                        icon: info.icon(),
                        hidden: info.is_hidden() || info.name().to_string_lossy().starts_with('.'),
                    });
                }
            }
            Err(e) => self
                .status
                .set_text(&format!("Can't open this folder: {}", e.message())),
        }
        self.nav.borrow_mut().entries = entries;

        // chrome
        let n = self.nav.borrow();
        let text = cwd
            .path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| cwd.uri().to_string());
        self.address.set_text(&text);
        let title = cwd
            .basename()
            .map(|b| b.to_string_lossy().to_string())
            .filter(|b| !b.is_empty() && b != "/")
            .unwrap_or_else(|| "This PC".into());
        self.tab.set_text(&title);
        self.win
            .set_title(Some(&format!("{title} - File Explorer")));
        self.btn_back.set_sensitive(!n.back.is_empty());
        self.btn_fwd.set_sensitive(!n.fwd.is_empty());
        self.btn_up.set_sensitive(cwd.parent().is_some());
        drop(n);
        self.apply_view();
    }

    fn apply_view(self: &Rc<Self>) {
        let (visible, sort, asc) = {
            let n = self.nav.borrow();
            let mut v: Vec<Entry> = n
                .entries
                .iter()
                .filter(|e| n.show_hidden || !e.hidden)
                .filter(|e| n.filter.is_empty() || e.name.to_lowercase().contains(&n.filter))
                .cloned()
                .collect();
            let key = n.sort;
            v.sort_by(|a, b| {
                // folders always first
                if a.is_dir != b.is_dir {
                    return b.is_dir.cmp(&a.is_dir);
                }
                let ord = match key {
                    SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortKey::Modified => a.modified.cmp(&b.modified),
                    SortKey::Kind => a
                        .kind
                        .cmp(&b.kind)
                        .then(a.name.to_lowercase().cmp(&b.name.to_lowercase())),
                    SortKey::Size => a.size.cmp(&b.size),
                };
                if n.ascending {
                    ord
                } else {
                    ord.reverse()
                }
            });
            (v, n.sort, n.ascending)
        };

        for (k, label, text) in &self.header_labels {
            if *k == sort {
                label.set_text(&format!("{text}  {}", if asc { "˄" } else { "˅" }));
            } else {
                label.set_text(text);
            }
        }

        while let Some(c) = self.list.first_child() {
            self.list.remove(&c);
        }
        for e in &visible {
            self.list.append(&make_row(e));
        }
        self.nav.borrow_mut().visible = visible;
        self.update_status();
    }

    fn update_status(&self) {
        let n = self.nav.borrow();
        let total = n.visible.len();
        let sel = self.list.selected_rows().len();
        let mut s = format!("{total} item{}", if total == 1 { "" } else { "s" });
        if sel > 0 {
            s.push_str(&format!(
                "    |    {sel} item{} selected",
                if sel == 1 { "" } else { "s" }
            ));
        }
        self.status.set_text(&s);
    }

    fn selected(&self) -> Vec<Entry> {
        let n = self.nav.borrow();
        self.list
            .selected_rows()
            .iter()
            .filter_map(|r| n.visible.get(r.index() as usize).cloned())
            .collect()
    }

    fn error(&self, msg: &str) {
        self.status.set_text(msg);
    }

    // ------------------------------------------------------ operations

    fn set_clipboard(self: &Rc<Self>, cut: bool) {
        let sel = self.selected();
        if sel.is_empty() {
            return;
        }
        let mut n = self.nav.borrow_mut();
        n.clipboard = sel.into_iter().map(|e| e.file).collect();
        n.cut = cut;
        drop(n);
        self.status
            .set_text(if cut { "Ready to move" } else { "Copied" });
    }

    fn paste(self: &Rc<Self>) {
        let (clip, cut, cwd) = {
            let n = self.nav.borrow();
            (n.clipboard.clone(), n.cut, n.cwd.clone())
        };
        for src in clip {
            let name = src
                .basename()
                .map(|b| b.to_string_lossy().to_string())
                .unwrap_or_default();
            let same_dir = src.parent().map(|p| p.equal(&cwd)).unwrap_or(false);
            let dst = if same_dir && !cut {
                unique_child(&cwd, &name, " - Copy")
            } else {
                unique_child(&cwd, &name, "")
            };
            let res = if cut {
                src.move_(&dst, gio::FileCopyFlags::NONE, gio::Cancellable::NONE, None)
            } else {
                copy_recursive(&src, &dst)
            };
            if let Err(e) = res {
                self.error(&format!("Couldn't paste {name}: {}", e.message()));
            }
        }
        if cut {
            self.nav.borrow_mut().clipboard.clear();
        }
        self.reload();
    }

    fn new_folder(self: &Rc<Self>) {
        let cwd = self.nav.borrow().cwd.clone();
        let f = unique_child(&cwd, "New folder", "");
        match f.make_directory(gio::Cancellable::NONE) {
            Ok(()) => {
                self.reload();
                // select the new folder
                let idx = self
                    .nav
                    .borrow()
                    .visible
                    .iter()
                    .position(|e| e.file.equal(&f));
                if let Some(i) = idx {
                    if let Some(row) = self.list.row_at_index(i as i32) {
                        self.list.select_row(Some(&row));
                        row.grab_focus();
                    }
                }
                self.rename_dialog();
            }
            Err(e) => self.error(&format!("Couldn't create folder: {}", e.message())),
        }
    }

    fn delete_selected(self: &Rc<Self>) {
        let sel = self.selected();
        if sel.is_empty() {
            return;
        }
        let in_trash = self
            .nav
            .borrow()
            .cwd
            .uri_scheme()
            .map(|s| s == "trash")
            .unwrap_or(false);
        if in_trash {
            let dialog = gtk::AlertDialog::builder()
                .message("Permanently delete these items?")
                .detail("This can't be undone.")
                .buttons(["Cancel", "Delete"])
                .cancel_button(0)
                .default_button(0)
                .build();
            let e = self.clone();
            dialog.choose(Some(&self.win), gio::Cancellable::NONE, move |r| {
                if r == Ok(1) {
                    for s in &sel {
                        let _ = s.file.delete(gio::Cancellable::NONE);
                    }
                    e.reload();
                }
            });
        } else {
            for s in &sel {
                if let Err(err) = s.file.trash(gio::Cancellable::NONE) {
                    self.error(&format!(
                        "Couldn't move {} to the Recycle Bin: {}",
                        s.name,
                        err.message()
                    ));
                }
            }
            self.reload();
        }
    }

    fn rename_dialog(self: &Rc<Self>) {
        let sel = self.selected();
        let Some(target) = sel.first().cloned() else {
            return;
        };
        let dlg = gtk::Window::builder()
            .transient_for(&self.win)
            .modal(true)
            .title("Rename")
            .default_width(380)
            .resizable(false)
            .build();
        dlg.add_css_class("explorer");
        let v = gtk::Box::new(gtk::Orientation::Vertical, 12);
        v.set_margin_top(18);
        v.set_margin_bottom(18);
        v.set_margin_start(18);
        v.set_margin_end(18);
        let entry = gtk::Entry::new();
        entry.add_css_class("address-bar");
        entry.set_text(&target.name);
        // select the stem only, like Explorer does
        let stem_len = match target.name.rfind('.') {
            Some(i) if i > 0 && !target.is_dir => target.name[..i].chars().count() as i32,
            _ => -1,
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_halign(gtk::Align::End);
        let cancel = gtk::Button::with_label("Cancel");
        let ok = gtk::Button::with_label("Rename");
        ok.add_css_class("link-button");
        cancel.add_css_class("link-button");
        row.append(&cancel);
        row.append(&ok);
        v.append(&entry);
        v.append(&row);
        dlg.set_child(Some(&v));

        let do_rename = {
            let (e, dlg, entry, target) =
                (self.clone(), dlg.clone(), entry.clone(), target.clone());
            Rc::new(move || {
                let new = entry.text().to_string();
                if !new.is_empty() && new != target.name {
                    if let Err(err) = target.file.set_display_name(&new, gio::Cancellable::NONE) {
                        e.error(&format!("Couldn't rename: {}", err.message()));
                    }
                }
                dlg.close();
                e.reload();
            })
        };
        let r = do_rename.clone();
        ok.connect_clicked(move |_| r());
        let r = do_rename.clone();
        entry.connect_activate(move |_| r());
        let d = dlg.clone();
        cancel.connect_clicked(move |_| d.close());
        dlg.present();
        entry.grab_focus();
        if stem_len > 0 {
            entry.select_region(0, stem_len);
        }
    }

    fn context_menu(self: &Rc<Self>, list: &gtk::ListBox, x: f64, y: f64) {
        let pop = gtk::Popover::new();
        pop.set_parent(list);
        pop.set_has_arrow(false);
        pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let menu = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let have_sel = !self.selected().is_empty();
        type Action = Box<dyn Fn(&Rc<Explorer>)>;
        let items: Vec<(&str, bool, Action)> = vec![
            (
                "Open",
                have_sel,
                Box::new(|e: &Rc<Explorer>| {
                    if let Some(s) = e.selected().first().cloned() {
                        e.open(&s);
                    }
                }),
            ),
            (
                "Cut",
                have_sel,
                Box::new(|e: &Rc<Explorer>| e.set_clipboard(true)),
            ),
            (
                "Copy",
                have_sel,
                Box::new(|e: &Rc<Explorer>| e.set_clipboard(false)),
            ),
            ("Paste", true, Box::new(|e: &Rc<Explorer>| e.paste())),
            (
                "Rename",
                have_sel,
                Box::new(|e: &Rc<Explorer>| e.rename_dialog()),
            ),
            (
                "Delete",
                have_sel,
                Box::new(|e: &Rc<Explorer>| e.delete_selected()),
            ),
            (
                "New folder",
                true,
                Box::new(|e: &Rc<Explorer>| e.new_folder()),
            ),
        ];
        for (label, enabled, act) in items {
            let b = gtk::Button::with_label(label);
            b.add_css_class("menu-item");
            b.set_sensitive(enabled);
            if let Some(l) = b.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
                l.set_xalign(0.0);
            }
            let (e, p) = (self.clone(), pop.clone());
            b.connect_clicked(move |_| {
                p.popdown();
                act(&e);
            });
            menu.append(&b);
        }
        pop.set_child(Some(&menu));
        pop.connect_closed(|p| {
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
        pop.popup();
    }

    fn on_key(
        self: &Rc<Self>,
        key: gtk::gdk::Key,
        state: gtk::gdk::ModifierType,
    ) -> glib::Propagation {
        use gtk::gdk::{Key, ModifierType as M};
        let typing = gtk::prelude::GtkWindowExt::focus(&self.win)
            .map(|w| w.is::<gtk::Text>() || w.is::<gtk::Editable>())
            .unwrap_or(false);
        let ctrl = state.contains(M::CONTROL_MASK);
        let alt = state.contains(M::ALT_MASK);
        let shift = state.contains(M::SHIFT_MASK);
        let stop = glib::Propagation::Stop;

        if alt {
            match key {
                Key::Left => {
                    self.go_back();
                    return stop;
                }
                Key::Right => {
                    self.go_forward();
                    return stop;
                }
                Key::Up => {
                    self.go_up();
                    return stop;
                }
                _ => {}
            }
        }
        if ctrl {
            match key {
                Key::l | Key::L => {
                    self.address.grab_focus();
                    return stop;
                }
                Key::f | Key::F => {
                    self.search.grab_focus();
                    return stop;
                }
                Key::n | Key::N if shift => {
                    self.new_folder();
                    return stop;
                }
                Key::w | Key::W => {
                    self.win.close();
                    return stop;
                }
                Key::c | Key::C if !typing => {
                    self.set_clipboard(false);
                    return stop;
                }
                Key::x | Key::X if !typing => {
                    self.set_clipboard(true);
                    return stop;
                }
                Key::v | Key::V if !typing => {
                    self.paste();
                    return stop;
                }
                _ => {}
            }
        }
        if !typing {
            match key {
                Key::F2 => {
                    self.rename_dialog();
                    return stop;
                }
                Key::F5 => {
                    self.reload();
                    return stop;
                }
                Key::Delete => {
                    self.delete_selected();
                    return stop;
                }
                Key::BackSpace => {
                    self.go_up();
                    return stop;
                }
                _ => {}
            }
        }
        glib::Propagation::Proceed
    }
}

fn make_row(e: &Entry) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let h = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let icon = match &e.icon {
        Some(i) => gtk::Image::from_gicon(i),
        None => gtk::Image::from_icon_name(if e.is_dir { "folder" } else { "text-x-generic" }),
    };
    icon.set_pixel_size(24);
    let name = gtk::Label::new(Some(&e.name));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let date = gtk::Label::new(Some(
        &glib::DateTime::from_unix_local(e.modified)
            .ok()
            .and_then(|d| d.format("%-m/%-d/%Y %-I:%M %p").ok())
            .map(|s| s.to_string())
            .unwrap_or_default(),
    ));
    date.add_css_class("file-meta");
    date.set_xalign(0.0);
    date.set_width_request(170);
    let kind = gtk::Label::new(Some(&e.kind));
    kind.add_css_class("file-meta");
    kind.set_xalign(0.0);
    kind.set_width_request(150);
    kind.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let size = gtk::Label::new(Some(&if e.is_dir {
        String::new()
    } else {
        fmt_size(e.size)
    }));
    size.add_css_class("file-meta");
    size.set_xalign(1.0);
    size.set_width_request(90);

    h.append(&icon);
    h.append(&name);
    h.append(&date);
    h.append(&kind);
    h.append(&size);
    row.set_child(Some(&h));
    row
}

fn build_sidebar(sidebar: &gtk::ListBox, targets: &Rc<RefCell<Vec<Option<gio::File>>>>) {
    let mut t = targets.borrow_mut();
    let home = glib::home_dir();
    let heading = |text: &str| {
        let r = gtk::ListBoxRow::new();
        r.set_selectable(false);
        r.set_activatable(false);
        let l = gtk::Label::new(Some(text));
        l.add_css_class("nav-heading");
        l.set_xalign(0.0);
        r.set_child(Some(&l));
        r
    };
    let item = |icon: &str, text: &str| {
        let r = gtk::ListBoxRow::new();
        let h = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let img = gtk::Image::from_icon_name(icon);
        h.append(&img);
        h.append(&gtk::Label::new(Some(text)));
        r.set_child(Some(&h));
        r
    };

    sidebar.append(&heading("Quick access"));
    t.push(None);
    sidebar.append(&item("user-home-symbolic", "Home"));
    t.push(Some(gio::File::for_path(&home)));
    if gio::Vfs::default()
        .supported_uri_schemes()
        .iter()
        .any(|s| s.as_str() == "trash")
    {
        sidebar.append(&heading("Other"));
        t.push(None);
        sidebar.append(&item("user-trash-symbolic", "Recycle Bin"));
        t.push(Some(gio::File::for_uri("trash:///")));
    }
    let dirs = [
        (
            glib::UserDirectory::Desktop,
            "user-desktop-symbolic",
            "Desktop",
        ),
        (
            glib::UserDirectory::Documents,
            "folder-documents-symbolic",
            "Documents",
        ),
        (
            glib::UserDirectory::Downloads,
            "folder-download-symbolic",
            "Downloads",
        ),
        (
            glib::UserDirectory::Pictures,
            "folder-pictures-symbolic",
            "Pictures",
        ),
        (glib::UserDirectory::Music, "folder-music-symbolic", "Music"),
        (
            glib::UserDirectory::Videos,
            "folder-videos-symbolic",
            "Videos",
        ),
    ];
    for (d, icon, label) in dirs {
        if let Some(p) = glib::user_special_dir(d) {
            if p != home {
                sidebar.append(&item(icon, label));
                t.push(Some(gio::File::for_path(p)));
            }
        }
    }
    sidebar.append(&heading("This PC"));
    t.push(None);
    sidebar.append(&item("drive-harddisk-symbolic", "Local Disk (/)"));
    t.push(Some(gio::File::for_path("/")));
    for m in gio::VolumeMonitor::get().mounts() {
        let root = m.root();
        if root
            .path()
            .map(|p| p == std::path::Path::new("/"))
            .unwrap_or(false)
        {
            continue;
        }
        sidebar.append(&item("drive-removable-media-symbolic", &m.name()));
        t.push(Some(root));
    }
    sidebar.append(&heading("Other"));
    t.push(None);
    sidebar.append(&item("user-trash-symbolic", "Recycle Bin"));
    t.push(Some(gio::File::for_uri("trash:///")));
    w11_common::mem::keep_trimmed(60, 4096);
}
