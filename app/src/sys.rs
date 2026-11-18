use std::process::Command;

#[cfg(windows)]
pub fn is_admin() -> bool {
    unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().as_bool() }
}
#[cfg(not(windows))]
pub fn is_admin() -> bool {
    false
}

#[cfg(windows)]
pub fn relaunch_elevated(args: &str) -> bool {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let Ok(exe) = std::env::current_exe() else { return false };
    let exe = HSTRING::from(exe.as_os_str());
    let params = HSTRING::from(args);
    let r = unsafe { ShellExecuteW(None, &HSTRING::from("runas"), &exe, &params, PCWSTR::null(), SW_SHOWNORMAL) };
    r.0 as isize > 32
}
#[cfg(not(windows))]
pub fn relaunch_elevated(_args: &str) -> bool {
    false
}

pub fn reveal(path: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("explorer.exe").raw_arg(format!("/select,\"{path}\"")).spawn()?;
    }
    #[cfg(not(windows))]
    Command::new("xdg-open").arg(path).spawn()?;
    Ok(())
}

pub fn open(path: &str) -> std::io::Result<()> {
    Command::new(if cfg!(windows) { "explorer.exe" } else { "xdg-open" }).arg(path).spawn()?;
    Ok(())
}

pub fn quote_arg(s: &str) -> String {
    if !s.is_empty() && !s.contains([' ', '\t', '"']) {
        return s.to_string();
    }
    let mut out = String::from("\"");
    let mut bs = 0;
    for c in s.chars() {
        match c {
            '\\' => bs += 1,
            '"' => {
                out.push_str(&"\\".repeat(bs * 2 + 1));
                out.push('"');
                bs = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(bs));
                out.push(c);
                bs = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(bs * 2));
    out.push('"');
    out
}

#[cfg(windows)]
pub fn style_titlebar(cc: &eframe::CreationContext<'_>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{BOOL, HWND};
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let Ok(h) = cc.window_handle() else { return };
    let RawWindowHandle::Win32(w) = h.as_raw() else { return };
    let hwnd = HWND(w.hwnd.get() as _);
    unsafe {
        let dark = BOOL(1);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
        let color: u32 = 0x00140e0b;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &color as *const _ as _, 4);
    }
}
#[cfg(not(windows))]
pub fn style_titlebar(_cc: &eframe::CreationContext<'_>) {}

#[cfg(windows)]
pub fn shell_open(target: &str, args: &str) -> bool {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let r = unsafe { ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from(target), &HSTRING::from(args), PCWSTR::null(), SW_SHOWNORMAL) };
    r.0 as isize > 32
}
#[cfg(not(windows))]
pub fn shell_open(target: &str, _args: &str) -> bool {
    Command::new("xdg-open").arg(target).spawn().is_ok()
}
