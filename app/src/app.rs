use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use eframe::egui::{self, Color32, Pos2};

use crate::backend::{self, DeleteResult, ScanInfo};
use crate::fsinfo::{self, DriveInfo};
use crate::scan::ScanCtl;
use crate::theme::{self, hex, hsl};
use crate::tree::{self, Mode, Row, TmRect, Tree, CLEAN_RULES};
use crate::gallery::MediaState;
use crate::hashing;
use crate::media;
use crate::thumbs::Thumbs;
use crate::update::{self, Release};
use crate::{sys, viz};
use std::sync::atomic::AtomicU64;

pub type Shared = Arc<RwLock<Option<Tree>>>;

pub fn fmt_bytes(n: u64) -> String {
    const U: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if n == 0 {
        return "0 B".into();
    }
    let (mut v, mut i) = (n as f64, 0);
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else if v >= 100.0 {
        format!("{v:.0} {}", U[i])
    } else if v >= 10.0 {
        format!("{v:.1} {}", U[i])
    } else {
        format!("{v:.2} {}", U[i])
    }
}

pub fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn fmt_pct(p: f64) -> String {
    if !p.is_finite() || p <= 0.0 {
        "0%".into()
    } else if p >= 99.95 {
        "100%".into()
    } else if p < 10.0 {
        format!("{p:.1}%")
    } else {
        format!("{p:.0}%")
    }
}

pub fn fmt_date(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
        use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
        let ft = ((ms as u64) + 11_644_473_600_000) * 10_000;
        let ft = FILETIME { dwLowDateTime: ft as u32, dwHighDateTime: (ft >> 32) as u32 };
        let mut utc = SYSTEMTIME::default();
        let mut local = SYSTEMTIME::default();
        if FileTimeToSystemTime(&ft, &mut utc).is_ok() && SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_ok() {
            return format!("{:04}-{:02}-{:02} {:02}:{:02}", local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute);
        }
    }
    String::new()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Tree,
    Search,
    Dups,
    Clean,
    Media,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Vis {
    Treemap,
    Sunburst,
}

#[derive(Clone)]
pub struct VisRow {
    pub row: Row,
    pub depth: u16,
    pub pv: u64,
}

#[derive(Clone)]
pub struct NodeDetail {
    pub row: Row,
    pub crumbs: Vec<(u32, String)>,
    pub parent_value: u64,
}

#[derive(Clone)]
pub struct ExtStat {
    pub id: u16,
    pub alloc: u64,
    pub size: u64,
    pub count: u64,
}

#[derive(Clone)]
pub struct DupGroup {
    pub size: u64,
    pub wasted: u64,
    pub items: Vec<Row>,
}

#[derive(Clone)]
pub struct CleanItem {
    pub row: Row,
    pub category: &'static str,
    pub label: &'static str,
    pub note: &'static str,
}

#[derive(Clone)]
pub struct SunArc {
    pub id: u32,
    pub d: u8,
    pub a0: f32,
    pub a1: f32,
    pub ext: u16,
    pub dir: bool,
    pub name: String,
    pub value: u64,
}

pub struct TmLayout {
    pub key: (u32, Mode, u32, u32),
    pub rects: Vec<TmRect>,
    pub labels: Vec<(usize, String)>,
}

pub enum Msg {
    Scanned(Result<ScanInfo, String>),
    Find(u64, Vec<Row>),
    Dups(u64, Vec<DupGroup>),
    Clean(u64, Vec<CleanItem>),
    CleanPill(u64),
    Types(u64, Vec<ExtStat>),
    Treemap(u64, TmLayout),
    Sun(u64, Vec<SunArc>),
    Deleted(DeleteResult, DeleteKind),
    Exported(Result<String, String>),
    Picked(Pick, Option<PathBuf>),
    UpdateChecked(Result<Option<Release>, String>, bool),
    UpdateDownloaded(Release, Result<PathBuf, String>),
    MediaList(u64, Vec<media::Item>),
    MediaIndexed(u64, Vec<media::Item>, Vec<media::Group>, bool),
}

#[derive(Clone)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Downloading(Release, Arc<AtomicU64>),
    Ready(Release, PathBuf),
    Failed(String, Option<Release>),
}

#[derive(Clone, Copy, PartialEq)]
pub enum DeleteKind {
    Recycle,
    Permanent,
    Clean,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Pick {
    Folder,
    Csv,
    Export(u32),
}

pub struct Toast {
    pub title: String,
    pub msg: String,
    pub err: bool,
    pub born: Instant,
}

#[derive(Clone)]
pub enum MenuKind {
    Node(Box<NodeDetail>),
    MinSize,
    Age,
    DupMin,
}

pub struct Menu {
    pub pos: Pos2,
    pub kind: MenuKind,
    pub opened: f64,
}

pub enum ModalAction {
    Delete(Vec<u32>, bool),
    Clean(Vec<u32>),
}

pub struct Modal {
    pub title: String,
    pub body: String,
    pub warn: Option<String>,
    pub list: Vec<(String, u64)>,
    pub ok: String,
    pub danger: bool,
    pub ack: Option<(String, bool)>,
    pub lock: bool,
    pub action: ModalAction,
}

pub struct ScanState {
    pub ctl: Arc<ScanCtl>,
    pub title: String,
    pub started: Instant,
}

pub const MIN_SIZES: [(u64, &str); 4] = [(0, "Any size"), (1 << 20, "≥ 1 MB"), (100 << 20, "≥ 100 MB"), (1 << 30, "≥ 1 GB")];
pub const AGES: [(u32, &str); 5] = [(0, "Any age"), (30, "Older than 30 days"), (180, "Older than 6 months"), (365, "Older than 1 year"), (1095, "Older than 3 years")];
pub const DUP_MINS: [(u64, &str); 4] = [(100 << 10, "≥ 100 KB"), (1 << 20, "≥ 1 MB"), (10 << 20, "≥ 10 MB"), (100 << 20, "≥ 100 MB")];
pub const PALETTE: [u32; 16] = [
    0x6366f1, 0x0ea5e9, 0x10b981, 0xf59e0b, 0xa855f7, 0xec4899, 0x14b8a6, 0xf97316, 0x3b82f6, 0x84cc16, 0xef4444, 0x06b6d4, 0x8b5cf6, 0xeab308,
    0x22c55e, 0xf43f5e,
];
pub const DIR_FILL: Color32 = hex(0x2a3346);

pub struct App {
    pub shared: Shared,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub ctx: egui::Context,

