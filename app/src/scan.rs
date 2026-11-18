use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use rayon::prelude::*;

use crate::tree::{RawEntry, Tree};

#[derive(Default)]
pub struct ScanCtl {
    pub cancel: AtomicBool,
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub phase: Mutex<String>,
    pub current: Mutex<String>,
}

impl ScanCtl {
    pub fn set_phase(&self, s: &str) {
        if let Ok(mut p) = self.phase.lock() {
            *p = s.to_string();
        }
    }
    pub fn reset(&self) {
        self.cancel.store(false, Ordering::Relaxed);
        self.done.store(0, Ordering::Relaxed);
        self.total.store(0, Ordering::Relaxed);
        self.set_phase("Starting");
        if let Ok(mut c) = self.current.lock() {
            c.clear();
        }
    }
}

struct WNode {
    name: String,
    is_dir: bool,
    size: u64,
    alloc: u64,
    modified: u64,
    attrs: u32,
    children: Vec<WNode>,
}

const ATTR_REPARSE: u32 = 0x400;
const ATTR_OFFLINE: u32 = 0x1000;
const ATTR_RECALL_ON_OPEN: u32 = 0x40000;
const ATTR_RECALL_ON_DATA: u32 = 0x400000;

#[cfg(windows)]
fn meta_bits(m: &std::fs::Metadata) -> (u64, u32) {
    use std::os::windows::fs::MetadataExt;
    (m.last_write_time(), m.file_attributes())
}
#[cfg(not(windows))]
fn meta_bits(_m: &std::fs::Metadata) -> (u64, u32) {
    (0, 0)
}

fn walk(dir: &Path, cluster: u64, ctl: &ScanCtl) -> Vec<WNode> {
    if ctl.cancel.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files = Vec::new();
    let mut dirs: Vec<(PathBuf, WNode)> = Vec::new();
    for entry in rd.flatten() {
        let Ok(m) = entry.metadata() else { continue };
        let (modified, attrs) = meta_bits(&m);
        let name = entry.file_name().to_string_lossy().into_owned();
        if m.is_dir() {
            let node = WNode { name, is_dir: true, size: 0, alloc: 0, modified, attrs, children: Vec::new() };
            if attrs & ATTR_REPARSE != 0 {
                files.push(node);
            } else {
                dirs.push((entry.path(), node));
            }
        } else {
            let size = m.len();
            let placeholder = attrs & (ATTR_OFFLINE | ATTR_RECALL_ON_OPEN | ATTR_RECALL_ON_DATA) != 0;
            let alloc = if placeholder { 0 } else { size.div_ceil(cluster) * cluster };
            files.push(WNode { name, is_dir: false, size, alloc, modified, attrs, children: Vec::new() });
        }
    }
    let n = (files.len() + dirs.len()) as u64;
    let prev = ctl.done.fetch_add(n, Ordering::Relaxed);
    if prev / 4096 != (prev + n) / 4096 {
        if let Ok(mut c) = ctl.current.lock() {
            *c = dir.display().to_string();
        }
    }
    let sub: Vec<WNode> = dirs
        .into_par_iter()
        .map(|(p, mut node)| {
            node.children = walk(&p, cluster, ctl);
            node
        })
        .collect();
    files.extend(sub);
    files
}

fn flatten(children: Vec<WNode>, parent: u32, out: &mut Vec<RawEntry>) {
    let mut stack: Vec<(u32, Vec<WNode>)> = vec![(parent, children)];
    while let Some((p, kids)) = stack.pop() {
        for k in kids {
            let idx = out.len() as u32;
            out.push(RawEntry {
                parent: p,
                name: k.name,
                is_dir: k.is_dir,
                valid: true,
                size: k.size,
                alloc: k.alloc,
                modified: k.modified,
                attrs: k.attrs,
            });
            if !k.children.is_empty() {
                stack.push((idx, k.children));
            }
        }
    }
}

