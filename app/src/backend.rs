use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::scan::{self, ScanCtl};
use crate::tree::{self, Tree};
use crate::{fsinfo, mft, sys};

#[derive(Clone, Debug)]
pub struct ScanInfo {
    pub kind: String,
    pub engine: String,
    pub title: String,
    pub root_path: String,
    pub capacity: Option<u64>,
    pub free: Option<u64>,
    pub elapsed_ms: u64,
    pub note: Option<String>,
}

impl ScanInfo {
    pub fn fast(&self) -> bool {
        self.engine == "NTFS MFT"
    }
}

fn drive_letter_of(path: &str) -> Option<char> {
    let mut c = path.chars();
    let l = c.next()?;
    (l.is_ascii_alphabetic() && c.next() == Some(':')).then(|| l.to_ascii_uppercase())
}

pub fn run_scan(kind: &str, target: &str, ctl: &ScanCtl) -> Result<(Tree, ScanInfo), String> {
    let started = Instant::now();
    let drives = fsinfo::enumerate();
    let mut note = None;

    let finish = |tree: Tree, engine: &str, kind: &str, title: String, cap: Option<u64>, free: Option<u64>, note: Option<String>| {
        let info = ScanInfo {
            kind: kind.into(),
            engine: engine.into(),
            title,
            root_path: tree.root_path.clone(),
            capacity: cap,
            free,
            elapsed_ms: started.elapsed().as_millis() as u64,
            note,
        };
        (tree, info)
    };

    if kind == "csv" {
        ctl.set_phase("Reading CSV");
        let bytes = std::fs::read(target).map_err(|e| format!("Could not read {target}: {e}"))?;
        let text = if bytes.starts_with(&[0xFF, 0xFE]) {
            let w: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            String::from_utf16_lossy(&w)
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        ctl.set_phase("Building tree");
        let (raw, root_path) = scan::import_csv(&text)?;
        let name = PathBuf::from(target).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let tree = tree::build(raw, 0, root_path.clone(), root_path).ok_or("CSV produced an empty tree")?;
        return Ok(finish(tree, "CSV import", "csv", name, None, None, None));
    }

    let target = target.trim().to_string();
    let letter = drive_letter_of(&target).ok_or_else(|| format!("Not a local path: {target}"))?;
    let drive = drives.iter().find(|d| d.id.starts_with(letter));
    let is_drive_root = target.trim_end_matches('\\').len() == 2;
    let ntfs = drive.map(|d| d.fs.eq_ignore_ascii_case("NTFS")).unwrap_or(false);
    let (cap, free) = match drive {
        Some(d) if is_drive_root => (Some(d.capacity), Some(d.free)),
        _ => (None, None),
    };
    let label = drive.map(|d| d.label.clone()).unwrap_or_default();
    let title = if is_drive_root {
        if label.is_empty() { format!("Local Disk ({letter}:)") } else { format!("{label} ({letter}:)") }
    } else {
        target.trim_end_matches('\\').to_string()
    };

    if ntfs {
        match mft::scan(letter, ctl) {
            Ok((raw, root, _bytes)) => {
                ctl.set_phase("Building tree");
                let mut tree = tree::build(raw, root, format!("{letter}:"), format!("{letter}:\\")).ok_or("MFT produced an empty tree")?;
                if !is_drive_root {
                    let id = tree.find_path(&target).ok_or_else(|| format!("Folder not found on volume: {target}"))?;
                    let p = tree.path(id);
                    tree.root = id;
                    tree.root_path = p;
                }
                return Ok(finish(tree, "NTFS MFT", if is_drive_root { "drive" } else { "folder" }, title, cap, free, None));
            }
            Err(mft::MftError::Cancelled) => return Err("Scan cancelled".into()),
            Err(mft::MftError::Access(e)) => {
                note = Some(if sys::is_admin() {
                    format!("Raw volume access failed ({e}); used the folder walk instead.")
                } else {
                    "Fast MFT scan needs administrator rights, so the slower folder walk was used.".to_string()
                });
            }
            Err(mft::MftError::Corrupt(why)) => note = Some(format!("MFT read failed ({why}); used the folder walk instead.")),
            Err(mft::MftError::NotNtfs) => {}
        }
        ctl.done.store(0, Ordering::Relaxed);
        ctl.total.store(0, Ordering::Relaxed);
    }

    let root = PathBuf::from(if is_drive_root { format!("{letter}:\\") } else { target.clone() });
    if !root.is_dir() {
        return Err(format!("Not a folder: {}", root.display()));
    }
    let raw = scan::walk_folder(&root, fsinfo::cluster_size(&target), ctl);
    if ctl.cancel.load(Ordering::Relaxed) {
        return Err("Scan cancelled".into());
    }
    ctl.set_phase("Building tree");
    let root_path = root.display().to_string();
    let root_name = if is_drive_root {
        format!("{letter}:")
    } else {
        root.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or(root_path.clone())
    };
    let tree = tree::build(raw, 0, root_name, root_path).ok_or("Folder walk produced an empty tree")?;
    Ok(finish(tree, "Folder walk", if is_drive_root { "drive" } else { "folder" }, title, cap, free, note))
}

#[derive(Default, Debug, Clone)]
pub struct DeleteResult {
    pub freed: u64,
    pub removed: u64,
    pub failed: Vec<String>,
}

pub fn is_protected(path: &str) -> bool {
    let p = path.trim_end_matches('\\').to_lowercase();
    if p.len() <= 2 {
        return true;
    }
    let rest = p[2..].trim_start_matches('\\');
    let parts: Vec<&str> = rest.split('\\').collect();
    let top = parts[0];
    const CRITICAL: [&str; 10] = [
        "windows", "program files", "program files (x86)", "programdata", "users", "boot", "recovery",
        "system volume information", "pagefile.sys", "hiberfil.sys",
    ];
    if parts.len() == 1 && (CRITICAL.contains(&top) || top.starts_with('$') || top == "swapfile.sys") {
        return true;
    }
    if top == "windows" && !rest.starts_with(r"windows\temp") && !rest.starts_with(r"windows\softwaredistribution\download") {
        return true;
    }
    top == "users" && parts.len() == 2
}

fn remove_path(p: &std::path::Path, permanent: bool) -> std::io::Result<()> {
    if permanent {
        if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) }
    } else {
        trash::delete(p).map_err(|e| std::io::Error::other(e.to_string()))
    }
}