    pub admin: bool,
    pub banner: bool,
    pub drives: Vec<DriveInfo>,
    pub home: bool,
    pub info: Option<ScanInfo>,
    pub last_scan: Option<(String, String)>,
    pub scan: Option<ScanState>,
    pub mode: Mode,

    pub root: u32,
    pub root_row: Option<Row>,
    pub focus: u32,
    pub focus_detail: Option<NodeDetail>,
    pub sel: Option<u32>,
    pub kids: HashMap<u32, Vec<Row>>,
    pub expanded: HashSet<u32>,
    pub visible: Vec<VisRow>,
    pub vis_index: HashMap<u32, usize>,
    pub sort_key: &'static str,
    pub sort_desc: bool,
    pub scroll_to: Option<usize>,
    pub node_cache: HashMap<u32, NodeDetail>,

    pub ext_names: Vec<String>,
    pub ext_colors: Vec<Color32>,
    pub ext_filter: Option<u16>,
    pub types: Vec<ExtStat>,

    pub tab: Tab,
    pub q: String,
    pub q_kind: usize,
    pub q_min: usize,
    pub q_age: usize,
    pub q_dirty_at: Option<Instant>,
    pub find: Vec<Row>,
    pub find_status: String,
    pub dups: Option<Vec<DupGroup>>,
    pub dup_min: usize,
    pub dup_content: bool,
    pub dup_job: Option<Arc<hashing::Job>>,
    pub media: MediaState,
    pub thumbs: Thumbs,
    pub clean: Option<Vec<CleanItem>>,
    pub clean_sel: HashSet<u32>,
    pub clean_pill: u64,
    pub stale_search: bool,
    pub stale_dups: bool,
    pub stale_clean: bool,
    pub focus_search: bool,

    pub vis: Vis,
    pub vis_h: f32,
    pub tm: Option<TmLayout>,
    pub tm_mesh: Option<(viz::MeshKey, Arc<egui::Mesh>)>,
    pub tm_want: Option<((u32, Mode, u32, u32), Instant)>,
    pub tm_in_flight: Option<(u32, Mode, u32, u32)>,
    pub tm_hover: Option<usize>,
    pub sun: Vec<SunArc>,
    pub sun_key: Option<(u32, Mode)>,
    pub sun_hover: Option<usize>,
    pub vis_info: Option<String>,

    pub menu: Option<Menu>,
    pub modal: Option<Modal>,
    pub toasts: Vec<Toast>,
    pub update: UpdateState,
    pub update_banner: bool,
    pub about: bool,
    pub last_version: String,

    seq: u64,
    pub seq_find: u64,
    pub seq_dups: u64,
    pub seq_clean: u64,
    pub seq_types: u64,
    pub seq_tm: u64,
    pub seq_sun: u64,
}

fn settings_path() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("PrismDisk").join("settings.ini")
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        theme::style(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let (tx, rx) = channel();
        let mut app = App {
            shared: Arc::new(RwLock::new(None)),
            tx,
            rx,
            ctx: cc.egui_ctx.clone(),
            admin: sys::is_admin(),
            banner: !sys::is_admin(),
            drives: fsinfo::enumerate(),
            home: true,
            info: None,
            last_scan: None,
            scan: None,
            mode: Mode::Alloc,
            root: 0,
            root_row: None,
            focus: 0,
            focus_detail: None,
            sel: None,
            kids: HashMap::new(),
            expanded: HashSet::new(),
            visible: Vec::new(),
            vis_index: HashMap::new(),
            sort_key: "alloc",
            sort_desc: true,
            scroll_to: None,
            node_cache: HashMap::new(),
            ext_names: Vec::new(),
            ext_colors: Vec::new(),
            ext_filter: None,
            types: Vec::new(),
            tab: Tab::Tree,
            q: String::new(),
            q_kind: 0,
            q_min: 0,
            q_age: 0,
            q_dirty_at: None,
            find: Vec::new(),
            find_status: String::new(),
            dups: None,
            dup_min: 1,
            dup_content: true,
            dup_job: None,
            media: MediaState::default(),
            thumbs: Thumbs::new(&cc.egui_ctx),
            clean: None,
            clean_sel: HashSet::new(),
            clean_pill: 0,
            stale_search: true,
            stale_dups: true,
            stale_clean: true,
            focus_search: false,
            vis: Vis::Treemap,
            vis_h: 0.0,
            tm: None,
            tm_mesh: None,
            tm_want: None,
            tm_in_flight: None,
            tm_hover: None,
            sun: Vec::new(),
            sun_key: None,
            sun_hover: None,
            vis_info: None,
            menu: None,
            modal: None,
            toasts: Vec::new(),
            update: UpdateState::Idle,
            update_banner: true,
            about: false,
            last_version: String::new(),
            seq: 0,
            seq_find: 0,
            seq_dups: 0,
            seq_clean: 0,
            seq_types: 0,
            seq_tm: 0,
            seq_sun: 0,
        };
        app.load_settings();
        let current = update::current_version();
        if !app.last_version.is_empty() && app.last_version != current {
            app.about = true;
        }
        if app.last_version != current {
            app.last_version = current;
            app.save_settings();
        }
        app.check_updates(false);

        let target = std::env::args().skip(1).find(|a| !a.starts_with("--")).map(|a| a.trim_matches('"').to_string());
        if let Some(t) = target {
            let lower = t.to_lowercase();
            if lower.ends_with(".csv") {
                app.start_scan("csv", &t);
            } else {
                let t = if t.len() == 2 && t.ends_with(':') { format!("{t}\\") } else { t };
                let kind = if t.trim_end_matches('\\').len() == 2 { "drive" } else { "folder" };
                app.start_scan(kind, &t);
            }
        }
        app
    }

