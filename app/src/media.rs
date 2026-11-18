use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;

use rayon::prelude::*;

use crate::cache::{put_str, Reader, Record, Store};
use crate::decode;
use crate::hashing::{self, FileRef};
use crate::tree::Tree;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Image,
    Video,
}

const IMAGE_EXTS: [&str; 22] = [
    "jpg", "jpeg", "jpe", "jfif", "png", "webp", "gif", "bmp", "tif", "tiff", "heic", "heif", "avif", "jxr", "dng", "cr2", "cr3", "nef", "arw", "orf", "rw2", "raf",
];
const VIDEO_EXTS: [&str; 14] = ["mp4", "m4v", "mov", "mkv", "avi", "wmv", "webm", "mts", "m2ts", "3gp", "mpg", "mpeg", "flv", "asf"];

pub fn kind_of(name: &str) -> Option<Kind> {
    let (_, ext) = name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    if IMAGE_EXTS.contains(&ext.as_str()) {
        Some(Kind::Image)
    } else if VIDEO_EXTS.contains(&ext.as_str()) {
        Some(Kind::Video)
    } else {
        None
    }
}

#[derive(Clone, Default, Debug)]
pub struct Meta {
    pub ok: bool,
    pub w: u32,
    pub h: u32,
    pub taken: i64,
    pub exif: bool,
    pub camera: String,
    pub phash: [u64; 4],
    pub detail: f32,
    pub sharp: f32,
    pub duration_ms: u32,
    pub vhash: Vec<u64>,
}

impl Record for Meta {
    const LEN: usize = 192;
    const TAG: &'static [u8; 4] = b"PDM2";
    fn write(&self, out: &mut Vec<u8>) {
        out.push(self.ok as u8);
        out.push(self.exif as u8);
        out.extend_from_slice(&self.w.to_le_bytes());
        out.extend_from_slice(&self.h.to_le_bytes());
        out.extend_from_slice(&self.taken.to_le_bytes());
        for h in self.phash {
            out.extend_from_slice(&h.to_le_bytes());
        }
        out.extend_from_slice(&self.detail.to_le_bytes());
        out.extend_from_slice(&self.sharp.to_le_bytes());
        out.extend_from_slice(&self.duration_ms.to_le_bytes());
        out.push(self.vhash.len().min(5) as u8);
        for i in 0..5 {
            out.extend_from_slice(&self.vhash.get(i).copied().unwrap_or(0).to_le_bytes());
        }
        put_str(out, &self.camera, 48);
    }
    fn read(b: &[u8]) -> Self {
        let mut r = Reader::new(b);
        let ok = r.u8() != 0;
        let exif = r.u8() != 0;
        let w = r.u32();
        let h = r.u32();
        let taken = r.i64();
        let phash = [r.u64(), r.u64(), r.u64(), r.u64()];
        let detail = r.f32();
        let sharp = r.f32();
        let duration_ms = r.u32();
        let n = r.u8() as usize;
        let all: Vec<u64> = (0..5).map(|_| r.u64()).collect();
        let camera = r.str(48);
        Meta { ok, w, h, taken, exif, camera, phash, detail, sharp, duration_ms, vhash: all[..n.min(5)].to_vec() }
    }
}

fn store() -> &'static Store<Meta> {
    static S: OnceLock<Store<Meta>> = OnceLock::new();
    S.get_or_init(|| Store::open("media.bin", 400_000))
}

#[derive(Clone)]
pub struct Item {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: u64,
    pub modified_ms: i64,
    pub kind: Kind,
    pub meta: Option<Meta>,
}

impl Item {
    pub fn folder(&self) -> &str {
        &self.path[..self.path.len().saturating_sub(self.name.len() + 1)]
    }
    pub fn date_ms(&self) -> i64 {
        match &self.meta {
            Some(m) if m.taken > 0 => m.taken,
            _ => self.modified_ms,
        }
    }
    pub fn has_date(&self) -> bool {
        self.meta.as_ref().map(|m| m.taken > 0).unwrap_or(false) || self.kind == Kind::Video
    }
    pub fn is_screenshot(&self) -> bool {
        if self.kind != Kind::Image {
            return false;
        }
        let n = self.name.to_lowercase();
        let p = self.path.to_lowercase();
        n.contains("screenshot") || n.contains("screen shot") || n.contains("bildschirmfoto") || n.starts_with("captura") || n.starts_with("snip") || p.contains("\\screenshots\\")
    }
    pub fn is_blurry(&self) -> bool {
        match &self.meta {
            Some(m) => self.kind == Kind::Image && m.ok && m.detail >= 10.0 && m.sharp < BLUR_LIMIT && m.sharp < m.detail * 0.42 && !self.is_screenshot(),
            None => false,
        }
    }
    pub fn pixels(&self) -> u64 {
        self.meta.as_ref().map(|m| m.w as u64 * m.h as u64).unwrap_or(0)
    }
}

pub const BLUR_LIMIT: f32 = 14.0;
pub const LARGE_VIDEO: u64 = 500 << 20;

const SYSTEM_DIRS: [&str; 8] = ["windows", "$recycle.bin", "system volume information", "$windows.~bt", "$windows.~ws", "$sysreset", "recovery", "msocache"];

const SKIP_DIRS: [&str; 53] = [
    "windows", "program files", "program files (x86)", "programdata", "$recycle.bin", "system volume information", "appdata", "node_modules", ".git", ".cache",
    "steamapps", "$windows.~bt", "$windows.~ws", "msocache", "target", ".venv", "venv", "site-packages", "__pycache__", "vendor", "bower_components", ".gradle",
    ".nuget", ".cargo", ".rustup", ".npm", ".vscode", ".idea", "dist-packages", "$sysreset", "steamlibrary", "riot games", "epic games", "xboxgames",
    "gog games", "ea games", "ubisoft", "battle.net", "my games", "windowsapps", "saved games", "assets", "textures", "sprites", "icons", "modtools",
    "static", "public", "res", "resources", "thumbs", "thumbnails", ".thumbnails",
];

