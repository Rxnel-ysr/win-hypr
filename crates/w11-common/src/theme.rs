pub const CSS: &str = include_str!("css/fluent.css");
pub const TASKVIEW_CSS: &str = include_str!("css/taskview.css");

/// Install the Fluent (Windows 11 dark) stylesheet, plus `~/.config/win11-hypr/user.css` if present.
pub fn apply() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    if let Some(s) = gtk::Settings::default() {
        s.set_gtk_application_prefer_dark_theme(true);
    }
    for css in [CSS, TASKVIEW_CSS] {
        let p = gtk::CssProvider::new();
        p.load_from_data(css);
        gtk::style_context_add_provider_for_display(
            &display,
            &p,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    let user = crate::config::config_dir().join("user.css");
    if user.exists() {
        let p = gtk::CssProvider::new();
        p.load_from_path(&user);
        gtk::style_context_add_provider_for_display(
            &display,
            &p,
            gtk::STYLE_PROVIDER_PRIORITY_USER,
        );
    }
}
