#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod app;
mod backend;
mod cache;
mod container;
mod decode;
mod fsinfo;
mod gallery;
mod hashing;
mod icons;
mod md;
mod media;
mod mft;
mod scan;
mod sys;
mod theme;
mod thumbs;
mod tree;
mod ui;
mod update;
mod viz;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--media-index") {
        if let Some(list) = args.get(i + 1) {
            media::child_main(list);
        }
        return Ok(());
    }
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../icons/128x128@2x.png")).ok();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(format!("PrismDisk {}", env!("CARGO_PKG_VERSION")))
        .with_inner_size(initial_size())
        .with_min_inner_size([960.0, 600.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "PrismDisk",
        options,
        Box::new(|cc| {
            sys::style_titlebar(cc);
            Ok(Box::new(app::App::new(cc)))
        }),
    )
}

fn initial_size() -> [f32; 2] {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::HiDpi::GetDpiForSystem;
        use windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS};
        let mut r = RECT::default();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut r as *mut _ as _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok() {
            let scale = GetDpiForSystem().max(96) as f32 / 96.0;
            let (w, h) = ((r.right - r.left) as f32 / scale, (r.bottom - r.top) as f32 / scale);
            return [1440f32.min(w * 0.94), 920f32.min(h * 0.92 - 32.0)];
        }
    }
    [1440.0, 920.0]
}