const CODE_EXTS: [&str; 34] = [
    "html", "htm", "js", "mjs", "cjs", "ts", "tsx", "jsx", "css", "scss", "vue", "svelte", "py", "rs", "cs", "cpp", "cc", "c", "h", "hpp", "java", "kt", "go",
    "lua", "luau", "gd", "php", "rb", "swift", "gsc", "csc", "ps1", "bat", "sh",
];

const MEDIA_DIRS: [&str; 33] = [
    "dcim", "camera roll", "camera uploads", "camera", "whatsapp images", "whatsapp video", "whatsapp animated gifs", "telegram desktop", "telegram images",
    "telegram video", "screenshots", "pictures", "my pictures", "videos", "my videos", "photos", "fotos", "bilder", "icloud photos", "google photos", "saved pictures",
    "snapchat", "instagram", "screen recordings", "medal", "steelseries moments", "clips", "captures", "game clips", "radeon relive", "shadowplay",
    "outplayed", "highlights",
];

const PROJECT_FILES: [&str; 16] = [
    ".git", "package.json", "cargo.toml", "pyproject.toml", "requirements.txt", "project.godot", "go.mod", "pom.xml", "build.gradle", "cmakelists.txt",
    "makefile", "composer.json", "gemfile", "projectsettings", "pubspec.yaml", "deno.json",
];

const PROJECT_EXTS: [&str; 6] = ["sln", "csproj", "vcxproj", "uproject", "xcodeproj", "unityproj"];

const APP_EXTS: [&str; 24] = [
    "exe", "dll", "pak", "uasset", "umap", "unity3d", "assets", "resource", "vpk", "pck", "asar", "bsa", "ba2", "esm", "esp", "wad", "rpf", "forge", "big",
    "sys", "so", "jar", "msi", "ucas",
];

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Scope {
    #[default]
    Personal,
    All,
}

pub fn personal_roots() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Com::CoTaskMemFree;
        use windows::Win32::UI::Shell::*;
        for id in [FOLDERID_Pictures, FOLDERID_Videos, FOLDERID_Desktop, FOLDERID_Downloads, FOLDERID_CameraRoll, FOLDERID_SavedPictures, FOLDERID_Screenshots] {
            if let Ok(p) = SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None) {
                if let Ok(s) = p.to_string() {
                    out.push(s);
                }
                CoTaskMemFree(Some(p.0 as *const _));
            }
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        for sub in ["Pictures", "Videos", "iCloud Photos", "iCloudPhotos", "Dropbox\\Camera Uploads", "Dropbox\\Photos", "Google Drive\\Google Photos"] {
            out.push(format!("{profile}\\{sub}"));
        }
    }
    for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Ok(od) = std::env::var(var) {
            for sub in ["Pictures", "Videos", "Camera Roll", "Desktop"] {
                out.push(format!("{od}\\{sub}"));
            }
        }
    }
    out.extend(clip_app_folders());
    let mut out: Vec<String> = out.into_iter().map(|s| s.trim_end_matches('\\').to_lowercase()).filter(|s| s.len() > 3).collect();
    out.sort();
    out.dedup();
    out
}

fn json_strings(text: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{key}\"");
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(&needle) {
        rest = &rest[i + needle.len()..];
        let after = rest.trim_start();
        let Some(after) = after.strip_prefix(':') else { continue };
        let Some(after) = after.trim_start().strip_prefix('"') else { continue };
        let mut val = String::new();
        let mut chars = after.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => {
                    if let Some(n) = chars.next() {
                        val.push(n);
                    }
                }
                _ => val.push(c),
            }
        }
        if !val.is_empty() {
            out.push(val);
        }
    }
    out
}

fn moments_folders(db: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(db).replace("\\\\", "\\");
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (i, _) in text.match_indices(":\\") {
        if i == 0 {
            continue;
        }
        let drive = text.as_bytes()[i - 1];
        if !drive.is_ascii_alphabetic() {
            continue;
        }
        let tail: String = text[i - 1..].chars().take_while(|c| !c.is_control() && !matches!(c, '"' | '<' | '>' | '|' | '*' | '?')).take(260).collect();
        let lower = tail.to_lowercase();
        let Some(end) = [".mp4", ".mkv", ".webm", ".mov"].iter().filter_map(|e| lower.find(e)).min() else { continue };
        let file = &tail[..end];
        let Some((dir, _)) = file.rsplit_once('\\') else { continue };
        let dir = dir.trim_end_matches("\\thumbs").to_string();
        *counts.entry(dir).or_default() += 1;
    }
    counts.into_keys().collect()
}

pub fn clip_app_folders() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        if let Ok(text) = std::fs::read_to_string(format!("{appdata}\\Medal\\store\\settings.json")) {
            out.extend(json_strings(&text, "clipFolder"));
            out.extend(json_strings(&text, "screenshotFolder"));
        }
    }
    if let Ok(pd) = std::env::var("ProgramData") {
        let db = format!("{pd}\\SteelSeries\\GG\\apps\\moments\\db\\database.db");
        if std::fs::metadata(&db).map(|m| m.len() < 64 << 20).unwrap_or(false) {
            if let Ok(bytes) = std::fs::read(&db) {
                out.extend(moments_folders(&bytes));
            }
        }
    }
    out
}

fn ext_lower(name: &str) -> Option<String> {
    name.rsplit_once('.').filter(|(s, _)| !s.is_empty()).map(|(_, e)| e.to_ascii_lowercase())
}

