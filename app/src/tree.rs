use std::collections::{BinaryHeap, HashMap};
use std::cmp::Reverse;

pub const NONE: u32 = u32::MAX;

const FLAG_DIR: u8 = 1;
const FLAG_DELETED: u8 = 2;

#[derive(Default, Clone)]
pub struct RawEntry {
    pub parent: u32,
    pub name: String,
    pub is_dir: bool,
    pub valid: bool,
    pub size: u64,
    pub alloc: u64,
    pub modified: u64,
    pub attrs: u32,
}

#[derive(Clone)]
pub struct Node {
    name_off: u32,
    name_len: u32,
    pub parent: u32,
    pub first_child: u32,
    pub child_count: u32,
    pub files: u32,
    pub folders: u32,
    pub size: u64,
    pub alloc: u64,
    pub modified: u64,
    pub attrs: u32,
    pub ext: u16,
    flags: u8,
}

impl Node {
    pub fn is_dir(&self) -> bool {
        self.flags & FLAG_DIR != 0
    }
    pub fn deleted(&self) -> bool {
        self.flags & FLAG_DELETED != 0
    }
}

pub struct Tree {
    pub nodes: Vec<Node>,
    names: String,
    pub exts: Vec<String>,
    pub root: u32,
    pub root_path: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Alloc,
    Size,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub id: u32,
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub alloc: u64,
    pub files: u32,
    pub folders: u32,
    pub modified: i64,
    pub attrs: u32,
    pub ext: u16,
    pub kids: u32,
    pub path: Option<String>,
}

pub fn filetime_to_unix_ms(ft: u64) -> i64 {
    if ft == 0 {
        return 0;
    }
    (ft / 10_000) as i64 - 11_644_473_600_000
}

fn ext_of(name: &str) -> Option<&str> {
    let dot = name.rfind('.')?;
    if dot == 0 || dot + 1 >= name.len() {
        return None;
    }
    let ext = &name[dot + 1..];
    if ext.len() > 16 || ext.contains(' ') {
        return None;
    }
    Some(ext)
}

