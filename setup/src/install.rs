use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, KEY_READ};
use winreg::RegKey;

pub static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.gz"));
pub const PAYLOAD_LEN: u64 = include!(concat!(env!("OUT_DIR"), "/payload_len.rs"));
pub const LICENSE: &str = include_str!("../../installer/license.txt");

pub const APP: &str = "PrismDisk";
pub const APP_EXE: &str = "PrismDisk.exe";
pub const UNINSTALL_EXE: &str = "Uninstall PrismDisk.exe";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const PUBLISHER: &str = "PrismDisk";
const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PrismDisk";
const CTX_KEYS: [&str; 2] = [r"SOFTWARE\Classes\Directory\shell\PrismDisk", r"SOFTWARE\Classes\Drive\shell\PrismDisk"];
const LAYERS_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn env_path(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(fallback))
}
pub fn default_dir() -> PathBuf {
    env_path("ProgramW6432", r"C:\Program Files").join(APP)
}
fn start_menu_lnk() -> PathBuf {
    env_path("ProgramData", r"C:\ProgramData").join(r"Microsoft\Windows\Start Menu\Programs").join(format!("{APP}.lnk"))
}
fn desktop_lnk() -> PathBuf {
    env_path("PUBLIC", r"C:\Users\Public").join("Desktop").join(format!("{APP}.lnk"))
}
fn app_data_dirs() -> Vec<PathBuf> {
    vec![env_path("LOCALAPPDATA", "").join("com.prismdisk.analyzer")]
}

fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn kill_app() {
    let _ = hidden(Command::new("taskkill").args(["/F", "/IM", APP_EXE])).output();
}

#[cfg(windows)]
pub fn free_space(path: &Path) -> Option<u64> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let mut p = path.to_path_buf();
    while !p.exists() {
        p = p.parent()?.to_path_buf();
    }
    let mut free = 0u64;
    unsafe { GetDiskFreeSpaceExW(&HSTRING::from(p.as_os_str()), Some(&mut free), None, None) }.ok()?;
    Some(free)
}
#[cfg(not(windows))]
pub fn free_space(_p: &Path) -> Option<u64> {
    None
}

#[cfg(windows)]
fn create_shortcut(lnk: &Path, target: &Path, workdir: &Path, desc: &str) -> Result<(), String> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        link.SetPath(&HSTRING::from(target.as_os_str())).map_err(|e| e.to_string())?;
        link.SetWorkingDirectory(&HSTRING::from(workdir.as_os_str())).map_err(|e| e.to_string())?;
        link.SetDescription(&HSTRING::from(desc)).map_err(|e| e.to_string())?;
        link.SetIconLocation(&HSTRING::from(target.as_os_str()), 0).map_err(|e| e.to_string())?;
        let pf: IPersistFile = link.cast().map_err(|e| e.to_string())?;
        if let Some(dir) = lnk.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        pf.Save(&HSTRING::from(lnk.as_os_str()), true).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[cfg(not(windows))]
fn create_shortcut(_: &Path, _: &Path, _: &Path, _: &str) -> Result<(), String> {
    Ok(())
}

fn hklm() -> RegKey {
    RegKey::predef(HKEY_LOCAL_MACHINE)
}

#[derive(Clone)]
pub struct Existing {
    pub dir: String,
    pub version: String,
}

pub fn existing_install() -> Option<Existing> {
    let k = hklm().open_subkey_with_flags(UNINSTALL_KEY, KEY_READ).ok()?;
    let dir: String = k.get_value("InstallLocation").ok()?;
    let version: String = k.get_value("DisplayVersion").unwrap_or_default();
    Some(Existing { dir, version })
}

pub fn previous_options() -> Option<Options> {
    let k = hklm().open_subkey_with_flags(UNINSTALL_KEY, KEY_READ).ok()?;
    let dir: String = k.get_value("InstallLocation").ok()?;
    let flag = |name: &str| k.get_value::<u32, _>(name).map(|v| v != 0).unwrap_or(true);
    Some(Options { dir, desktop: flag("OptDesktop"), start_menu: flag("OptStartMenu"), context_menu: flag("OptContextMenu"), run_as_admin: flag("OptRunAsAdmin") })
}