pub fn camera_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let stem = n.rsplit_once('.').map(|(s, _)| s).unwrap_or(&n);
    const PREFIXES: [&str; 24] = [
        "img_", "img-", "vid_", "vid-", "pxl_", "mvimg_", "pano_", "burst", "dsc_", "dsc0", "dscn", "dscf", "gopr", "dji_", "photo_", "video_", "screenshot",
        "screen shot", "signal-", "snapchat-", "received_", "wp_2", "bildschirmfoto", "captura",
    ];
    if PREFIXES.iter().any(|p| stem.starts_with(p)) {
        return true;
    }
    let digits_after = |prefix: &str, min: usize| stem.strip_prefix(prefix).map(|r| r.chars().take_while(|c| c.is_ascii_digit()).count() >= min).unwrap_or(false);
    if digits_after("img", 4) || digits_after("vid", 4) || digits_after("dsc", 4) || digits_after("gh0", 4) || digits_after("gx0", 4) || digits_after("mvi_", 4) {
        return true;
    }
    let d: Vec<char> = stem.chars().take(15).collect();
    d.len() == 15 && stem.starts_with("20") && d[..8].iter().all(|c| c.is_ascii_digit()) && (d[8] == '_' || d[8] == '-') && d[9..].iter().all(|c| c.is_ascii_digit())
}

fn is_project_or_app(t: &Tree, id: u32) -> bool {
    for c in t.children(id) {
        let name = t.name(c).to_ascii_lowercase();
        if PROJECT_FILES.contains(&name.as_str()) {
            return true;
        }
        if let Some(e) = ext_lower(&name) {
            if PROJECT_EXTS.contains(&e.as_str()) || (!t.nodes[c as usize].is_dir() && (APP_EXTS.contains(&e.as_str()) || CODE_EXTS.contains(&e.as_str()))) {
                return true;
            }
        }
    }
    false
}

fn in_roots(folder_lower: &str, roots: &[String]) -> bool {
    roots.iter().any(|r| folder_lower == r || (folder_lower.starts_with(r.as_str()) && folder_lower.as_bytes().get(r.len()) == Some(&b'\\')))
}

fn media_folder(folder_lower: &str) -> bool {
    folder_lower.split('\\').skip(1).any(|seg| MEDIA_DIRS.contains(&seg))
}

pub fn collect(t: &Tree, start: u32, scope: Scope) -> Vec<Item> {
    let mut roots = if scope == Scope::Personal { personal_roots() } else { Vec::new() };
    let start_path = t.path(start);
    if start_path.trim_end_matches('\\').len() > 2 {
        roots.push(start_path.trim_end_matches('\\').to_lowercase());
    }
    let mut ids = Vec::new();
    t.walk_paths(start, |id, path| {
        let name = t.name(id);
        if t.nodes[id as usize].is_dir() {
            if id == start {
                return true;
            }
            let lower = name.to_lowercase();
            if scope == Scope::Personal {
                let full = path.trim_end_matches('\\').to_lowercase();
                let on_way = roots.iter().any(|r| *r == full || (r.starts_with(full.as_str()) && r.as_bytes().get(full.len()) == Some(&b'\\')));
                if on_way {
                    return true;
                }
            }
            return match scope {
                Scope::All => !SYSTEM_DIRS.contains(&lower.as_str()),
                Scope::Personal => !SKIP_DIRS.contains(&lower.as_str()) && !lower.starts_with('.') && !is_project_or_app(t, id),
            };
        }
        let Some(kind) = kind_of(name) else { return false };
        let n = &t.nodes[id as usize];
        if kind == Kind::Image && n.size < 8 * 1024 {
            return false;
        }
        if scope == Scope::Personal {
            let folder = path[..path.len().saturating_sub(name.len() + 1)].to_lowercase();
            if !(in_roots(&folder, &roots) || media_folder(&folder) || camera_name(name)) {
                return false;
            }
        }
        ids.push((id, kind, path.to_string()));
        false
    });
    ids.into_iter()
        .map(|(id, kind, path)| {
            let n = &t.nodes[id as usize];
            Item { id, name: t.name(id).to_string(), path, size: n.size, modified: n.modified, modified_ms: crate::tree::filetime_to_unix_ms(n.modified), kind, meta: None }
        })
        .collect()
}

fn dct_table() -> &'static [f32; 1024] {
    static T: OnceLock<[f32; 1024]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0f32; 1024];
        for u in 0..32 {
            for x in 0..32 {
                t[u * 32 + x] = (((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI) / 64.0).cos();
            }
        }
        t
    })
}

pub fn phash(m: &[f32]) -> u64 {
    let t = dct_table();
    let mut rows = [0f32; 8 * 32];
    for u in 0..8 {
        for y in 0..32 {
            let mut s = 0.0;
            for x in 0..32 {
                s += m[y * 32 + x] * t[u * 32 + x];
            }
            rows[u * 32 + y] = s;
        }
    }
    let mut c = [0f32; 64];
    for v in 0..8 {
        for u in 0..8 {
            let mut s = 0.0;
            for y in 0..32 {
                s += rows[u * 32 + y] * t[v * 32 + y];
            }
            c[v * 8 + u] = s;
        }
    }
    let mut sorted = c;
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let med = (sorted[31] + sorted[32]) / 2.0;
    let mut h = 0u64;
    for (i, v) in c.iter().enumerate() {
        if *v > med {
            h |= 1 << i;
        }
    }
    h
}

pub fn hashes4(m: &[f32; 1024]) -> [u64; 4] {
    let r90 = decode::orient(m, 32, 32, 6).0;
    let r180 = decode::orient(m, 32, 32, 3).0;
    let r270 = decode::orient(m, 32, 32, 8).0;
    [phash(m), phash(&r90), phash(&r180), phash(&r270)]
}

fn std_dev(m: &[f32]) -> f32 {
    let n = m.len() as f32;
    let mean = m.iter().sum::<f32>() / n;
    (m.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n).sqrt()
}