pub fn build(mut raw: Vec<RawEntry>, root: usize, root_name: String, root_path: String) -> Option<Tree> {
    let n = raw.len();
    if root >= n {
        return None;
    }
    raw[root].valid = true;
    raw[root].is_dir = true;

    let mut head = vec![NONE; n];
    let mut next = vec![NONE; n];
    for i in 0..n {
        if i == root || !raw[i].valid {
            continue;
        }
        let p = raw[i].parent as usize;
        if p >= n || p == i || !raw[p].valid || !raw[p].is_dir {
            continue;
        }
        next[i] = head[p];
        head[p] = i as u32;
    }

    let mut order: Vec<u32> = Vec::with_capacity(n / 2);
    let mut seen = vec![false; n];
    let mut stack = vec![root as u32];
    seen[root] = true;
    while let Some(i) = stack.pop() {
        order.push(i);
        let mut c = head[i as usize];
        while c != NONE {
            if !seen[c as usize] {
                seen[c as usize] = true;
                stack.push(c);
            }
            c = next[c as usize];
        }
    }
    drop(seen);

    let mut agg_alloc = vec![0u64; n];
    let mut agg_size = vec![0u64; n];
    let mut files = vec![0u32; n];
    let mut folders = vec![0u32; n];
    for &i in order.iter().rev() {
        let i = i as usize;
        let e = &raw[i];
        if !e.is_dir {
            agg_alloc[i] += e.alloc;
            agg_size[i] += e.size;
            files[i] += 1;
        }
        if i != root {
            let p = e.parent as usize;
            agg_alloc[p] += agg_alloc[i];
            agg_size[p] += agg_size[i];
            files[p] += files[i];
            folders[p] += folders[i] + if e.is_dir { 1 } else { 0 };
        }
    }

    let mut raw_names: Vec<String> = raw.iter_mut().map(|e| std::mem::take(&mut e.name)).collect();
    let mut names = String::with_capacity(order.len() * 16);
    let mut ext_ids: HashMap<String, u16> = HashMap::new();
    let mut exts: Vec<String> = vec![String::new()];
    let mut nodes: Vec<Node> = Vec::with_capacity(order.len());
    let mut src: Vec<u32> = Vec::with_capacity(order.len());

    let mut push = |nodes: &mut Vec<Node>, src: &mut Vec<u32>, names: &mut String, r: usize, parent: u32, name: &str| {
        let e = &raw[r];
        let ext = if e.is_dir {
            0
        } else if let Some(x) = ext_of(name) {
            let lx = x.to_ascii_lowercase();
            match ext_ids.get(&lx) {
                Some(&id) => id,
                None if exts.len() < u16::MAX as usize => {
                    let id = exts.len() as u16;
                    exts.push(lx.clone());
                    ext_ids.insert(lx, id);
                    id
                }
                None => 0,
            }
        } else {
            0
        };
        let off = names.len() as u32;
        names.push_str(name);
        nodes.push(Node {
            name_off: off,
            name_len: name.len() as u32,
            parent,
            first_child: NONE,
            child_count: 0,
            files: files[r],
            folders: folders[r],
            size: agg_size[r],
            alloc: agg_alloc[r],
            modified: e.modified,
            attrs: e.attrs,
            ext,
            flags: if e.is_dir { FLAG_DIR } else { 0 },
        });
        src.push(r as u32);
    };

    push(&mut nodes, &mut src, &mut names, root, NONE, &root_name);
    let mut kids: Vec<u32> = Vec::new();
    let mut q = 0usize;
    while q < nodes.len() {
        let r = src[q] as usize;
        kids.clear();
        let mut c = head[r];
        while c != NONE {
            kids.push(c);
            c = next[c as usize];
        }
        if !kids.is_empty() {
            kids.sort_unstable_by(|&a, &b| {
                agg_alloc[b as usize]
                    .cmp(&agg_alloc[a as usize])
                    .then(agg_size[b as usize].cmp(&agg_size[a as usize]))
            });
            nodes[q].first_child = nodes.len() as u32;
            nodes[q].child_count = kids.len() as u32;
            for &k in &kids {
                let name = std::mem::take(&mut raw_names[k as usize]);
                push(&mut nodes, &mut src, &mut names, k as usize, q as u32, &name);
            }
        }
        q += 1;
    }
    names.shrink_to_fit();

    Some(Tree { nodes, names, exts, root: 0, root_path })
}

pub struct Arc {
    pub id: u32,
    pub depth: u8,
    pub a0: f32,
    pub a1: f32,
}

impl Tree {
    pub fn name(&self, id: u32) -> &str {
        let n = &self.nodes[id as usize];
        &self.names[n.name_off as usize..(n.name_off + n.name_len) as usize]
    }

    pub fn valid(&self, id: u32) -> bool {
        (id as usize) < self.nodes.len() && !self.nodes[id as usize].deleted()
    }

    pub fn value(&self, id: u32, mode: Mode) -> u64 {
        let n = &self.nodes[id as usize];
        match mode {
            Mode::Alloc => n.alloc,
            Mode::Size => n.size,
        }
    }

