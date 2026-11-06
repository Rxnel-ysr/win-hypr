//! Keyboard navigation for Task View: arrows move, Enter opens, Delete closes,
//! Tab switches between the window grid and the desktop strip.

use super::{Tv, Zone};
use gtk::gdk;
use gtk::glib;
use std::rc::Rc;

impl Tv {
    pub(super) fn on_key(self: &Rc<Self>, key: gdk::Key) -> glib::Propagation {
        use gdk::Key;
        let zone = self.st.borrow().zone;
        match (zone, key) {
            (_, Key::Tab) | (_, Key::ISO_Left_Tab) => {
                if zone == Zone::Windows {
                    self.enter_desktops();
                } else {
                    self.enter_windows();
                }
            }
            (Zone::Windows, Key::Left) => self.move_sel(-1),
            (Zone::Windows, Key::Right) => self.move_sel(1),
            (Zone::Windows, Key::Up) => self.move_vertical(-1),
            (Zone::Windows, Key::Down) => self.move_vertical(1),
            (Zone::Windows, Key::Home) => self.select_first_or_last(true),
            (Zone::Windows, Key::End) => self.select_first_or_last(false),
            (Zone::Windows, Key::Return)
            | (Zone::Windows, Key::KP_Enter)
            | (Zone::Windows, Key::space) => {
                let sel = self.st.borrow().sel;
                if let Some(i) = sel {
                    self.activate(i);
                }
            }
            (Zone::Windows, Key::Delete) => {
                let sel = self.st.borrow().sel;
                if let Some(i) = sel {
                    self.close_window(i);
                }
            }
            (Zone::Desktops, Key::Left) => self.move_dsel(-1),
            (Zone::Desktops, Key::Right) => self.move_dsel(1),
            (Zone::Desktops, Key::Up) => self.enter_windows(),
            (Zone::Desktops, Key::Return)
            | (Zone::Desktops, Key::KP_Enter)
            | (Zone::Desktops, Key::space) => {
                let (dsel, count) = {
                    let st = self.st.borrow();
                    (st.dsel, st.count)
                };
                match dsel {
                    Some(i) if (i as i64) < count => self.switch_desktop(i as i64 + 1),
                    Some(_) => self.new_desktop(),
                    None => {}
                }
            }
            (Zone::Desktops, Key::Delete) => {
                let (dsel, count) = {
                    let st = self.st.borrow();
                    (st.dsel, st.count)
                };
                if let Some(i) = dsel {
                    if (i as i64) < count && count > 1 {
                        self.close_desktop(i as i64 + 1);
                    }
                }
            }
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    fn move_sel(self: &Rc<Self>, delta: i32) {
        let (n, cur) = {
            let st = self.st.borrow();
            (st.cards.len(), st.sel)
        };
        if n == 0 {
            self.enter_desktops();
            return;
        }
        let next = match cur {
            None => 0,
            Some(i) => (i as i32 + delta).clamp(0, n as i32 - 1) as usize,
        };
        self.set_sel(Some(next));
    }

    fn select_first_or_last(self: &Rc<Self>, first: bool) {
        let n = self.st.borrow().cards.len();
        if n > 0 {
            self.set_sel(Some(if first { 0 } else { n - 1 }));
        }
    }

    fn move_vertical(self: &Rc<Self>, dir: i32) {
        let (target, none_selected) = {
            let st = self.st.borrow();
            match st.sel.and_then(|i| st.cards.get(i)) {
                None => (if st.cards.is_empty() { None } else { Some(0) }, true),
                Some(cur) => {
                    let want = cur.row as i32 + dir;
                    let best = st
                        .cards
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| c.row as i32 == want)
                        .min_by(|a, b| {
                            (a.1.cx - cur.cx)
                                .abs()
                                .partial_cmp(&(b.1.cx - cur.cx).abs())
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .map(|(i, _)| i);
                    (best, false)
                }
            }
        };
        match target {
            Some(i) => self.set_sel(Some(i)),
            None if dir > 0 || none_selected => self.enter_desktops(),
            None => {}
        }
    }

    fn enter_desktops(self: &Rc<Self>) {
        self.set_sel(None);
        let idx = {
            let mut st = self.st.borrow_mut();
            st.zone = Zone::Desktops;
            (st.current - 1).max(0) as usize
        };
        self.set_dsel(Some(idx));
    }

    fn enter_windows(self: &Rc<Self>) {
        self.set_dsel(None);
        self.st.borrow_mut().zone = Zone::Windows;
        let has = !self.st.borrow().cards.is_empty();
        if has {
            self.set_sel(Some(0));
        }
    }

    fn move_dsel(self: &Rc<Self>, delta: i32) {
        let (n, cur, count) = {
            let st = self.st.borrow();
            (st.desks.len(), st.dsel, st.count)
        };
        if n == 0 {
            return;
        }
        let next = (cur.unwrap_or(0) as i32 + delta).clamp(0, n as i32 - 1) as usize;
        self.set_dsel(Some(next));
        if (next as i64) < count {
            self.preview(next as i64 + 1);
        }
    }
}