pub fn sharpness(g: &decode::Gray) -> f32 {
    if g.w < 12 || g.h < 12 {
        return 0.0;
    }
    const T: usize = 4;
    let mut tiles = Vec::with_capacity(T * T);
    for ty in 0..T {
        for tx in 0..T {
            let (x0, x1) = ((tx * g.w / T).max(1), ((tx + 1) * g.w / T).min(g.w - 1));
            let (y0, y1) = ((ty * g.h / T).max(1), ((ty + 1) * g.h / T).min(g.h - 1));
            let (mut sum, mut sq, mut n) = (0f64, 0f64, 0f64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let c = g.px[y * g.w + x] as f64;
                    let l = g.px[y * g.w + x - 1] as f64 + g.px[y * g.w + x + 1] as f64 + g.px[(y - 1) * g.w + x] as f64 + g.px[(y + 1) * g.w + x] as f64 - 4.0 * c;
                    sum += l;
                    sq += l * l;
                    n += 1.0;
                }
            }
            if n > 0.0 {
                let mean = sum / n;
                tiles.push((sq / n - mean * mean).max(0.0).sqrt() as f32);
            }
        }
    }
    tiles.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let top = tiles.len().min(3);
    tiles[..top].iter().sum::<f32>() / top.max(1) as f32
}

struct VideoSlot;

static SLOTS: (std::sync::Mutex<usize>, std::sync::Condvar) = (std::sync::Mutex::new(0), std::sync::Condvar::new());

impl VideoSlot {
    fn take() -> VideoSlot {
        let mut n = SLOTS.0.lock().unwrap();
        while *n >= 1 {
            n = SLOTS.1.wait(n).unwrap();
        }
        *n += 1;
        VideoSlot
    }
}

impl Drop for VideoSlot {
    fn drop(&mut self) {
        *SLOTS.0.lock().unwrap() -= 1;
        SLOTS.1.notify_one();
    }
}

pub fn analyze(item: &Item, cancel: &AtomicBool) -> Meta {
    match item.kind {
        Kind::Image => match decode::analyze_image(&item.path, 256) {
            Some(info) => {
                let t = decode::tiny(&info.gray.px, info.gray.w, info.gray.h, info.gray.w);
                Meta {
                    ok: true,
                    w: info.w,
                    h: info.h,
                    taken: info.taken,
                    exif: info.exif,
                    camera: info.camera,
                    phash: hashes4(&t),
                    detail: std_dev(&t),
                    sharp: sharpness(&info.gray),
                    ..Default::default()
                }
            }
            None => Meta::default(),
        },
        Kind::Video => match {
            let _slot = VideoSlot::take();
            decode::analyze_video(&item.path, cancel)
        } {
            Some(v) => Meta {
                ok: true,
                w: v.w,
                h: v.h,
                duration_ms: v.duration_ms,
                detail: v.frames.iter().map(|f| std_dev(f)).fold(0.0, f32::max),
                vhash: v.frames.iter().map(|f| phash(f)).collect(),
                ..Default::default()
            },
            None => Meta::default(),
        },
    }
}

#[derive(Default)]
pub struct Job {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
    pub hashing: hashing::Job,
    pub stage: AtomicU64,
}

fn analyze_all(work: &[(usize, Item)], cancel: &AtomicBool, mut progress: impl FnMut(usize), mut sink: impl FnMut(usize, Meta)) {
    let threads = (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 2).clamp(2, 4);
    let Ok(pool) = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .start_handler(|_| {
            decode::com_mta();
            crate::sys::background_thread();
        })
        .build()
    else {
        return;
    };
    let mut since_save = 0usize;
    for chunk in work.chunks(64) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let results: Vec<(usize, Meta)> = pool.install(|| {
            chunk
                .par_iter()
                .filter_map(|(i, it)| {
                    if cancel.load(Ordering::Relaxed) {
                        return None;
                    }
                    let m = analyze(it, cancel);
                    if cancel.load(Ordering::Relaxed) {
                        return None;
                    }
                    store().put(&it.path, it.size, it.modified, m.clone());
                    Some((*i, m))
                })
                .collect()
        });
        for (i, m) in results {
            sink(i, m);
        }
        progress(chunk.len());
        since_save += chunk.len();
        if since_save >= 1000 {
            store().save();
            since_save = 0;
        }
    }
    store().save();
}

pub fn child_main(list: &str) {
    decode::com_mta();
    let Ok(text) = std::fs::read_to_string(list) else { return };
    let work: Vec<(usize, Item)> = text
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let mut f = line.splitn(4, '\t');
            let kind = if f.next()? == "v" { Kind::Video } else { Kind::Image };
            let size = f.next()?.parse().ok()?;
            let modified = f.next()?.parse().ok()?;
            let path = f.next()?.to_string();
            let name = path.rsplit('\\').next().unwrap_or_default().to_string();
            Some((i, Item { id: i as u32, name, path, size, modified, modified_ms: 0, kind, meta: None }))
        })
        .collect();
    let cancel = AtomicBool::new(false);
    let mut out = std::io::stdout();
    analyze_all(&work, &cancel, |n| {
        use std::io::Write;
        let _ = writeln!(out, "{n}");
        let _ = out.flush();
    }, |_, _| {});
}

fn index_in_child(items: &mut [Item], todo: &[usize], job: &Job) -> bool {
    use std::io::BufRead;
    let Ok(exe) = std::env::current_exe() else { return false };
    let list = std::env::temp_dir().join(format!("PrismDisk-index-{}.txt", std::process::id()));
    let body: String = todo
        .iter()
        .map(|&i| {
            let it = &items[i];
            format!("{}\t{}\t{}\t{}\n", if it.kind == Kind::Video { "v" } else { "i" }, it.size, it.modified, it.path)
        })
        .collect();
    if std::fs::write(&list, body).is_err() {
        return false;
    }
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--media-index").arg(&list).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000 | 0x0000_4000);
    }
    let Ok(mut child) = cmd.spawn() else {
        let _ = std::fs::remove_file(&list);
        return false;
    };
    let stdout = child.stdout.take();
    let base = job.done.load(Ordering::Relaxed);
    let reader = stdout.map(|out| {
        let done = std::sync::Arc::new(AtomicU64::new(0));
        let d = done.clone();
        let h = std::thread::spawn(move || {
            for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                if let Ok(n) = line.trim().parse::<u64>() {
                    d.fetch_add(n, Ordering::Relaxed);
                }
            }
        });
        (done, h)
    });
    loop {
        if let Some((d, _)) = &reader {
            job.done.store(base + d.load(Ordering::Relaxed), Ordering::Relaxed);
        }
        if job.cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(120)),
            Err(_) => break,
        }
    }
    if let Some((_, h)) = reader {
        let _ = h.join();
    }
    let _ = std::fs::remove_file(&list);
    store().reload();
    for &i in todo {
        let it = &mut items[i];
        it.meta = Some(store().get(&it.path, it.size, it.modified).unwrap_or_default());
    }
    job.done.store(items.len() as u64, Ordering::Relaxed);
    true
}