pub fn delete_items(shared: &std::sync::RwLock<Option<Tree>>, ids: &[u32], permanent: bool, empty_only: bool) -> DeleteResult {
    let targets: Vec<(u32, String, u64)> = {
        let g = shared.read().unwrap();
        let Some(t) = g.as_ref() else { return DeleteResult::default() };
        ids.iter().filter(|&&i| t.valid(i) && i != t.root).map(|&i| (i, t.path(i), t.nodes[i as usize].alloc)).collect()
    };

    let mut result = DeleteResult::default();
    let mut gone = Vec::new();
    let mut partial = Vec::new();
    for (id, path, alloc) in targets {
        let p = PathBuf::from(&path);
        if is_protected(&path) && !empty_only {
            result.failed.push(format!("{path}: protected system location"));
            continue;
        }
        if p.symlink_metadata().is_err() {
            gone.push(id);
            continue;
        }
        if empty_only && p.is_dir() {
            let mut any_fail = false;
            if let Ok(rd) = std::fs::read_dir(&p) {
                for e in rd.flatten() {
                    if remove_path(&e.path(), permanent).is_err() {
                        any_fail = true;
                    }
                }
            }
            partial.push(id);
            if any_fail {
                result.failed.push(format!("{path}: some files are in use and were skipped"));
            }
            continue;
        }
        match remove_path(&p, permanent) {
            Ok(()) => {
                result.freed += alloc;
                result.removed += 1;
                gone.push(id);
            }
            Err(e) => result.failed.push(format!("{path}: {e}")),
        }
    }

    let mut g = shared.write().unwrap();
    if let Some(t) = g.as_mut() {
        for id in gone {
            t.remove(id);
        }
        for id in partial {
            let kids: Vec<u32> = t.children(id).collect();
            for k in kids {
                if std::fs::symlink_metadata(t.path(k)).is_err() {
                    result.freed += t.nodes[k as usize].alloc;
                    t.remove(k);
                }
            }
            result.removed += 1;
        }
    }
    result
}

pub fn export_csv(shared: &std::sync::RwLock<Option<Tree>>, id: u32, path: &std::path::Path) -> Result<u64, String> {
    let g = shared.read().unwrap();
    let t = g.as_ref().ok_or("Nothing scanned yet")?;
    scan::export_csv(t, id, path).map_err(|e| format!("Export failed: {e}"))
}

pub fn age_cutoff(days: u32) -> u64 {
    if days == 0 {
        return 0;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (now.saturating_sub(days as u64 * 86_400) + 11_644_473_600) * 10_000_000
}