    pub fn children(&self, id: u32) -> impl Iterator<Item = u32> + '_ {
        let n = &self.nodes[id as usize];
        let (start, count) = if n.first_child == NONE { (0, 0) } else { (n.first_child, n.child_count) };
        (start..start + count).filter(move |&c| !self.nodes[c as usize].deleted())
    }

    pub fn live_kids(&self, id: u32) -> u32 {
        self.children(id).count() as u32
    }

    pub fn path(&self, id: u32) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut cur = id;
        while cur != self.root && cur != NONE {
            parts.push(self.name(cur));
            cur = self.nodes[cur as usize].parent;
        }
        let mut out = self.root_path.clone();
        for p in parts.iter().rev() {
            if !out.ends_with('\\') {
                out.push('\\');
            }
            out.push_str(p);
        }
        out
    }

    pub fn ancestors(&self, id: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cur = id;
        while cur != NONE {
            out.push(cur);
            if cur == self.root {
                break;
            }
            cur = self.nodes[cur as usize].parent;
        }
        out.reverse();
        out
    }

    pub fn row(&self, id: u32, with_path: bool) -> Row {
        let n = &self.nodes[id as usize];
        Row {
            id,
            name: self.name(id).to_string(),
            dir: n.is_dir(),
            size: n.size,
            alloc: n.alloc,
            files: n.files,
            folders: n.folders,
            modified: filetime_to_unix_ms(n.modified),
            attrs: n.attrs,
            ext: n.ext,
            kids: if n.is_dir() { self.live_kids(id) } else { 0 },
            path: if with_path { Some(self.path(id)) } else { None },
        }
    }

    pub fn child_rows(&self, id: u32, key: &str, desc: bool) -> Vec<Row> {
        let mut ids: Vec<u32> = self.children(id).collect();
        match key {
            "name" => ids.sort_by_cached_key(|&c| self.name(c).to_lowercase()),
            "size" => ids.sort_by_key(|&c| self.nodes[c as usize].size),
            "files" => ids.sort_by_key(|&c| self.nodes[c as usize].files),
            "folders" => ids.sort_by_key(|&c| self.nodes[c as usize].folders),
            "modified" => ids.sort_by_key(|&c| self.nodes[c as usize].modified),
            _ => ids.sort_by_key(|&c| self.nodes[c as usize].alloc),
        }
        if desc {
            ids.reverse();
        }
        if key == "name" {
            ids.sort_by_key(|&c| !self.nodes[c as usize].is_dir());
        }
        ids.into_iter().map(|c| self.row(c, false)).collect()
    }

    fn walk_paths(&self, start: u32, mut f: impl FnMut(u32, &str) -> bool) {
        let base = self.path(start);
        let mut stack: Vec<(u32, usize)> = vec![(start, base.len())];
        let mut path = base;
        while let Some((id, plen)) = stack.pop() {
            path.truncate(plen);
            if id != start {
                if !path.ends_with('\\') {
                    path.push('\\');
                }
                path.push_str(self.name(id));
            }
            let descend = f(id, &path);
            if descend && self.nodes[id as usize].is_dir() {
                let l = path.len();
                for c in self.children(id) {
                    stack.push((c, l));
                }
            }
        }
    }

    pub fn find(
        &self,
        start: u32,
        pattern: &str,
        kind: &str,
        min_size: u64,
        max_modified: u64,
        limit: usize,
        mode: Mode,
    ) -> Vec<Row> {
        let pat = pattern.trim().to_lowercase();
        let path_match = pat.contains('\\') || pat.contains('/');
        let pat = pat.replace('/', "\\");
        let wildcard = pat.contains('*') || pat.contains('?');
        let ext_filter = pat.strip_prefix("ext:").map(|s| s.trim_start_matches('.').to_string());
        let ext_id = ext_filter.as_ref().map(|e| self.exts.iter().position(|x| x == e).unwrap_or(usize::MAX));

        let want_files = kind != "folders";
        let want_dirs = kind == "folders" || kind == "all";
        let mut heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::with_capacity(limit + 1);

        let mut lower = String::new();
        self.walk_paths(start, |id, path| {
            let n = &self.nodes[id as usize];
            if id == start && n.is_dir() {
                return true;
            }
            let is_dir = n.is_dir();
            let ok_kind = if is_dir { want_dirs } else { want_files };
            let v = self.value(id, mode);
            let old_enough = max_modified == 0 || (n.modified != 0 && n.modified < max_modified);
            if ok_kind && v >= min_size && old_enough {
                let matched = if let Some(eid) = ext_id {
                    !is_dir && n.ext as usize == eid
                } else if pat.is_empty() {
                    true
                } else {
                    lower.clear();
                    if path_match {
                        lower.push_str(&path.to_lowercase());
                    } else {
                        lower.push_str(&self.name(id).to_lowercase());
                    }
                    if wildcard { wild_match(&pat, &lower) } else { lower.contains(&pat) }
                };
                if matched {
                    heap.push(Reverse((v, id)));
                    if heap.len() > limit {
                        heap.pop();
                    }
                }
            }
            is_dir && self.value(id, mode) >= min_size
        });

        let mut out: Vec<(u64, u32)> = heap.into_iter().map(|Reverse(x)| x).collect();
        out.sort_unstable_by(|a, b| b.cmp(a));
        out.into_iter().map(|(_, id)| self.row(id, true)).collect()
    }

    pub fn ext_stats(&self, start: u32) -> Vec<(u16, u64, u64, u64)> {
        let mut acc: Vec<(u64, u64, u64)> = vec![(0, 0, 0); self.exts.len()];
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            let n = &self.nodes[id as usize];
            if n.is_dir() {
                stack.extend(self.children(id));
            } else {
                let a = &mut acc[n.ext as usize];
                a.0 += n.alloc;
                a.1 += n.size;
                a.2 += 1;
            }
        }
        let mut out: Vec<(u16, u64, u64, u64)> = acc
            .into_iter()
            .enumerate()
            .filter(|(_, a)| a.2 > 0)
            .map(|(i, a)| (i as u16, a.0, a.1, a.2))
            .collect();
        out.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
        out
    }

    pub fn duplicates(&self, start: u32, min_size: u64, limit: usize) -> Vec<(u64, Vec<u32>)> {
        let mut groups: HashMap<(u64, String), Vec<u32>> = HashMap::new();
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            let n = &self.nodes[id as usize];
            if n.is_dir() {
                if n.size >= min_size {
                    stack.extend(self.children(id));
                }
            } else if n.size >= min_size {
                groups.entry((n.size, self.name(id).to_lowercase())).or_default().push(id);
            }
        }
        let mut out: Vec<(u64, Vec<u32>)> = groups
            .into_iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|((size, _), v)| (size, v))
            .collect();
        out.sort_unstable_by(|a, b| {
            let wa = a.0 * (a.1.len() as u64 - 1);
            let wb = b.0 * (b.1.len() as u64 - 1);
            wb.cmp(&wa)
        });
        out.truncate(limit);
        out
    }

    pub fn cleanup_candidates(&self, start: u32) -> Vec<(u32, &'static CleanRule)> {
        let mut out = Vec::new();
        let mut lower = String::new();
        self.walk_paths(start, |id, path| {
            let n = &self.nodes[id as usize];
            if !n.is_dir() {
                return false;
            }
            lower.clear();
            lower.push_str(&path.to_lowercase());
            for rule in CLEAN_RULES {
                if (rule.matcher)(&lower) {
                    if n.alloc > 0 {
                        out.push((id, rule));
                    }
                    return false;
                }
            }
            true
        });
        out.sort_unstable_by(|a, b| self.nodes[b.0 as usize].alloc.cmp(&self.nodes[a.0 as usize].alloc));
        out
    }

    pub fn remove(&mut self, id: u32) {
        if !self.valid(id) || id == self.root {
            return;
        }
        let (alloc, size, files, folders, is_dir) = {
            let n = &self.nodes[id as usize];
            (n.alloc, n.size, n.files, n.folders, n.is_dir())
        };
        self.nodes[id as usize].flags |= FLAG_DELETED;
        let mut cur = self.nodes[id as usize].parent;
        while cur != NONE {
            let p = &mut self.nodes[cur as usize];
            p.alloc = p.alloc.saturating_sub(alloc);
            p.size = p.size.saturating_sub(size);
            p.files = p.files.saturating_sub(files);
            p.folders = p.folders.saturating_sub(folders + if is_dir { 1 } else { 0 });
            cur = p.parent;
        }
    }

    pub fn find_path(&self, path: &str) -> Option<u32> {
        let rel = path
            .to_lowercase()
            .strip_prefix(&self.root_path.to_lowercase())?
            .trim_matches('\\')
            .to_string();
        let mut cur = self.root;
        if rel.is_empty() {
            return Some(cur);
        }
        for part in rel.split('\\').filter(|s| !s.is_empty()) {
            cur = self.children(cur).find(|&c| self.name(c).to_lowercase() == part)?;
        }
        Some(cur)
    }

    pub fn sunburst(&self, focus: u32, max_depth: u8, mode: Mode, min_angle: f32) -> Vec<Arc> {
        let mut out = Vec::new();
        let total = self.value(focus, mode);
        if total == 0 {
            return out;
        }
        let mut stack: Vec<(u32, u8, f64, f64)> = vec![(focus, 0, 0.0, std::f64::consts::TAU)];
        while let Some((id, depth, a0, a1)) = stack.pop() {
            if depth > 0 {
                out.push(Arc { id, depth, a0: a0 as f32, a1: a1 as f32 });
            }
            if depth >= max_depth || !self.nodes[id as usize].is_dir() {
                continue;
            }
            let v = self.value(id, mode) as f64;
            if v <= 0.0 {
                continue;
            }
            let span = a1 - a0;
            let mut a = a0;
            for c in self.children(id) {
                let cv = self.value(c, mode) as f64;
                let w = span * cv / v;
                if w < min_angle as f64 {
                    continue;
                }
                stack.push((c, depth + 1, a, a + w));
                a += w;
            }
        }
        out
    }

    pub fn treemap(&self, focus: u32, w: f64, h: f64, mode: Mode) -> Vec<TmRect> {
        let mut out: Vec<TmRect> = Vec::with_capacity(16_384);
        let mut items: Vec<(u32, f64)> = Vec::new();
        let mut rects: Vec<(u32, [f64; 4])> = Vec::new();
        let mut stack: Vec<(u32, [f64; 4], u8)> = vec![(focus, [0.0, 0.0, w, h], 0)];
        const MAX_RECTS: usize = 250_000;

        while let Some((id, r, depth)) = stack.pop() {
            let n = &self.nodes[id as usize];
            out.push(TmRect { x: r[0] as f32, y: r[1] as f32, w: r[2] as f32, h: r[3] as f32, id, ext: n.ext, depth, dir: n.is_dir() });
            if out.len() >= MAX_RECTS {
                break;
            }
            if !n.is_dir() || r[2] < 3.0 || r[3] < 3.0 {
                continue;
            }
            let pad = if r[2] > 12.0 && r[3] > 12.0 { 1.0 } else { 0.0 };
            let inner = [r[0] + pad, r[1] + pad, r[2] - 2.0 * pad, r[3] - 2.0 * pad];
            let total = self.value(id, mode) as f64;
            if total <= 0.0 || inner[2] <= 0.0 || inner[3] <= 0.0 {
                continue;
            }
            let scale = inner[2] * inner[3] / total;
            items.clear();
            for c in self.children(id) {
                let a = self.value(c, mode) as f64 * scale;
                if a > 0.0 {
                    items.push((c, a));
                }
            }
            if mode == Mode::Size {
                items.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
            let mut keep = items.len();
            while keep > 0 && items[keep - 1].1 < 1.5 {
                keep -= 1;
            }
            items.truncate(keep);
            if items.is_empty() {
                continue;
            }
            rects.clear();
            squarify(&items, inner, &mut rects);
            for &(c, cr) in rects.iter().rev() {
                if cr[2] >= 0.5 && cr[3] >= 0.5 {
                    stack.push((c, cr, depth.saturating_add(1)));
                }
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TmRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub id: u32,
    pub ext: u16,
    pub depth: u8,
    pub dir: bool,
}

fn squarify(items: &[(u32, f64)], rect: [f64; 4], out: &mut Vec<(u32, [f64; 4])>) {
    let [mut x, mut y, mut w, mut h] = rect;
    let mut i = 0;
    while i < items.len() {
        let short = w.min(h).max(1e-9);
        let mut sum = 0.0;
        let mut j = i;
        let mut best = f64::INFINITY;
        while j < items.len() {
            let s = sum + items[j].1;
            let max_a = items[i].1;
            let min_a = items[j].1;
            let worst = ((short * short * max_a) / (s * s)).max((s * s) / (short * short * min_a));
            if worst > best {
                break;
            }
            best = worst;
            sum = s;
            j += 1;
        }
        let thick = (sum / short).min(if w >= h { w } else { h });
        if w >= h {
            let mut yy = y;
            for k in i..j {
                let hh = items[k].1 / thick;
                out.push((items[k].0, [x, yy, thick, hh]));
                yy += hh;
            }
            x += thick;
            w -= thick;
        } else {
            let mut xx = x;
            for k in i..j {
                let ww = items[k].1 / thick;
                out.push((items[k].0, [xx, y, ww, thick]));
                xx += ww;
            }
            y += thick;
            h -= thick;
        }
        if w <= 0.0 || h <= 0.0 {
            break;
        }
        i = j;
    }
}

pub fn wild_match(pat: &str, text: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = ti;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub struct CleanRule {
    pub category: &'static str,
    pub label: &'static str,
    pub note: &'static str,
    matcher: fn(&str) -> bool,
}

fn ends(p: &str, suffix: &str) -> bool {
    p.ends_with(suffix)
}

pub static CLEAN_RULES: &[CleanRule] = &[
    CleanRule { category: "Temp", label: "User temp files", note: "Leftovers from installers and apps. Files in use are skipped.",
        matcher: |p| ends(p, "\\appdata\\local\\temp") },
    CleanRule { category: "Temp", label: "Windows temp", note: "System temporary files.",
        matcher: |p| ends(p, "\\windows\\temp") },
    CleanRule { category: "Updates", label: "Windows Update downloads", note: "Already-installed update packages. Windows re-downloads if needed.",
        matcher: |p| ends(p, "\\windows\\softwaredistribution\\download") },
    CleanRule { category: "Shader cache", label: "DirectX shader cache", note: "Rebuilt automatically by games and drivers.",
        matcher: |p| ends(p, "\\appdata\\local\\d3dscache") },
    CleanRule { category: "Shader cache", label: "NVIDIA shader cache", note: "Rebuilt automatically by the driver.",
        matcher: |p| ends(p, "\\nvidia\\dxcache") || ends(p, "\\nvidia\\glcache") || ends(p, "\\nvidia corporation\\nv_cache") },
    CleanRule { category: "Shader cache", label: "AMD shader cache", note: "Rebuilt automatically by the driver.",
        matcher: |p| ends(p, "\\amd\\dxcache") || ends(p, "\\amd\\glcache") || ends(p, "\\amd\\vkcache") },
    CleanRule { category: "Crash dumps", label: "Crash dumps", note: "Diagnostic memory dumps from crashed apps.",
        matcher: |p| ends(p, "\\appdata\\local\\crashdumps") || ends(p, "\\windows\\minidump") || ends(p, "\\programdata\\microsoft\\windows\\wer\\reportarchive") || ends(p, "\\programdata\\microsoft\\windows\\wer\\reportqueue") },
    CleanRule { category: "Browser cache", label: "Browser cache", note: "Cached web content. Sign-ins and history are kept.",
        matcher: |p| (p.contains("\\google\\chrome\\user data\\") || p.contains("\\microsoft\\edge\\user data\\") || p.contains("\\bravesoftware\\brave-browser\\user data\\"))
            && (ends(p, "\\cache") || ends(p, "\\code cache") || ends(p, "\\gpucache")) },
    CleanRule { category: "Dev cache", label: "npm cache", note: "Package tarballs; npm re-downloads on demand.",
        matcher: |p| ends(p, "\\npm-cache") },
    CleanRule { category: "Dev cache", label: "pip cache", note: "Python wheels; pip re-downloads on demand.",
        matcher: |p| ends(p, "\\appdata\\local\\pip\\cache") },
    CleanRule { category: "Dev cache", label: "uv cache", note: "Python package cache for uv.",
        matcher: |p| ends(p, "\\appdata\\local\\uv\\cache") },
    CleanRule { category: "Dev cache", label: "Yarn / pnpm cache", note: "JavaScript package caches.",
        matcher: |p| ends(p, "\\appdata\\local\\yarn\\cache") || ends(p, "\\appdata\\local\\pnpm-cache") },
    CleanRule { category: "Dev cache", label: "NuGet HTTP cache", note: "Cached NuGet downloads.",
        matcher: |p| ends(p, "\\appdata\\local\\nuget\\v3-cache") },
    CleanRule { category: "Dev cache", label: "Cargo registry cache", note: "Downloaded crate archives.",
        matcher: |p| ends(p, "\\.cargo\\registry\\cache") },
    CleanRule { category: "Recycle Bin", label: "Recycle Bin", note: "Files you already deleted.",
        matcher: |p| p.len() <= 16 && ends(p, "\\$recycle.bin") },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn e(parent: u32, name: &str, dir: bool, size: u64) -> RawEntry {
        RawEntry { parent, name: name.into(), is_dir: dir, valid: true, size, alloc: size, modified: 0, attrs: 0 }
    }

    #[test]
    fn builds_and_aggregates() {
        let raw = vec![
            e(0, "", true, 0),
            e(0, "a", true, 0),
            e(1, "x.txt", false, 10),
            e(1, "y.bin", false, 30),
            e(0, "z.txt", false, 5),
        ];
        let t = build(raw, 0, "R".into(), "C:\\".into()).unwrap();
        assert_eq!(t.nodes[0].alloc, 45);
        assert_eq!(t.nodes[0].files, 3);
        assert_eq!(t.nodes[0].folders, 1);
        let kids: Vec<u32> = t.children(0).collect();
        assert_eq!(t.name(kids[0]), "a");
        assert_eq!(t.path(t.find_path("C:\\a\\y.bin").unwrap()), "C:\\a\\y.bin");
        let found = t.find(0, "*.txt", "files", 0, 0, 10, Mode::Alloc);
        assert_eq!(found.len(), 2);
        let mut t = t;
        let a = t.find_path("C:\\a").unwrap();
        t.remove(a);
        assert_eq!(t.nodes[0].alloc, 5);
        assert_eq!(t.nodes[0].folders, 0);
    }

    #[test]
    fn protected_paths() {
        use crate::backend::is_protected as p;
        for yes in [r"C:\", r"C:\Windows", r"C:\Windows\System32", r"C:\Program Files", r"C:\Users\bob", r"C:\$Recycle.Bin", r"D:\pagefile.sys"] {
            assert!(p(yes), "{yes} should be protected");
        }
        for no in [r"C:\Windows\Temp\x.log", r"C:\Program Files\Foo", r"C:\Users\bob\Downloads", r"D:\Games"] {
            assert!(!p(no), "{no} should be deletable");
        }
    }

    #[test]
    fn wildcard() {
        assert!(wild_match("*.mp4", "video.mp4"));
        assert!(wild_match("a?c*", "abcdef"));
        assert!(!wild_match("*.mp4", "video.mp3"));
    }

    #[test]
    fn treemap_fills_area() {
        let raw = vec![e(0, "", true, 0), e(0, "a", false, 60), e(0, "b", false, 30), e(0, "c", false, 10)];
        let t = build(raw, 0, "R".into(), "C:\\".into()).unwrap();
        let rects = t.treemap(0, 100.0, 50.0, Mode::Alloc);
        assert_eq!(rects.len(), 4);
        let area: f32 = rects.iter().skip(1).map(|r| r.w * r.h).sum();
        assert!((area - 98.0 * 48.0).abs() < 1.0, "area {area}");
    }
}