pub fn walk_folder(root: &Path, cluster: u64, ctl: &ScanCtl) -> Vec<RawEntry> {
    ctl.set_phase("Walking folders");
    let kids = walk(root, cluster.max(512), ctl);
    let mut out = vec![RawEntry { parent: 0, name: String::new(), is_dir: true, valid: true, ..Default::default() }];
    flatten(kids, 0, &mut out);
    out
}

fn parse_csv_line(line: &str, out: &mut Vec<String>) {
    out.clear();
    let mut cur = String::new();
    let mut q = false;
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' if q && it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            '"' => q = !q,
            ',' if !q => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
}

fn parse_date(s: &str) -> u64 {
    let d: Vec<i64> = s
        .split(|c: char| !c.is_ascii_digit())
        .filter(|x| !x.is_empty())
        .filter_map(|x| x.parse().ok())
        .collect();
    if d.len() < 3 || d[0] < 1601 {
        return 0;
    }
    let (h, mi, se) = (*d.get(3).unwrap_or(&0), *d.get(4).unwrap_or(&0), *d.get(5).unwrap_or(&0));
    let days = days_from_civil(d[0], d[1], d[2]);
    let unix = days * 86400 + h * 3600 + mi * 60 + se;
    ((unix + 11_644_473_600) * 10_000_000) as u64
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn fmt_date(ft: u64) -> String {
    if ft == 0 {
        return String::new();
    }
    let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
    let (y, m, d) = civil_from_days(secs.div_euclid(86400));
    let t = secs.rem_euclid(86400);
    format!("{y:04}/{m:02}/{d:02} {:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
}

pub fn import_csv(text: &str) -> Result<(Vec<RawEntry>, String), String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let mut cols = Vec::new();
    let mut header = None;
    for _ in 0..3 {
        let Some(l) = lines.next() else { break };
        let l = l.trim_start_matches('\u{feff}');
        if l.to_lowercase().contains("file name") {
            parse_csv_line(l, &mut cols);
            header = Some(cols.iter().map(|c| c.trim().to_lowercase()).collect::<Vec<_>>());
            break;
        }
    }
    let header = header.ok_or("Not a WizTree CSV: no \"File Name\" header found")?;
    let col = |n: &str| header.iter().position(|h| h == n);
    let i_name = col("file name").ok_or("missing File Name column")?;
    let i_size = col("size").ok_or("missing Size column")?;
    let i_alloc = col("allocated");
    let i_mod = col("modified");
    let i_attr = col("attributes");

    let mut out = vec![RawEntry { parent: 0, is_dir: true, valid: true, ..Default::default() }];
    let mut by_path: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut root_path: Option<String> = None;

    for line in lines {
        parse_csv_line(line, &mut cols);
        let Some(raw) = cols.get(i_name) else { continue };
        let is_dir = raw.ends_with('\\');
        let path = raw.trim_end_matches('\\').to_string();
        if path.is_empty() {
            continue;
        }
        let get = |i: Option<usize>| i.and_then(|i| cols.get(i)).map(|s| s.trim()).unwrap_or("");
        let size: u64 = cols.get(i_size).and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        let alloc: u64 = get(i_alloc).parse().unwrap_or(size);
        let modified = parse_date(get(i_mod));
        let attrs: u32 = get(i_attr).parse().unwrap_or(0);

        if root_path.is_none() {
            let rp = if is_dir { path.clone() } else { path.rsplit_once('\\').map(|x| x.0.to_string()).unwrap_or_default() };
            by_path.insert(rp.to_lowercase(), 0);
            out[0].modified = if is_dir { modified } else { 0 };
            root_path = Some(rp);
            if is_dir {
                continue;
            }
        }
        let key = path.to_lowercase();
        let (parent_path, name) = match path.rsplit_once('\\') {
            Some((p, n)) => (p.to_string(), n.to_string()),
            None => continue,
        };
        let parent = ensure_dir(&parent_path, &mut out, &mut by_path);
        if is_dir {
            if let Some(&existing) = by_path.get(&key) {
                out[existing as usize].modified = modified;
                out[existing as usize].attrs = attrs;
                continue;
            }
        }
        let idx = out.len() as u32;
        out.push(RawEntry {
            parent,
            name,
            is_dir,
            valid: true,
            size: if is_dir { 0 } else { size },
            alloc: if is_dir { 0 } else { alloc },
            modified,
            attrs,
        });
        if is_dir {
            by_path.insert(key, idx);
        }
    }
    let root = root_path.ok_or("CSV contains no rows")?;
    Ok((out, if root.len() == 2 && root.ends_with(':') { format!("{root}\\") } else { root }))
}

fn ensure_dir(orig: &str, out: &mut Vec<RawEntry>, by_path: &mut std::collections::HashMap<String, u32>) -> u32 {
    let key = orig.to_lowercase();
    if let Some(&i) = by_path.get(&key) {
        return i;
    }
    let Some((parent_path, name)) = orig.rsplit_once('\\') else { return 0 };
    let parent = ensure_dir(parent_path, out, by_path);
    let idx = out.len() as u32;
    out.push(RawEntry { parent, name: name.to_string(), is_dir: true, valid: true, ..Default::default() });
    by_path.insert(key, idx);
    idx
}

fn csv_escape(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

pub fn export_csv(tree: &Tree, start: u32, path: &Path) -> std::io::Result<u64> {
    use std::io::Write;
    let f = std::fs::File::create(path)?;
    let mut w = std::io::BufWriter::with_capacity(1 << 20, f);
    w.write_all(b"\xEF\xBB\xBF")?;
    writeln!(w, "Generated by PrismDisk {}", env!("CARGO_PKG_VERSION"))?;
    writeln!(w, "File Name,Size,Allocated,Modified,Attributes,Files,Folders")?;
    let mut rows = 0u64;
    let base = tree.path(start);
    let mut stack: Vec<(u32, String)> = vec![(start, base)];
    while let Some((id, p)) = stack.pop() {
        let n = &tree.nodes[id as usize];
        let shown = if n.is_dir() && !p.ends_with('\\') { format!("{p}\\") } else { p.clone() };
        writeln!(
            w,
            "{},{},{},{},{},{},{}",
            csv_escape(&shown),
            n.size,
            n.alloc,
            fmt_date(n.modified),
            n.attrs,
            n.files,
            n.folders
        )?;
        rows += 1;
        if n.is_dir() {
            let kids: Vec<u32> = tree.children(id).collect();
            for &c in kids.iter().rev() {
                let cp = if p.ends_with('\\') { format!("{p}{}", tree.name(c)) } else { format!("{p}\\{}", tree.name(c)) };
                stack.push((c, cp));
            }
        }
    }
    w.flush()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_roundtrip() {
        let csv = "Generated by WizTree\r\nFile Name,Size,Allocated,Modified,Attributes,Files,Folders\r\n\
            \"D:\\\",300,8192,2024/01/02 03:04:05,16,2,1\r\n\
            \"D:\\Games\\\",200,4096,,16,1,0\r\n\
            \"D:\\Games\\big, file.bin\",200,4096,2023/12/31 23:59:59,32,0,0\r\n\
            \"D:\\a.txt\",100,4096,,32,0,0\r\n";
        let (raw, root) = import_csv(csv).unwrap();
        assert_eq!(root, "D:\\");
        let t = crate::tree::build(raw, 0, "D:".into(), root).unwrap();
        assert_eq!(t.nodes[0].size, 300);
        assert_eq!(t.nodes[0].files, 2);
        let id = t.find_path("D:\\Games\\big, file.bin").unwrap();
        assert_eq!(fmt_date(t.nodes[id as usize].modified), "2023/12/31 23:59:59");
    }
}