pub fn index(items: &mut [Item], job: &Job) {
    job.total.store(items.len() as u64, Ordering::Relaxed);
    let todo: Vec<usize> = items
        .iter_mut()
        .enumerate()
        .filter_map(|(i, it)| match store().get(&it.path, it.size, it.modified) {
            Some(m) => {
                it.meta = Some(m);
                job.done.fetch_add(1, Ordering::Relaxed);
                None
            }
            None => Some(i),
        })
        .collect();
    if todo.is_empty() {
        return;
    }
    if !cfg!(test) && index_in_child(items, &todo, job) {
        return;
    }
    let work: Vec<(usize, Item)> = todo.iter().map(|&i| (i, items[i].clone())).collect();
    let mut found: Vec<(usize, Meta)> = Vec::new();
    analyze_all(&work, &job.cancel, |n| {
        job.done.fetch_add(n as u64, Ordering::Relaxed);
    }, |i, m| found.push((i, m)));
    for (i, m) in found {
        items[i].meta = Some(m);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Match {
    Exact,
    Same,
    Similar,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub kind: Match,
    pub media: Kind,
    pub items: Vec<usize>,
    pub keep: usize,
    pub wasted: u64,
}

pub const SAME_DIST: u32 = 6;
pub const SIMILAR_DIST: u32 = 10;

pub fn image_dist(a: &Meta, b: &Meta) -> u32 {
    b.phash.iter().map(|h| (a.phash[0] ^ h).count_ones()).min().unwrap_or(64)
}

pub fn video_dist(a: &Meta, b: &Meta) -> Option<u32> {
    let n = a.vhash.len().min(b.vhash.len());
    if n < 3 {
        return None;
    }
    let d: Vec<u32> = (0..n).map(|i| (a.vhash[i] ^ b.vhash[i]).count_ones()).collect();
    let worst = *d.iter().max().unwrap();
    let mean = d.iter().sum::<u32>() / n as u32;
    if worst > 20 {
        return None;
    }
    Some(mean)
}

fn durations_close(a: u32, b: u32) -> bool {
    let diff = a.abs_diff(b) as f64;
    diff <= 1500.0_f64.max(a.max(b) as f64 * 0.02)
}

fn bands(h: u64, k: usize) -> Vec<u16> {
    let mut out = Vec::with_capacity(k);
    let mut shift = 0;
    for i in 0..k {
        let bits = 64 / k + usize::from(i < 64 % k);
        out.push(((h >> shift) & ((1u64 << bits) - 1)) as u16);
        shift += bits;
    }
    out
}

fn image_pairs(items: &[Item], idx: &[usize]) -> Vec<(usize, usize)> {
    let radius = if idx.len() <= 40_000 { SIMILAR_DIST } else { SAME_DIST };
    let k = radius as usize + 1;
    let mut maps: Vec<HashMap<u16, Vec<usize>>> = (0..k).map(|_| HashMap::new()).collect();
    for &i in idx {
        let m = items[i].meta.as_ref().unwrap();
        for (b, v) in bands(m.phash[0], k).iter().enumerate() {
            maps[b].entry(*v).or_default().push(i);
        }
    }
    idx.par_iter()
        .flat_map_iter(|&i| {
            let m = items[i].meta.as_ref().unwrap();
            let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
            let mut out = Vec::new();
            for rot in 0..4 {
                for (b, v) in bands(m.phash[rot], k).iter().enumerate() {
                    let Some(bucket) = maps[b].get(v) else { continue };
                    if bucket.len() > 20_000 {
                        continue;
                    }
                    for &j in bucket {
                        if j == i || (rot == 0 && j < i) || seen.contains(&j) {
                            continue;
                        }
                        let o = items[j].meta.as_ref().unwrap();
                        if (m.phash[rot] ^ o.phash[0]).count_ones() <= radius {
                            seen.insert(j);
                            out.push((i, j));
                        }
                    }
                }
            }
            out
        })
        .collect()
}

fn video_pairs(items: &[Item], idx: &[usize]) -> Vec<(usize, usize)> {
    let mut v: Vec<usize> = idx.to_vec();
    v.sort_by_key(|&i| items[i].meta.as_ref().unwrap().duration_ms);
    let mut out = Vec::new();
    for a in 0..v.len() {
        let ma = items[v[a]].meta.as_ref().unwrap();
        for &bi in &v[a + 1..] {
            let mb = items[bi].meta.as_ref().unwrap();
            if !durations_close(ma.duration_ms, mb.duration_ms) {
                break;
            }
            if let Some(d) = video_dist(ma, mb) {
                if d <= SIMILAR_DIST {
                    out.push((v[a], bi));
                }
            }
        }
    }
    out
}

fn copy_like(name: &str) -> bool {
    let n = name.to_lowercase();
    let stem = n.rsplit_once('.').map(|(s, _)| s).unwrap_or(&n);
    stem.contains("copy") || stem.contains("kopie") || stem.contains("-wa") || stem.starts_with("received") || stem.ends_with(')') || stem.ends_with("-1") || stem.ends_with("_1")
}

fn quality(items: &[Item], a: usize, b: usize) -> std::cmp::Ordering {
    let (x, y) = (&items[a], &items[b]);
    let ex = x.meta.as_ref().map(|m| m.exif).unwrap_or(false);
    let ey = y.meta.as_ref().map(|m| m.exif).unwrap_or(false);
    x.pixels()
        .cmp(&y.pixels())
        .then(y.is_blurry().cmp(&x.is_blurry()))
        .then(ex.cmp(&ey))
        .then(x.size.cmp(&y.size))
        .then(copy_like(&y.name).cmp(&copy_like(&x.name)))
        .then(y.date_ms().cmp(&x.date_ms()))
        .then(y.path.len().cmp(&x.path.len()))
}

fn best_of(items: &[Item], members: &[usize]) -> usize {
    *members.iter().max_by(|&&a, &&b| quality(items, a, b)).unwrap()
}

fn aspect_ok(a: &Meta, b: &Meta) -> bool {
    if a.w == 0 || a.h == 0 || b.w == 0 || b.h == 0 {
        return true;
    }
    let ra = a.w as f32 / a.h as f32;
    let rb = b.w as f32 / b.h as f32;
    (ra / rb).ln().abs() < 0.12 || (ra * rb).ln().abs() < 0.12
}

fn pair_dist(items: &[Item], a: usize, b: usize) -> Option<u32> {
    let (x, y) = (&items[a], &items[b]);
    let (mx, my) = (x.meta.as_ref()?, y.meta.as_ref()?);
    if x.kind != y.kind {
        return None;
    }
    match x.kind {
        Kind::Image => {
            let d = image_dist(mx, my);
            let limit = if x.is_screenshot() || y.is_screenshot() { SAME_DIST } else { SIMILAR_DIST };
            (d <= limit && aspect_ok(mx, my)).then_some(d)
        }
        Kind::Video => video_dist(mx, my).filter(|d| *d <= SIMILAR_DIST),
    }
}

pub fn group(items: &[Item], job: &Job) -> Vec<Group> {
    let n = items.len();
    let refs: Vec<FileRef> = items.iter().enumerate().map(|(i, it)| FileRef { id: i as u32, path: it.path.clone(), size: it.size, modified: it.modified }).collect();
    job.stage.store(1, Ordering::Relaxed);
    let exact = hashing::exact_groups(refs, &job.hashing);
    if job.cancel.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let mut exact_of = vec![usize::MAX; n];
    let mut adj: Vec<Vec<(usize, u32)>> = vec![Vec::new(); n];
    for (gi, g) in exact.iter().enumerate() {
        for f in g {
            exact_of[f.id as usize] = gi;
        }
        for a in g {
            for b in g {
                if a.id != b.id {
                    adj[a.id as usize].push((b.id as usize, 0));
                }
            }
        }
    }
    job.stage.store(2, Ordering::Relaxed);
    let usable = |i: &usize| items[*i].meta.as_ref().map(|m| m.ok && m.detail >= if items[*i].kind == Kind::Video { 2.5 } else { 6.0 }).unwrap_or(false);
    let images: Vec<usize> = (0..n).filter(|i| items[*i].kind == Kind::Image).filter(usable).collect();
    let videos: Vec<usize> = (0..n).filter(|i| items[*i].kind == Kind::Video).filter(usable).collect();
    for (a, b) in image_pairs(items, &images).into_iter().chain(video_pairs(items, &videos)) {
        if exact_of[a] != usize::MAX && exact_of[a] == exact_of[b] {
            continue;
        }
        if let Some(d) = pair_dist(items, a, b) {
            adj[a].push((b, d));
            adj[b].push((a, d));
        }
    }
    let mut order: Vec<usize> = (0..n).filter(|&i| !adj[i].is_empty()).collect();
    order.sort_by(|&a, &b| quality(items, b, a));
    let mut taken = vec![false; n];
    let mut out: Vec<Group> = Vec::new();
    for c in order {
        if taken[c] {
            continue;
        }
        let generic = adj[c].len() > 40;
        let mut members = vec![c];
        let mut seen = std::collections::HashSet::new();
        for &(m, d) in &adj[c] {
            if generic && d > SAME_DIST.min(3) && exact_of[m] != exact_of[c] {
                continue;
            }
            if !taken[m] && m != c && seen.insert(m) {
                members.push(m);
            }
        }
        if members.len() < 2 {
            continue;
        }
        for &m in &members {
            taken[m] = true;
        }
        let keep = best_of(items, &members);
        let media = items[c].kind;
        let all_exact = exact_of[c] != usize::MAX && members.iter().all(|&i| exact_of[i] == exact_of[c]);
        let kind = if all_exact {
            Match::Exact
        } else {
            let km = items[keep].meta.as_ref();
            let close = members.iter().filter(|&&i| i != keep).all(|&i| {
                if exact_of[i] != usize::MAX && exact_of[i] == exact_of[keep] {
                    return true;
                }
                match (km, items[i].meta.as_ref()) {
                    (Some(a), Some(b)) if media == Kind::Image => image_dist(a, b) <= SAME_DIST,
                    (Some(a), Some(b)) => video_dist(a, b).map(|d| d <= SAME_DIST).unwrap_or(false),
                    _ => false,
                }
            });
            if close {
                Match::Same
            } else {
                Match::Similar
            }
        };
        let wasted = members.iter().filter(|&&i| i != keep).map(|&i| items[i].size).sum();
        members.sort_by_key(|&i| (i != keep, std::cmp::Reverse(items[i].pixels()), std::cmp::Reverse(items[i].size)));
        out.push(Group { kind, media, items: members, keep, wasted });
    }
    out.sort_by(|a, b| a.kind.cmp_rank().cmp(&b.kind.cmp_rank()).then(b.wasted.cmp(&a.wasted)));
    out
}

impl Match {
    fn cmp_rank(&self) -> u8 {
        match self {
            Match::Exact => 0,
            Match::Same => 1,
            Match::Similar => 2,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Match::Exact => "Exact copy",
            Match::Same => "Same, re-saved",
            Match::Similar => "Similar",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(w: usize, h: usize, seed: u32) -> Vec<u8> {
        let mut px = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let fx = x as f32 / w as f32;
                let fy = y as f32 / h as f32;
                let s = seed as f32;
                let v = 128.0 + 60.0 * ((fx * 7.0 + s).sin() * (fy * 5.0 - s * 0.7).cos()) + 50.0 * ((fx * fy * 13.0 + s * 1.3).sin());
                px[y * w + x] = v.clamp(0.0, 255.0) as u8;
            }
        }
        px
    }

    fn meta_of(px: &[u8], w: usize, h: usize) -> Meta {
        let t = decode::tiny(px, w, h, w);
        Meta { ok: true, phash: hashes4(&t), detail: std_dev(&t), ..Default::default() }
    }

    fn shrink(px: &[u8], w: usize, h: usize, f: usize) -> (Vec<u8>, usize, usize) {
        let (nw, nh) = (w / f, h / f);
        let mut out = vec![0u8; nw * nh];
        for y in 0..nh {
            for x in 0..nw {
                let mut s = 0u32;
                for dy in 0..f {
                    for dx in 0..f {
                        s += px[(y * f + dy) * w + x * f + dx] as u32;
                    }
                }
                out[y * nw + x] = (s / (f * f) as u32) as u8;
            }
        }
        (out, nw, nh)
    }

    #[test]
    fn perceptual_matching() {
        let (w, h) = (320, 240);
        let a = scene(w, h, 1);
        let ma = meta_of(&a, w, h);
        let (small, sw, sh) = shrink(&a, w, h, 2);
        let ms = meta_of(&small, sw, sh);
        let noisy: Vec<u8> = a.iter().enumerate().map(|(i, &v)| v.saturating_add(((i * 2654435761) % 9) as u8)).collect();
        let mn = meta_of(&noisy, w, h);
        let (rot, rw, rh) = decode::orient(&a, w, h, 6);
        let mr = meta_of(&rot, rw, rh);
        let b = scene(w, h, 7);
        let mb = meta_of(&b, w, h);
        assert!(image_dist(&ma, &ms) <= SAME_DIST, "resized {}", image_dist(&ma, &ms));
        assert!(image_dist(&ma, &mn) <= SAME_DIST, "noisy {}", image_dist(&ma, &mn));
        assert!(image_dist(&mr, &ma) <= SAME_DIST, "rotated {}", image_dist(&mr, &ma));
        assert!(image_dist(&ma, &mb) > SIMILAR_DIST, "different {}", image_dist(&ma, &mb));
    }

    #[test]
    #[ignore]
    fn real_files() {
        decode::com_mta();
        let dir = std::env::var("PD_MEDIA_DIR").expect("set PD_MEDIA_DIR");
        let mut items: Vec<Item> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .enumerate()
            .filter_map(|(i, e)| {
                let name = e.file_name().to_string_lossy().into_owned();
                let kind = kind_of(&name)?;
                let md = e.metadata().ok()?;
                Some(Item { id: i as u32, path: e.path().display().to_string(), name, size: md.len(), modified: i as u64 + 7, modified_ms: 0, kind, meta: None })
            })
            .collect();
        let cancel = AtomicBool::new(false);
        for it in items.iter_mut() {
            let t = std::time::Instant::now();
            let m = analyze(it, &cancel);
            println!("{:<34} ok={} {}x{} taken={} exif={} cam={:?} detail={:.1} sharp={:.1} dur={} frames={} in {:?}", it.name, m.ok, m.w, m.h, m.taken, m.exif, m.camera, m.detail, m.sharp, m.duration_ms, m.vhash.len(), t.elapsed());
            it.meta = Some(m);
        }
        for a in 0..items.len() {
            for b in a + 1..items.len() {
                if items[a].kind == Kind::Image && items[b].kind == Kind::Image {
                    println!("  {:>2}  {} <> {}", image_dist(items[a].meta.as_ref().unwrap(), items[b].meta.as_ref().unwrap()), items[a].name, items[b].name);
                }
            }
        }
        let vids: Vec<&Item> = items.iter().filter(|i| i.kind == Kind::Video).collect();
        for a in 0..vids.len() {
            for b in a + 1..vids.len() {
                let (x, y) = (vids[a].meta.as_ref().unwrap(), vids[b].meta.as_ref().unwrap());
                let d: Vec<u32> = x.vhash.iter().zip(&y.vhash).map(|(p, q)| (p ^ q).count_ones()).collect();
                println!("  video {:?} {} <> {}", d, vids[a].name, vids[b].name);
            }
        }
        for g in group(&items, &Job::default()) {
            println!("{:?} keep={} -> {:?}", g.kind, items[g.keep].name, g.items.iter().map(|&i| items[i].name.as_str()).collect::<Vec<_>>());
        }
    }

    #[test]
    #[ignore]
    fn bench_dir() {
        let dir = std::env::var("PD_BENCH_DIR").expect("set PD_BENCH_DIR");
        let mut stack = vec![std::path::PathBuf::from(dir)];
        let mut items = Vec::new();
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let Ok(md) = e.metadata() else { continue };
                if md.is_dir() {
                    if !SKIP_DIRS.contains(&e.file_name().to_string_lossy().to_lowercase().as_str()) {
                        stack.push(e.path());
                    }
                    continue;
                }
                let name = e.file_name().to_string_lossy().into_owned();
                let only = std::env::var("PD_BENCH_KIND").unwrap_or_default();
                if let Some(kind) = kind_of(&name).filter(|k| only.is_empty() || (only == "video") == (*k == Kind::Video)) {
                    items.push(Item { id: items.len() as u32, path: e.path().display().to_string(), name, size: md.len(), modified: 1, modified_ms: 0, kind, meta: None });
                }
            }
        }
        let job = Job::default();
        let t = std::time::Instant::now();
        index(&mut items, &job);
        let t_index = t.elapsed();
        let imgs: Vec<usize> = (0..items.len()).filter(|&i| items[i].kind == Kind::Image && items[i].meta.as_ref().map(|m| m.ok && m.detail >= 6.0).unwrap_or(false)).collect();
        let t = std::time::Instant::now();
        let pairs = image_pairs(&items, &imgs);
        println!("image pairs={} from {} images in {:?}", pairs.len(), imgs.len(), t.elapsed());
        drop(pairs);
        let t = std::time::Instant::now();
        let groups = group(&items, &job);
        let t_group = t.elapsed();
        let ok = items.iter().filter(|i| i.meta.as_ref().map(|m| m.ok).unwrap_or(false)).count();
        let dated = items.iter().filter(|i| i.meta.as_ref().map(|m| m.taken > 0).unwrap_or(false)).count();
        let blurry = items.iter().filter(|i| i.is_blurry()).count();
        let sharp: Vec<f32> = items.iter().filter_map(|i| i.meta.as_ref().filter(|m| m.ok && i.kind == Kind::Image).map(|m| m.sharp)).collect();
        let mut s = sharp.clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let pct = |p: f32| s.get(((s.len() as f32 - 1.0) * p) as usize).copied().unwrap_or(0.0);
        println!("items={} decoded={} with_date={} blurry={} index={:?} group={:?}", items.len(), ok, dated, blurry, t_index, t_group);
        println!("sharpness p5={:.1} p25={:.1} p50={:.1} p75={:.1}", pct(0.05), pct(0.25), pct(0.5), pct(0.75));
        for k in [Match::Exact, Match::Same, Match::Similar] {
            let g: Vec<&Group> = groups.iter().filter(|g| g.kind == k).collect();
            let largest = g.iter().map(|g| g.items.len()).max().unwrap_or(0);
            println!("{:?}: {} groups, {} files, largest {}, {} bytes reclaimable", k, g.len(), g.iter().map(|g| g.items.len()).sum::<usize>(), largest, g.iter().map(|g| g.wasted).sum::<u64>());
        }
        println!("peak private {} MB", crate::sys::peak_private_mb());
    }

    #[test]
    #[ignore]
    fn personal_scope_report() {
        let drive = std::env::var("PD_SCOPE_DRIVE").unwrap_or_else(|_| "C:\\".into());
        let ctl = crate::scan::ScanCtl::default();
        let (t, _) = crate::backend::run_scan("drive", &drive, &ctl).expect("scan");
        println!("clip app folders: {:?}", clip_app_folders());
        for scope in [Scope::Personal, Scope::All] {
            let items = collect(&t, t.root, scope);
            let mut by: HashMap<String, usize> = HashMap::new();
            for it in &items {
                *by.entry(it.folder().to_string()).or_default() += 1;
            }
            let mut v: Vec<(String, usize)> = by.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            println!("{:?}: {} files in {} folders", scope, items.len(), v.len());
            if scope == Scope::Personal {
                for (f, n) in v.iter().take(40) {
                    println!("  {n:>6}  {f}");
                }
            }
        }
    }

    #[test]
    fn clip_app_settings() {
        let medal = r#"{"theme":"dark","clipFolder":"D:\\Clips\\Medal","clipSoundPath":"default"}"#;
        assert_eq!(json_strings(medal, "clipFolder"), vec![r"D:\Clips\Medal".to_string()]);
        let mut db = b"SQLite format 3\0\0junk".to_vec();
        db.extend_from_slice(br"E:\Game Clips\Moments\Battlefield 6__2025-10-31.mp4");
        db.extend_from_slice(b"\0\x05");
        db.extend_from_slice(br"E:\\Game Clips\\Moments\\thumbs\\Battlefield 6__2025-10-31.png.mp4");
        let folders = moments_folders(&db);
        assert_eq!(folders, vec![r"E:\Game Clips\Moments".to_string()]);
    }

    #[test]
    fn personal_names() {
        for n in ["IMG_20240612_181502.jpg", "IMG-20240613-WA0007.jpg", "VID_20230101_120000.mp4", "PXL_20250101_101010123.jpg", "DSC01234.JPG", "Screenshot 2026-11-18 081502.png", "20240612_181502.jpg", "GOPR0042.MP4", "signal-2024-01-01-120000.jpg", "IMG1234.HEIC"] {
            assert!(camera_name(n), "{n}");
        }
        for n in ["icon.png", "texture_diffuse.png", "hero-character.mov", "logo@2x.png", "background.jpg", "image.png", "splash_screen.png"] {
            assert!(!camera_name(n), "{n}");
        }
        assert!(media_folder(r"c:\users\a\onedrive\pictures\camera roll"));
        assert!(!media_folder(r"c:\games\thing\textures"));
        let roots = vec![r"c:\users\a\pictures".to_string()];
        assert!(in_roots(r"c:\users\a\pictures\2024", &roots));
        assert!(!in_roots(r"c:\users\a\pictures2", &roots));
    }

    #[test]
    fn record_roundtrip() {
        let m = Meta { ok: true, w: 4000, h: 3000, taken: 1_700_000_000_000, exif: true, camera: "Pixel 9 Pro".into(), phash: [1, 2, 3, 4], detail: 20.5, sharp: 33.0, duration_ms: 12345, vhash: vec![9, 8, 7] };
        let mut b = Vec::new();
        m.write(&mut b);
        assert!(b.len() <= Meta::LEN);
        let r = Meta::read(&b);
        assert_eq!((r.w, r.h, r.taken, r.camera.as_str(), r.phash, r.duration_ms, r.vhash.clone()), (4000, 3000, 1_700_000_000_000, "Pixel 9 Pro", [1, 2, 3, 4], 12345, vec![9, 8, 7]));
    }
}