pub fn launch_app(dir: &str) {
    let exe = PathBuf::from(dir).join(APP_EXE);
    let _ = Command::new("explorer.exe").arg(exe).spawn();
}

#[derive(Clone)]
pub struct Options {
    pub dir: String,
    pub desktop: bool,
    pub start_menu: bool,
    pub context_menu: bool,
    pub run_as_admin: bool,
}

pub fn do_install(o: &Options, report: &dyn Fn(f64, &str, u8)) -> Result<(), String> {
    if PAYLOAD.is_empty() {
        return Err("This setup was built without the PrismDisk app inside. Rebuild it with build.ps1.".into());
    }
    let dir = PathBuf::from(o.dir.trim());
    if !dir.is_absolute() {
        return Err("Please choose a full install path, like C:\\Program Files\\PrismDisk.".into());
    }

    report(2.0, "Preparing install folder", 0);
    kill_app();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    std::thread::sleep(Duration::from_millis(250));

    report(5.0, "Extracting PrismDisk", 1);
    let exe = dir.join(APP_EXE);
    let tmp = dir.join(format!("{APP_EXE}.new"));
    {
        let mut dec = flate2::read::GzDecoder::new(PAYLOAD);
        let mut f = std::fs::File::create(&tmp).map_err(|e| format!("Could not write {}: {e}", tmp.display()))?;
        let mut buf = vec![0u8; 256 * 1024];
        let mut done = 0u64;
        loop {
            let n = dec.read(&mut buf).map_err(|e| format!("Corrupt installer payload: {e}"))?;
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n]).map_err(|e| format!("Write failed: {e}"))?;
            done += n as u64;
            report(5.0 + 60.0 * done as f64 / PAYLOAD_LEN.max(1) as f64, "Extracting PrismDisk", 1);
            std::thread::sleep(Duration::from_millis(8));
        }
        f.flush().ok();
    }
    let mut last_err = None;
    for _ in 0..10 {
        match std::fs::rename(&tmp, &exe) {
            Ok(()) => {
                last_err = None;
                break;
            }
            Err(e) => {
                let _ = std::fs::remove_file(&exe);
                last_err = Some(e);
                kill_app();
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
    if let Some(e) = last_err {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("Could not replace {} (is PrismDisk still running?): {e}", exe.display()));
    }

    report(68.0, "Installing uninstaller", 2);
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let un = dir.join(UNINSTALL_EXE);
    if me != un {
        std::fs::copy(&me, &un).map_err(|e| format!("Could not write uninstaller: {e}"))?;
    }
    std::fs::write(dir.join("LICENSE.txt"), LICENSE).ok();
    std::thread::sleep(Duration::from_millis(200));

    report(76.0, "Creating shortcuts", 3);
    let _ = std::fs::remove_file(start_menu_lnk());
    let _ = std::fs::remove_file(desktop_lnk());
    if o.start_menu {
        create_shortcut(&start_menu_lnk(), &exe, &dir, "Modern disk space analyzer")?;
    }
    if o.desktop {
        create_shortcut(&desktop_lnk(), &exe, &dir, "Modern disk space analyzer")?;
    }
    std::thread::sleep(Duration::from_millis(200));

    report(86.0, "Registering with Windows", 4);
    let size_kb = (PAYLOAD_LEN + std::fs::metadata(&un).map(|m| m.len()).unwrap_or(0)) / 1024;
    let (k, _) = hklm().create_subkey(UNINSTALL_KEY).map_err(|e| format!("Registry: {e}"))?;
    let exe_s = exe.display().to_string();
    let un_s = un.display().to_string();
    let today = {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
        let (y, m, d) = civil(secs / 86400);
        format!("{y:04}{m:02}{d:02}")
    };
    let r: std::io::Result<()> = (|| {
        k.set_value("DisplayName", &APP)?;
        k.set_value("DisplayVersion", &VERSION)?;
        k.set_value("Publisher", &PUBLISHER)?;
        k.set_value("DisplayIcon", &format!("{exe_s},0"))?;
        k.set_value("InstallLocation", &dir.display().to_string())?;
        k.set_value("UninstallString", &format!("\"{un_s}\" --uninstall"))?;
        k.set_value("QuietUninstallString", &format!("\"{un_s}\" --uninstall --silent"))?;
        k.set_value("InstallDate", &today)?;
        k.set_value("EstimatedSize", &(size_kb as u32))?;
        k.set_value("NoModify", &1u32)?;
        k.set_value("NoRepair", &1u32)?;
        k.set_value("OptDesktop", &(o.desktop as u32))?;
        k.set_value("OptContextMenu", &(o.context_menu as u32))?;
        k.set_value("OptStartMenu", &(o.start_menu as u32))?;
        k.set_value("OptRunAsAdmin", &(o.run_as_admin as u32))?;
        Ok(())
    })();
    r.map_err(|e| format!("Registry: {e}"))?;

    for key in CTX_KEYS {
        let _ = hklm().delete_subkey_all(key);
    }
    if o.context_menu {
        for (key, arg) in CTX_KEYS.iter().zip(["%V", "%1"]) {
            let (k, _) = hklm().create_subkey(key).map_err(|e| format!("Registry: {e}"))?;
            k.set_value("MUIVerb", &"Analyze with PrismDisk").map_err(|e| e.to_string())?;
            k.set_value("Icon", &format!("\"{exe_s}\",0")).map_err(|e| e.to_string())?;
            let (c, _) = k.create_subkey("command").map_err(|e| e.to_string())?;
            c.set_value("", &format!("\"{exe_s}\" \"{arg}\"")).map_err(|e| e.to_string())?;
        }
    }

    if let Ok((layers, _)) = hklm().create_subkey(LAYERS_KEY) {
        if o.run_as_admin {
            let _ = layers.set_value(&exe_s, &"~ RUNASADMIN");
        } else {
            let _ = layers.delete_value(&exe_s);
        }
    }
    std::thread::sleep(Duration::from_millis(250));
    report(100.0, "Done", 5);
    Ok(())
}

fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

pub fn do_uninstall(keep_data: bool, report: &dyn Fn(f64, &str, u8)) -> Result<PathBuf, String> {
    let dir = existing_install()
        .map(|e| PathBuf::from(e.dir))
        .or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)))
        .ok_or("PrismDisk does not appear to be installed.")?;

    report(5.0, "Closing PrismDisk", 0);
    kill_app();
    std::thread::sleep(Duration::from_millis(400));

    report(25.0, "Removing program files", 1);
    let exe = dir.join(APP_EXE);
    for _ in 0..10 {
        if !exe.exists() || std::fs::remove_file(&exe).is_ok() {
            break;
        }
        kill_app();
        std::thread::sleep(Duration::from_millis(300));
    }
    let _ = std::fs::remove_file(dir.join("LICENSE.txt"));
    std::thread::sleep(Duration::from_millis(200));

    report(50.0, "Removing shortcuts", 2);
    let _ = std::fs::remove_file(start_menu_lnk());
    let _ = std::fs::remove_file(desktop_lnk());
    std::thread::sleep(Duration::from_millis(200));

    report(70.0, "Cleaning up the registry", 3);
    for key in CTX_KEYS {
        let _ = hklm().delete_subkey_all(key);
    }
    if let Ok(layers) = hklm().open_subkey_with_flags(LAYERS_KEY, KEY_ALL_ACCESS) {
        let _ = layers.delete_value(exe.display().to_string());
    }
    let _ = hklm().delete_subkey_all(UNINSTALL_KEY);
    std::thread::sleep(Duration::from_millis(200));

    report(88.0, if keep_data { "Keeping your settings" } else { "Removing settings and cache" }, 4);
    if !keep_data {
        for d in app_data_dirs() {
            if d.is_absolute() {
                let _ = std::fs::remove_dir_all(d);
            }
        }
    }
    std::thread::sleep(Duration::from_millis(250));
    report(100.0, "Done", 5);
    Ok(dir)
}

pub fn schedule_self_delete(dir: &Path) {
    let me = std::env::current_exe().ok();
    let inside = me.as_ref().map(|m| m.starts_with(dir)).unwrap_or(false);
    let script = if inside {
        format!("ping 127.0.0.1 -n 3 > nul & rmdir /s /q \"{}\"", dir.display())
    } else {
        format!("rmdir /s /q \"{}\"", dir.display())
    };
    let mut cmd = Command::new("cmd");
    hidden(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.arg("/C").raw_arg(&script);
    }
    let _ = cmd.spawn();
}
