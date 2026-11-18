use eframe::egui::{self, Color32, Rect, Ui};

#[derive(Clone, Copy)]
pub struct Icon(pub &'static str, pub &'static str);

macro_rules! svg {
    ($name:ident, $uri:literal, $body:literal) => {
        pub const $name: Icon = Icon(
            concat!("bytes://", $uri, ".svg"),
            concat!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">"#, $body, "</svg>"),
        );
    };
}


svg!(LOGO, "logo", r##"<circle cx="12" cy="12" r="9" fill="none" stroke="#fff" stroke-width="2.2" stroke-opacity=".35"/><path d="M12 3a9 9 0 0 1 9 9h-9z" fill="#fff"/><circle cx="12" cy="12" r="3" fill="#fff"/>"##);
svg!(DRIVE, "drive", r##"<path fill="none" stroke="#fff" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M4 13h16M4 13l2.2-6.6A2 2 0 0 1 8.1 5h7.8a2 2 0 0 1 1.9 1.4L20 13v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-4zM16.5 16h.01"/>"##);
svg!(USB, "usb", r##"<path fill="none" stroke="#fff" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M8 3h8v6H8zM6 9h12v9a3 3 0 0 1-3 3H9a3 3 0 0 1-3-3zM10 5.5h.01M14 5.5h.01"/>"##);
svg!(FOLDER, "folder", r##"<path fill="#fff" d="M3 6.5A2.5 2.5 0 0 1 5.5 4h3.6c.6 0 1.2.2 1.6.6L12.4 6h6.1A2.5 2.5 0 0 1 21 8.5v9a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5z"/>"##);
svg!(FOLDER_O, "folder-o", r##"<path fill="none" stroke="#fff" stroke-width="1.8" stroke-linejoin="round" d="M3 6.5A2.5 2.5 0 0 1 5.5 4h3.6c.6 0 1.2.2 1.6.6L12.4 6h6.1A2.5 2.5 0 0 1 21 8.5v9a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5z"/>"##);
svg!(FILE, "file", r##"<path fill="#fff" fill-opacity=".9" d="M6 2.5h7.6c.4 0 .8.2 1.1.4l4.4 4.4c.3.3.4.7.4 1.1V19a2.5 2.5 0 0 1-2.5 2.5h-11A2.5 2.5 0 0 1 3.5 19V5A2.5 2.5 0 0 1 6 2.5z"/><path fill="#000" fill-opacity=".25" d="M13.5 2.6V7a1.5 1.5 0 0 0 1.5 1.5h4.4z"/>"##);
svg!(CHEV, "chev", r##"<path fill="none" stroke="#fff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" d="m9 6 6 6-6 6"/>"##);
svg!(CHEV_DOWN, "chev-down", r##"<path fill="none" stroke="#fff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" d="m6 9 6 6 6-6"/>"##);
svg!(SCAN, "scan", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M4 8V6a2 2 0 0 1 2-2h2M16 4h2a2 2 0 0 1 2 2v2M20 16v2a2 2 0 0 1-2 2h-2M8 20H6a2 2 0 0 1-2-2v-2M4 12h16"/>"##);
svg!(REFRESH, "refresh", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M20 11a8 8 0 0 0-14.9-3.9L4 9M4 4v5h5M4 13a8 8 0 0 0 14.9 3.9L20 15m0 5v-5h-5"/>"##);
svg!(IMPORT, "import", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M12 3v12m0 0-4-4m4 4 4-4M4 17v1a3 3 0 0 0 3 3h10a3 3 0 0 0 3-3v-1"/>"##);
svg!(EXPORT, "export", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M12 15V3m0 0L8 7m4-4 4 4M4 17v1a3 3 0 0 0 3 3h10a3 3 0 0 0 3-3v-1"/>"##);
svg!(SHIELD, "shield", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M12 3 5 6v5c0 4.4 3 8.3 7 9.5 4-1.2 7-5.1 7-9.5V6z"/><path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="m9 12 2 2 4-4"/>"##);
svg!(BOLT, "bolt", r##"<path fill="#fff" d="M13.5 2 4 14h7l-1.5 8L19 10h-7z"/>"##);
svg!(SEARCH, "search", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" d="m21 21-5.2-5.2M17 10.5a6.5 6.5 0 1 1-13 0 6.5 6.5 0 0 1 13 0z"/>"##);
svg!(TREE, "tree", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M4 5h6M4 5v14h6M4 12h6M14 5h6M14 12h6M14 19h6"/>"##);
svg!(COPY, "copy", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linejoin="round" d="M9 9h10v11H9zM5 15V4h10"/>"##);
svg!(TRASH, "trash", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M4 7h16M10 11v6M14 11v6M5 7l1 12a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2l1-12M9 7V4h6v3"/>"##);
svg!(SPARKLE, "sparkle", r##"<path fill="#fff" d="M12 2c.4 3.6 2.4 5.6 6 6-3.6.4-5.6 2.4-6 6-.4-3.6-2.4-5.6-6-6 3.6-.4 5.6-2.4 6-6zM18.5 14c.2 1.8 1.2 2.8 3 3-1.8.2-2.8 1.2-3 3-.2-1.8-1.2-2.8-3-3 1.8-.2 2.8-1.2 3-3z"/>"##);
svg!(DUP, "dup", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linejoin="round" d="M8 8h11v13H8zM5 16V3h11"/><path stroke="#fff" stroke-width="2" stroke-linecap="round" d="M11.5 14.5h4"/>"##);
svg!(LIST, "list", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" d="M8 6h13M8 12h13M8 18h13M3.5 6h.01M3.5 12h.01M3.5 18h.01"/>"##);
svg!(GRID, "grid", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linejoin="round" d="M3 3h11v11H3zM14 3h7v6h-7zM14 9h7v12h-7zM3 14h11v7H3z"/>"##);
svg!(SUN, "sun", r##"<circle cx="12" cy="12" r="3" fill="#fff"/><path fill="none" stroke="#fff" stroke-width="2.2" d="M12 5.5a6.5 6.5 0 0 1 6.5 6.5M17.7 15.3A6.5 6.5 0 0 1 6 14.5M5.6 10A6.5 6.5 0 0 1 9 6.2"/>"##);
svg!(UP, "up", r##"<path fill="none" stroke="#fff" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" d="M12 19V5m0 0-6 6m6-6 6 6"/>"##);
svg!(X, "x", r##"<path stroke="#fff" stroke-width="2.2" stroke-linecap="round" d="M6 6l12 12M18 6 6 18"/>"##);
svg!(OPEN, "open", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M14 4h6v6M20 4l-9 9M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4"/>"##);
svg!(TARGET, "target", r##"<circle cx="12" cy="12" r="8" fill="none" stroke="#fff" stroke-width="2"/><circle cx="12" cy="12" r="3" fill="#fff"/>"##);
svg!(INFO, "info", r##"<circle cx="12" cy="12" r="9" fill="none" stroke="#fff" stroke-width="2"/><path fill="none" stroke="#fff" stroke-width="2.2" stroke-linecap="round" d="M12 11v5M12 7.6h.01"/>"##);
svg!(GLOBE, "globe", r##"<circle cx="12" cy="12" r="9" fill="none" stroke="#fff" stroke-width="2"/><path fill="none" stroke="#fff" stroke-width="2" d="M3 12h18M12 3c2.5 2.6 3.6 5.6 3.6 9s-1.1 6.4-3.6 9c-2.5-2.6-3.6-5.6-3.6-9s1.1-6.4 3.6-9z"/>"##);
svg!(DOWNLOAD, "download", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M12 3v12m0 0-4-4m4 4 4-4M4 17v1a3 3 0 0 0 3 3h10a3 3 0 0 0 3-3v-1"/>"##);
svg!(HOME,"home", r##"<path fill="none" stroke="#fff" stroke-width="2" stroke-linejoin="round" d="M4 11 12 4l8 7v8a1 1 0 0 1-1 1h-5v-6h-4v6H5a1 1 0 0 1-1-1z"/>"##);

pub fn paint(ui: &Ui, icon: Icon, rect: Rect, color: Color32) {
    egui::Image::new(egui::ImageSource::Bytes { uri: icon.0.into(), bytes: egui::load::Bytes::Static(icon.1.as_bytes()) })
        .tint(color)
        .paint_at(ui, rect);
}