    fn load_settings(&mut self) {
        let Ok(text) = std::fs::read_to_string(settings_path()) else { return };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k.trim() {
                "mode" => self.mode = if v.trim() == "size" { Mode::Size } else { Mode::Alloc },
                "vis" => self.vis = if v.trim() == "sunburst" { Vis::Sunburst } else { Vis::Treemap },
                "vis_h" => self.vis_h = v.trim().parse().unwrap_or(0.0),
                "last_version" => self.last_version = v.trim().to_string(),
                _ => {}
            }
        }
    }

    pub fn save_settings(&self) {
        let p = settings_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(
            p,
            format!(
                "mode={}\nvis={}\nvis_h={}\nlast_version={}\n",
                if self.mode == Mode::Size { "size" } else { "alloc" },
                if self.vis == Vis::Sunburst { "sunburst" } else { "treemap" },
                self.vis_h,
                self.last_version
            ),
        );
    }

    pub fn val(&self, r: &Row) -> u64 {
        if self.mode == Mode::Alloc { r.alloc } else { r.size }
    }

    pub fn ext_color(&self, ext: u16) -> Color32 {
        self.ext_colors.get(ext as usize).copied().unwrap_or(theme::DIM)
    }

    pub fn toast(&mut self, title: impl Into<String>, msg: impl Into<String>, err: bool) {
        self.toasts.push(Toast { title: title.into(), msg: msg.into(), err, born: Instant::now() });
        self.ctx.request_repaint();
    }

    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    pub fn next_seq_pub(&mut self) -> u64 {
        self.next_seq()
    }

    pub fn sender(&self) -> Sender<Msg> {
        self.tx.clone()
    }

    fn spawn<F: FnOnce(&Shared) -> Msg + Send + 'static>(&self, f: F) {
        let shared = self.shared.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let m = f(&shared);
            let _ = tx.send(m);
            ctx.request_repaint();
        });
    }

    pub fn start_scan(&mut self, kind: &str, target: &str) {
        if self.scan.is_some() || target.is_empty() {
            return;
        }
        self.menu = None;
        self.last_scan = Some((kind.to_string(), target.to_string()));
        let ctl = Arc::new(ScanCtl::default());
        ctl.reset();
        let title = match kind {
            "csv" => "Importing CSV".to_string(),
            "drive" => format!("Scanning {}", &target[..2.min(target.len())]),
            _ => "Scanning folder".to_string(),
        };
        if kind == "csv" {
            *ctl.current.lock().unwrap() = target.to_string();
        }
        self.scan = Some(ScanState { ctl: ctl.clone(), title, started: Instant::now() });
        let (kind, target) = (kind.to_string(), target.to_string());
        self.spawn(move |shared| match backend::run_scan(&kind, &target, &ctl) {
            Ok((tree, info)) => {
                *shared.write().unwrap() = Some(tree);
                Msg::Scanned(Ok(info))
            }
            Err(e) => Msg::Scanned(Err(e)),
        });
    }

    pub fn cancel_scan(&self) {
        if let Some(s) = &self.scan {
            s.ctl.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn on_scanned(&mut self, info: ScanInfo) {
        let (root, exts, ext_stats) = {
            let g = self.shared.read().unwrap();
            let t = g.as_ref().unwrap();
            (t.root, t.exts.clone(), t.ext_stats(t.root))
        };
        self.root = root;
        self.focus = root;
        self.sel = Some(root);
        self.kids.clear();
        self.node_cache.clear();
        self.expanded = HashSet::from([root]);
        self.ext_filter = None;
        self.clean_sel.clear();
        self.clean = None;
        self.dups = None;
        self.media.reset();
        self.thumbs.clear();
        self.find.clear();
        self.stale_search = true;
        self.stale_dups = true;
        self.stale_clean = true;
        self.tm = None;
        self.tm_mesh = None;
        self.sun.clear();
        self.sun_key = None;

        self.ext_colors = exts.iter().map(|n| if n.is_empty() { theme::DIM } else { hsl(hash_hue(n), 32.0, 46.0) }).collect();
        let mut i = 0;
        for (id, ..) in &ext_stats {
            if *id == 0 {
                continue;
            }
            if i >= PALETTE.len() {
                break;
            }
            self.ext_colors[*id as usize] = hex(PALETTE[i]);
            i += 1;
        }
        self.ext_names = exts;
        if let Some(note) = &info.note {
            self.toast("Heads up", note.clone(), true);
        }
        self.info = Some(info);
        self.home = false;
        self.drives = fsinfo::enumerate();
        self.reload_tree();
        self.after_focus_change();
        self.select(root, false);
        self.load_clean_pill();
    }

    pub fn node(&mut self, id: u32) -> Option<NodeDetail> {
        if let Some(d) = self.node_cache.get(&id) {
            return Some(d.clone());
        }
        let d = {
            let g = self.shared.read().unwrap();
            let t = g.as_ref()?;
            if !t.valid(id) {
                return None;
            }
            let crumbs = t.ancestors(id).into_iter().map(|a| (a, t.name(a).to_string())).collect();
            let parent = t.nodes[id as usize].parent;
            let parent_value = if id == t.root || parent == tree::NONE { t.value(id, self.mode) } else { t.value(parent, self.mode) };
            NodeDetail { row: t.row(id, true), crumbs, parent_value }
        };
        self.node_cache.insert(id, d.clone());
        Some(d)
    }

    pub fn load_kids(&mut self, id: u32) -> bool {
        if self.kids.contains_key(&id) {
            return true;
        }
        let key = match self.sort_key {
            "pct" => {
                if self.mode == Mode::Alloc { "alloc" } else { "size" }
            }
            k => k,
        };
        let rows = {
            let g = self.shared.read().unwrap();
            let Some(t) = g.as_ref() else { return false };
            if !t.valid(id) {
                return false;
            }
            t.child_rows(id, key, self.sort_desc)
        };
        self.kids.insert(id, rows);
        true
    }

    pub fn reload_tree(&mut self) {
        self.kids.clear();
        self.node_cache.clear();
        self.root_row = self.node(self.root).map(|d| d.row);
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            if !self.expanded.contains(&id) {
                continue;
            }
            if !self.load_kids(id) {
                self.expanded.remove(&id);
                continue;
            }
            for r in &self.kids[&id] {
                if r.dir && self.expanded.contains(&r.id) {
                    stack.push(r.id);
                }
            }
        }
        self.rebuild_visible();
    }

    pub fn rebuild_visible(&mut self) {
        let mut out = Vec::new();
        if let Some(rr) = self.root_row.clone() {
            let pv = self.val(&rr);
            out.push(VisRow { row: rr.clone(), depth: 0, pv });
            fn walk(app: &App, row: &Row, depth: u16, out: &mut Vec<VisRow>) {
                if !app.expanded.contains(&row.id) {
                    return;
                }
                let Some(kids) = app.kids.get(&row.id) else { return };
                let pv = app.val(row);
                for k in kids {
                    out.push(VisRow { row: k.clone(), depth, pv });
                    if k.dir {
                        walk(app, k, depth + 1, out);
                    }
                }
            }
            walk(self, &rr, 1, &mut out);
        }
        self.vis_index = out.iter().enumerate().map(|(i, v)| (v.row.id, i)).collect();
        self.visible = out;
    }

    pub fn toggle(&mut self, id: u32, force: Option<bool>) {
        let open = self.expanded.contains(&id);
        if force == Some(open) {
            return;
        }
        if open {
            self.expanded.remove(&id);
        } else if self.load_kids(id) {
            self.expanded.insert(id);
        }
        self.rebuild_visible();
    }

    pub fn select(&mut self, id: u32, scroll: bool) {
        self.sel = Some(id);
        if scroll {
            if let Some(&i) = self.vis_index.get(&id) {
                self.scroll_to = Some(i);
            }
        }
    }

    pub fn reveal_in_tree(&mut self, id: u32) {
        self.tab = Tab::Tree;
        let Some(d) = self.node(id) else { return };
        for (cid, _) in d.crumbs.iter().take(d.crumbs.len().saturating_sub(1)) {
            if !self.expanded.contains(cid) && self.load_kids(*cid) {
                self.expanded.insert(*cid);
            }
        }
        self.rebuild_visible();
        self.select(id, true);
    }

    pub fn set_sort(&mut self, key: &'static str) {
        let cur_value_key = if self.mode == Mode::Alloc { "alloc" } else { "size" };
        if self.sort_key == key || (key == "pct" && self.sort_key == cur_value_key) {
            self.sort_desc = !self.sort_desc;
        } else {
            self.sort_key = if key == "pct" { cur_value_key } else { key };
            self.sort_desc = key != "name";
        }
        self.reload_tree();
    }

    pub fn set_focus(&mut self, id: u32) {
        self.focus = id;
        self.media.reset();
        self.stale_search = true;
        self.stale_dups = true;
        self.stale_clean = true;
        self.after_focus_change();
    }

    pub fn after_focus_change(&mut self) {
        let d = match self.node(self.focus) {
            Some(d) => d,
            None => {
                self.focus = self.root;
                match self.node(self.root) {
                    Some(d) => d,
                    None => return,
                }
            }
        };
        self.focus_detail = Some(d);
        self.tm_hover = None;
        self.sun_hover = None;
        self.vis_info = None;
        self.load_types();
        match self.tab {
            Tab::Search => self.run_find(),
            Tab::Dups => self.load_dups(),
            Tab::Clean => self.load_clean(),
            Tab::Tree | Tab::Media => {}
        }
    }

    pub fn focus_up(&mut self) {
        if self.focus == self.root {
            return;
        }
        if let Some(d) = self.node(self.focus) {
            let parent = if d.crumbs.len() > 1 { d.crumbs[d.crumbs.len() - 2].0 } else { self.root };
            self.set_focus(parent);
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if mode == self.mode {
            return;
        }
        self.mode = mode;
        self.save_settings();
        if self.info.is_none() {
            return;
        }
        if matches!(self.sort_key, "alloc" | "size") {
            self.sort_key = if mode == Mode::Alloc { "alloc" } else { "size" };
        }
        self.reload_tree();
        self.stale_search = true;
        self.stale_dups = true;
        self.stale_clean = true;
        self.after_focus_change();
    }

    pub fn switch_tab(&mut self, tab: Tab) {
        if self.tab == Tab::Media && tab != Tab::Media {
            self.thumbs.clear();
            self.media.lightbox = None;
        }
        self.tab = tab;
        match tab {
            Tab::Search => {
                if self.stale_search {
                    self.run_find();
                }
                self.focus_search = true;
            }
            Tab::Dups if self.stale_dups => self.load_dups(),
            Tab::Clean if self.stale_clean => self.load_clean(),
            _ => {}
        }
    }

    pub fn load_types(&mut self) {
        let seq = self.next_seq();
        self.seq_types = seq;
        let id = self.focus;
        self.spawn(move |shared| {
            let g = shared.read().unwrap();
            let stats = g
                .as_ref()
                .filter(|t| t.valid(id))
                .map(|t| t.ext_stats(id).into_iter().map(|(id, alloc, size, count)| ExtStat { id, alloc, size, count }).collect())
                .unwrap_or_default();
            Msg::Types(seq, stats)
        });
    }

    pub fn run_find(&mut self) {
        self.stale_search = false;
        self.q_dirty_at = None;
        let seq = self.next_seq();
        self.seq_find = seq;
        self.find_status = "Searching…".into();
        let (id, pattern, mode) = (self.focus, self.q.clone(), self.mode);
        let kind = ["files", "folders", "all"][self.q_kind];
        let min = MIN_SIZES[self.q_min].0;
        let cutoff = backend::age_cutoff(AGES[self.q_age].0);
        self.spawn(move |shared| {
            let g = shared.read().unwrap();
            let rows = g.as_ref().filter(|t| t.valid(id)).map(|t| t.find(id, &pattern, kind, min, cutoff, 5000, mode)).unwrap_or_default();
            Msg::Find(seq, rows)
        });
    }

    pub fn load_dups(&mut self) {
        self.stale_dups = false;
        self.dups = None;
        if let Some(j) = self.dup_job.take() {
            j.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let seq = self.next_seq();
        self.seq_dups = seq;
        let (id, min) = (self.focus, DUP_MINS[self.dup_min].0);
        if !self.dup_content {
            self.spawn(move |shared| {
                let g = shared.read().unwrap();
                let groups = g
                    .as_ref()
                    .filter(|t| t.valid(id))
                    .map(|t| {
                        t.duplicates(id, min, 300)
                            .into_iter()
                            .map(|(size, ids)| DupGroup {
                                size,
                                wasted: size * (ids.len() as u64 - 1),
                                items: ids.into_iter().map(|i| t.row(i, true)).collect(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Msg::Dups(seq, groups)
            });
            return;
        }
        let job = Arc::new(hashing::Job::default());
        self.dup_job = Some(job.clone());
        self.spawn(move |shared| {
            let files = {
                let g = shared.read().unwrap();
                g.as_ref().filter(|t| t.valid(id)).map(|t| t.content_candidates(id, min)).unwrap_or_default()
            };
            let found = hashing::exact_groups(files, &job);
            if job.cancelled() {
                return Msg::Dups(0, Vec::new());
            }
            let g = shared.read().unwrap();
            let Some(t) = g.as_ref() else { return Msg::Dups(seq, Vec::new()) };
            let mut groups: Vec<DupGroup> = found
                .into_iter()
                .map(|grp| {
                    let size = grp[0].size;
                    let mut items: Vec<Row> = grp.iter().filter(|f| t.valid(f.id)).map(|f| t.row(f.id, true)).collect();
                    items.sort_by_key(|r| r.modified);
                    DupGroup { size, wasted: size * (items.len().max(1) as u64 - 1), items }
                })
                .filter(|g| g.items.len() > 1)
                .collect();
            groups.sort_unstable_by(|a, b| b.wasted.cmp(&a.wasted));
            groups.truncate(500);
            Msg::Dups(seq, groups)
        });
    }

    fn clean_items(t: &Tree, id: u32) -> Vec<CleanItem> {
        t.cleanup_candidates(id)
            .into_iter()
            .map(|(nid, r)| CleanItem { row: t.row(nid, true), category: r.category, label: r.label, note: r.note })
            .collect()
    }

    pub fn load_clean(&mut self) {
        self.stale_clean = false;
        let seq = self.next_seq();
        self.seq_clean = seq;
        let id = self.focus;
        self.spawn(move |shared| {
            let g = shared.read().unwrap();
            let items = g.as_ref().filter(|t| t.valid(id)).map(|t| Self::clean_items(t, id)).unwrap_or_default();
            Msg::Clean(seq, items)
        });
    }

    pub fn load_clean_pill(&mut self) {
        let seq = self.next_seq();
        let id = self.root;
        let _ = CLEAN_RULES.len();
        self.spawn(move |shared| {
            let g = shared.read().unwrap();
            let total = g.as_ref().map(|t| Self::clean_items(t, id).iter().map(|c| c.row.alloc).sum()).unwrap_or(0);
            let _ = seq;
            Msg::CleanPill(total)
        });
    }

    pub fn request_treemap(&mut self, w: f32, h: f32) {
        let key = (self.focus, self.mode, w.round() as u32, h.round() as u32);
        if self.tm.as_ref().map(|t| t.key) == Some(key) || self.tm_in_flight == Some(key) {
            return;
        }
        let same_size = self.tm.as_ref().map(|t| (t.key.2, t.key.3)) == Some((key.2, key.3));
        if !same_size && self.tm.is_some() {
            match self.tm_want {
                Some((k, since)) if k == key => {
                    if since.elapsed().as_millis() < 120 {
                        self.ctx.request_repaint_after(std::time::Duration::from_millis(40));
                        return;
                    }
                }
                _ => {
                    self.tm_want = Some((key, Instant::now()));
                    self.ctx.request_repaint_after(std::time::Duration::from_millis(130));
                    return;
                }
            }
        }
        self.tm_want = None;
        self.tm_in_flight = Some(key);
        let seq = self.next_seq();
        self.seq_tm = seq;
        self.spawn(move |shared| {
            let g = shared.read().unwrap();
            let Some(t) = g.as_ref().filter(|t| t.valid(key.0)) else {
                return Msg::Treemap(seq, TmLayout { key, rects: Vec::new(), labels: Vec::new() });
            };
            let rects = t.treemap(key.0, key.2 as f64, key.3 as f64, key.1);
            let mut labels = Vec::new();
            for (i, r) in rects.iter().enumerate().skip(1) {
                if labels.len() >= 80 {
                    break;
                }
                if r.w >= 90.0 && r.h >= 34.0 {
                    labels.push((i, t.name(r.id).to_string()));
                }
            }
            Msg::Treemap(seq, TmLayout { key, rects, labels })
        });
    }

    pub fn request_sunburst(&mut self) {
        let key = (self.focus, self.mode);
        if self.sun_key == Some(key) {
            return;
        }
        self.sun_key = Some(key);
        let seq = self.next_seq();
        self.seq_sun = seq;
        self.spawn(move |shared| {
            let g = shared.read().unwrap();
            let arcs = g
                .as_ref()
                .filter(|t| t.valid(key.0))
                .map(|t| {
                    t.sunburst(key.0, viz::SUN_DEPTH, key.1, 0.006)
                        .into_iter()
                        .map(|a| {
                            let n = &t.nodes[a.id as usize];
                            SunArc { id: a.id, d: a.depth, a0: a.a0, a1: a.a1, ext: n.ext, dir: n.is_dir(), name: t.name(a.id).to_string(), value: t.value(a.id, key.1) }
                        })
                        .collect()
                })
                .unwrap_or_default();
            Msg::Sun(seq, arcs)
        });
    }

    pub fn pick(&self, what: Pick) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let suggested = if let Pick::Export(id) = what {
            let g = self.shared.read().unwrap();
            g.as_ref().map(|t| {
                let n = t.name(id).replace([':', '\\', '/'], "");
                format!("PrismDisk {}.csv", if n.is_empty() { "export".to_string() } else { n })
            })
        } else {
            None
        };
        std::thread::spawn(move || {
            let path = match what {
                Pick::Folder => rfd::FileDialog::new().set_title("Choose a folder to analyze").pick_folder(),
                Pick::Csv => rfd::FileDialog::new().set_title("Open a WizTree or PrismDisk CSV export").add_filter("CSV export", &["csv"]).pick_file(),
                Pick::Export(_) => rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(suggested.unwrap_or_default()).save_file(),
            };
            let _ = tx.send(Msg::Picked(what, path));
            ctx.request_repaint();
        });
    }

    pub fn open_item(&mut self, id: u32) {
        if let Some(d) = self.node(id) {
            if let Err(e) = sys::open(d.row.path.as_deref().unwrap_or_default()) {
                self.toast("Could not open", e.to_string(), true);
            }
        }
    }

    pub fn reveal(&mut self, id: u32) {
        if let Some(d) = self.node(id) {
            if let Err(e) = sys::reveal(d.row.path.as_deref().unwrap_or_default()) {
                self.toast("Could not open Explorer", e.to_string(), true);
            }
        }
    }

    pub fn copy_path(&mut self, id: u32) {
        if let Some(d) = self.node(id) {
            let p = d.row.path.clone().unwrap_or_default();
            self.ctx.copy_text(p.clone());
            self.toast("Path copied", p, false);
        }
    }

    pub fn confirm_delete(&mut self, ids: Vec<u32>, permanent: bool) {
        if self.info.as_ref().map(|i| i.kind == "csv").unwrap_or(true) {
            return;
        }
        let root = self.root;
        let rows: Vec<Row> = ids.iter().filter(|&&i| i != root).filter_map(|&i| self.node(i).map(|d| d.row)).collect();
        if rows.is_empty() {
            return;
        }
        if let Some(r) = rows.iter().find(|r| backend::is_protected(r.path.as_deref().unwrap_or_default())) {
            let p = r.path.clone().unwrap_or_default();
            self.toast("Protected system location", format!("PrismDisk won't delete {p} because Windows needs it."), true);
            return;
        }
        let containers = self.containers_in(&rows);
        let total: u64 = rows.iter().map(|r| r.alloc).sum();
        let what = if rows.len() == 1 { format!("“{}”", rows[0].name) } else { format!("{} items", rows.len()) };
        self.modal = Some(Modal {
            title: if permanent { format!("Permanently delete {what}?") } else { format!("Move {what} to the Recycle Bin?") },
            warn: if containers.is_empty() {
                permanent.then(|| "This cannot be undone.".to_string())
            } else {
                let (lead, it) = if containers.len() == 1 {
                    ("This looks like an encrypted container (VeraCrypt or TrueCrypt).", "it")
                } else {
                    ("These look like encrypted containers (VeraCrypt or TrueCrypt).", "them")
                };
                Some(if permanent {
                    format!("{lead} Every file stored inside goes with {it}, and this cannot be undone.")
                } else {
                    format!("{lead} Every file stored inside goes with {it}.")
                })
            },
            body: if permanent { format!("{} total.", fmt_bytes(total)) } else { format!("You can restore it from the Recycle Bin later. {} total.", fmt_bytes(total)) },
            list: if containers.is_empty() {
                rows.iter().map(|r| (r.path.clone().unwrap_or_default(), r.alloc)).collect()
            } else {
                containers.clone()
            },
            ok: if permanent { "Delete permanently".into() } else { "Move to Recycle Bin".into() },
            danger: true,
            ack: (!containers.is_empty()).then(|| {
                let what = if containers.len() == 1 { "this container" } else { "these containers" };
                (format!("I understand everything inside {what} will be lost"), false)
            }),
            lock: !containers.is_empty(),
            action: ModalAction::Delete(rows.iter().map(|r| r.id).collect(), permanent),
        });
    }

    fn containers_in(&self, rows: &[Row]) -> Vec<(String, u64)> {
        let targets: Vec<crate::container::Target> = rows
            .iter()
            .map(|r| crate::container::Target { path: r.path.clone().unwrap_or_default(), name: r.name.clone(), dir: r.dir, size: r.size })
            .collect();
        let g = self.shared.read().unwrap();
        let Some(t) = g.as_ref() else { return Vec::new() };
        crate::container::find(&targets, |path, limit| {
            t.find_path(path).map(|id| t.files_where(id, limit, crate::container::is_candidate)).unwrap_or_default()
        })
    }

    pub fn confirm_clean(&mut self) {
        let Some(items) = &self.clean else { return };
        let items: Vec<&CleanItem> = items.iter().filter(|c| self.clean_sel.contains(&c.row.id)).collect();
        if items.is_empty() {
            return;
        }
        let total: u64 = items.iter().map(|c| c.row.alloc).sum();
        self.modal = Some(Modal {
            title: format!("Clean {} location{}?", items.len(), if items.len() > 1 { "s" } else { "" }),
            warn: None,
            body: format!(
                "About {} of cache and temp files will be deleted permanently. The folders themselves are kept, and files in use are skipped.",
                fmt_bytes(total)
            ),
            list: items.iter().map(|c| (c.row.path.clone().unwrap_or_default(), c.row.alloc)).collect(),
            ok: "Clean now".into(),
            danger: false,
            ack: None,
            lock: false,
            action: ModalAction::Clean(items.iter().map(|c| c.row.id).collect()),
        });
    }

    pub fn run_modal_action(&mut self, action: ModalAction) {
        let (ids, permanent, empty_only, kind) = match action {
            ModalAction::Delete(ids, p) => (ids, p, false, if p { DeleteKind::Permanent } else { DeleteKind::Recycle }),
            ModalAction::Clean(ids) => (ids, true, true, DeleteKind::Clean),
        };
        self.spawn(move |shared| Msg::Deleted(backend::delete_items(shared, &ids, permanent, empty_only), kind));
    }

    fn refresh_after_change(&mut self) {
        self.node_cache.clear();
        self.tm = None;
        self.tm_mesh = None;
        self.tm_in_flight = None;
        self.sun_key = None;
        self.stale_search = true;
        self.stale_dups = true;
        self.stale_clean = true;
        self.reload_tree();
        if let Some(s) = self.sel {
            if !self.vis_index.contains_key(&s) {
                self.sel = Some(self.root);
            }
        }
        self.after_focus_change();
        self.load_clean_pill();
        self.media_after_delete();
    }

    pub fn check_updates(&mut self, manual: bool) {
        if matches!(self.update, UpdateState::Checking | UpdateState::Downloading(..)) {
            return;
        }
        if let (false, UpdateState::Ready(..)) = (manual, &self.update) {
            return;
        }
        self.update = UpdateState::Checking;
        self.spawn(move |_| Msg::UpdateChecked(update::check(), manual));
    }

    fn start_download(&mut self, rel: Release) {
        let progress = Arc::new(AtomicU64::new(0));
        self.update = UpdateState::Downloading(rel.clone(), progress.clone());
        let ctx = self.ctx.clone();
        let ticker = progress.clone();
        let size = rel.size;
        std::thread::spawn(move || {
            while ticker.load(std::sync::atomic::Ordering::Relaxed) < size {
                ctx.request_repaint();
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        });
        self.spawn(move |_| {
            let r = update::download(&rel, progress.clone());
            progress.store(u64::MAX, std::sync::atomic::Ordering::Relaxed);
            Msg::UpdateDownloaded(rel, r)
        });
    }

    pub fn apply_update(&mut self) {
        let UpdateState::Ready(_, path) = &self.update else { return };
        match update::install(path) {
            Ok(()) => self.ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(e) => self.toast("Update not installed", e, true),
        }
    }

    pub fn retry_update(&mut self) {
        if let UpdateState::Failed(_, Some(rel)) = self.update.clone() {
            self.start_download(rel);
        } else {
            self.check_updates(true);
        }
    }

    pub fn open_url(&mut self, url: &str) {
        if !sys::shell_open(url, "") {
            self.toast("Could not open the browser", url.to_string(), true);
        }
    }

    pub fn relaunch_admin(&mut self) {
        let target = self.last_scan.as_ref().filter(|(k, _)| k != "csv").map(|(_, t)| sys::quote_arg(t)).unwrap_or_default();
        if sys::relaunch_elevated(&target) {
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            self.toast("Not restarted", "Elevation was cancelled", true);
        }
    }

    pub fn pump(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Scanned(r) => {
                    self.scan = None;
                    match r {
                        Ok(info) => self.on_scanned(info),
                        Err(e) if e.to_lowercase().contains("cancel") => {}
                        Err(e) => self.toast("Scan failed", e, true),
                    }
                }
                Msg::Find(seq, rows) if seq == self.seq_find => {
                    let total: u64 = rows.iter().map(|r| self.val(r)).sum();
                    self.find_status = if rows.is_empty() {
                        "No matches. Patterns: *.mp4 · name part · ext:iso · \\Downloads\\* (path)".into()
                    } else {
                        format!(
                            "{}{} items · {} combined · double-click to open, right-click for more",
                            fmt_num(rows.len() as u64),
                            if rows.len() == 5000 { "+" } else { "" },
                            fmt_bytes(total)
                        )
                    };
                    self.find = rows;
                }
                Msg::Dups(seq, g) if seq == self.seq_dups => {
                    self.dups = Some(g);
                    self.dup_job = None;
                }
                Msg::MediaList(seq, items) => self.media_on_list(seq, items),
                Msg::MediaIndexed(seq, items, groups, fresh) => self.media_on_indexed(seq, items, groups, fresh),
                Msg::Clean(seq, items) if seq == self.seq_clean => {
                    let keep: HashSet<u32> = items.iter().map(|c| c.row.id).collect();
                    self.clean_sel.retain(|id| keep.contains(id));
                    self.clean = Some(items);
                }
                Msg::CleanPill(total) => self.clean_pill = total,
                Msg::Types(seq, mut stats) if seq == self.seq_types => {
                    let m = self.mode;
                    stats.sort_by(|a, b| if m == Mode::Alloc { b.alloc.cmp(&a.alloc) } else { b.size.cmp(&a.size) });
                    self.types = stats;
                }
                Msg::Treemap(seq, layout) if seq == self.seq_tm => {
                    self.tm_in_flight = None;
                    self.tm_mesh = None;
                    self.tm_hover = None;
                    self.tm = Some(layout);
                }
                Msg::Sun(seq, arcs) if seq == self.seq_sun => {
                    self.sun = arcs;
                    self.sun_hover = None;
                }
                Msg::Deleted(res, kind) => {
                    match kind {
                        DeleteKind::Clean => {
                            let msg = if res.failed.is_empty() { "Cleanup complete.".to_string() } else { format!("{} location(s) had files in use that were skipped.", res.failed.len()) };
                            self.toast(format!("Freed {}", fmt_bytes(res.freed)), msg, false);
                            self.clean_sel.clear();
                        }
                        _ => {
                            if res.removed > 0 {
                                let title = if kind == DeleteKind::Permanent {
                                    format!("Deleted · freed {}", fmt_bytes(res.freed))
                                } else {
                                    format!("Moved {} to the Recycle Bin", fmt_bytes(res.freed))
                                };
                                self.toast(title, "", false);
                            }
                            if !res.failed.is_empty() {
                                self.toast("Some items were not deleted", res.failed.iter().take(3).cloned().collect::<Vec<_>>().join("\n"), true);
                            }
                        }
                    }
                    self.refresh_after_change();
                }
                Msg::UpdateChecked(r, manual) => match r {
                    Ok(Some(rel)) => {
                        self.update_banner = true;
                        self.start_download(rel);
                    }
                    Ok(None) => {
                        self.update = UpdateState::UpToDate;
                        if manual {
                            self.toast("You're up to date", format!("PrismDisk {} is the latest version.", update::current_version()), false);
                        }
                    }
                    Err(e) => {
                        self.update = UpdateState::Failed(e.clone(), None);
                        if manual {
                            self.toast("Couldn't check for updates", e, true);
                        }
                    }
                },
                Msg::UpdateDownloaded(rel, r) => match r {
                    Ok(path) => self.update = UpdateState::Ready(rel, path),
                    Err(e) => self.update = UpdateState::Failed(e, Some(rel)),
                },
                Msg::Exported(r) => match r {
                    Ok(p) => self.toast("Export complete", p, false),
                    Err(e) => self.toast("Export failed", e, true),
                },
                Msg::Picked(what, Some(path)) => {
                    let p = path.display().to_string();
                    match what {
                        Pick::Folder => {
                            let kind = if p.trim_end_matches('\\').len() == 2 { "drive" } else { "folder" };
                            self.start_scan(kind, &p);
                        }
                        Pick::Csv => self.start_scan("csv", &p),
                        Pick::Export(id) => {
                            self.spawn(move |shared| Msg::Exported(backend::export_csv(shared, id, &path).map(|_| p)));
                        }
                    }
                }
                _ => {}
            }
        }
        self.toasts.retain(|t| t.born.elapsed().as_secs_f32() < if t.err { 7.0 } else { 4.2 });
        if !self.toasts.is_empty() {
            self.ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        if let Some(at) = self.q_dirty_at {
            if at.elapsed().as_millis() >= 260 {
                self.run_find();
            } else {
                self.ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        }
    }
}

pub fn hash_hue(s: &str) -> f32 {
    let mut h: u32 = 0;
    for c in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(c as u32);
    }
    (h % 360) as f32
}
