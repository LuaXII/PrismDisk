use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use eframe::egui::{self, pos2, vec2, Align2, Color32, CursorIcon, Id, Key, Mesh, Order, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};

use crate::app::*;
use crate::icons;
use crate::media::{self, Group, Item, Kind, Match};
use crate::theme::*;
use crate::ui::{child, hchild, prism_bar, rich};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum MView {
    #[default]
    Gallery,
    Similar,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum MFilter {
    #[default]
    All,
    Photos,
    Videos,
    Screens,
    Blurry,
    Large,
    NoDate,
}

const FILTERS: [(MFilter, &str); 7] = [
    (MFilter::All, "All"),
    (MFilter::Photos, "Photos"),
    (MFilter::Videos, "Videos"),
    (MFilter::Screens, "Screenshots"),
    (MFilter::Blurry, "Blurry"),
    (MFilter::Large, "Large videos"),
    (MFilter::NoDate, "No date"),
];

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const ZOOM: [f32; 3] = [92.0, 128.0, 176.0];
const GAP: f32 = 6.0;
const HEAD_H: f32 = 48.0;
const SECTION_PAD: f32 = 14.0;
const SIM_TILE: f32 = 148.0;
const SIM_META_H: f32 = 56.0;

pub struct Section {
    pub title: String,
    pub sub: String,
    pub year: i32,
    pub month: u32,
    pub items: Vec<usize>,
}

pub struct MediaState {
    pub items: Vec<Item>,
    pub groups: Vec<Group>,
    pub job: Option<Arc<media::Job>>,
    pub loading: bool,
    pub indexed: bool,
    pub seq: u64,
    pub stale: bool,
    pub view: MView,
    pub filter: MFilter,
    pub by_folder: bool,
    pub zoom: usize,
    pub scope: media::Scope,
    pub sel: HashSet<u32>,
    pub marked: HashSet<u32>,
    pub lightbox: Option<usize>,
    pub order: Vec<usize>,
    pub sections: Vec<Section>,
    pub counts: [usize; 7],
    pub version: u64,
    pub built: Option<(u64, MFilter, bool)>,
    pub jump: Option<f32>,
    pub anchor: Option<usize>,
    pub group_filter: Option<Match>,
}

impl Default for MediaState {
    fn default() -> Self {
        MediaState {
            items: Vec::new(),
            groups: Vec::new(),
            job: None,
            loading: false,
            indexed: false,
            seq: 0,
            stale: true,
            view: MView::Gallery,
            filter: MFilter::All,
            by_folder: false,
            zoom: 1,
            scope: media::Scope::Personal,
            sel: HashSet::new(),
            marked: HashSet::new(),
            lightbox: None,
            order: Vec::new(),
            sections: Vec::new(),
            counts: [0; 7],
            version: 0,
            built: None,
            jump: None,
            anchor: None,
            group_filter: None,
        }
    }
}

fn local_ym(ms: i64) -> (i32, u32) {
    let s = fmt_date(ms);
    if s.len() < 7 {
        return (0, 0);
    }
    (s[0..4].parse().unwrap_or(0), s[5..7].parse().unwrap_or(0))
}

fn fmt_duration(ms: u32) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn matches(it: &Item, f: MFilter) -> bool {
    match f {
        MFilter::All => true,
        MFilter::Photos => it.kind == Kind::Image,
        MFilter::Videos => it.kind == Kind::Video,
        MFilter::Screens => it.is_screenshot(),
        MFilter::Blurry => it.is_blurry(),
        MFilter::Large => it.kind == Kind::Video && it.size >= media::LARGE_VIDEO,
        MFilter::NoDate => it.meta.is_some() && !it.has_date(),
    }
}

fn summary(items: &[Item], idx: &[usize]) -> String {
    let photos = idx.iter().filter(|&&i| items[i].kind == Kind::Image).count();
    let videos = idx.len() - photos;
    let bytes: u64 = idx.iter().map(|&i| items[i].size).sum();
    let mut parts = Vec::new();
    if photos > 0 {
        parts.push(format!("{} photo{}", fmt_num(photos as u64), if photos == 1 { "" } else { "s" }));
    }
    if videos > 0 {
        parts.push(format!("{} video{}", fmt_num(videos as u64), if videos == 1 { "" } else { "s" }));
    }
    parts.push(fmt_bytes(bytes));
    parts.join(" · ")
}

impl MediaState {
    pub fn reset(&mut self) {
        if let Some(j) = &self.job {
            j.cancel.store(true, Ordering::Relaxed);
            j.hashing.cancel.store(true, Ordering::Relaxed);
        }
        let keep = (self.view, self.filter, self.by_folder, self.zoom, self.scope);
        *self = MediaState::default();
        (self.view, self.filter, self.by_folder, self.zoom, self.scope) = keep;
    }

    fn rebuild(&mut self) {
        let key = (self.version, self.filter, self.by_folder);
        if self.built == Some(key) {
            return;
        }
        self.built = Some(key);
        for (k, (f, _)) in FILTERS.iter().enumerate() {
            self.counts[k] = self.items.iter().filter(|it| matches(it, *f)).count();
        }
        let mut idx: Vec<usize> = (0..self.items.len()).filter(|&i| matches(&self.items[i], self.filter)).collect();
        idx.sort_by_key(|&i| std::cmp::Reverse(self.items[i].date_ms()));
        let mut sections: Vec<Section> = Vec::new();
        if self.by_folder {
            let mut map: HashMap<&str, Vec<usize>> = HashMap::new();
            let mut first: Vec<&str> = Vec::new();
            for &i in &idx {
                let f = self.items[i].folder();
                map.entry(f).or_insert_with(|| {
                    first.push(f);
                    Vec::new()
                }).push(i);
            }
            for f in first {
                let items = map.remove(f).unwrap_or_default();
                let name = f.rsplit('\\').next().filter(|s| !s.is_empty()).unwrap_or(f).to_string();
                sections.push(Section { title: name, sub: format!("{}  ·  {}", summary(&self.items, &items), f), year: 0, month: 0, items });
            }
        } else {
            for &i in &idx {
                let (y, m) = local_ym(self.items[i].date_ms());
                match sections.last_mut() {
                    Some(s) if s.year == y && s.month == m => s.items.push(i),
                    _ => {
                        let title = if y == 0 { "Unknown date".to_string() } else { format!("{} {}", MONTHS[(m as usize).clamp(1, 12) - 1], y) };
                        sections.push(Section { title, sub: String::new(), year: y, month: m, items: vec![i] });
                    }
                }
            }
            for s in sections.iter_mut() {
                s.sub = summary(&self.items, &s.items);
            }
        }
        self.sections = sections;
    }

    fn flat(&self) -> Vec<usize> {
        self.sections.iter().flat_map(|s| s.items.iter().copied()).collect()
    }

    pub fn auto_mark(&mut self) {
        self.marked.clear();
        for g in &self.groups {
            if g.kind != Match::Similar {
                for &i in &g.items {
                    if i != g.keep {
                        self.marked.insert(self.items[i].id);
                    }
                }
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.loading || (self.job.is_some() && !self.indexed)
    }

    fn visible_groups(&self) -> Vec<usize> {
        (0..self.groups.len()).filter(|&g| self.group_filter.map(|k| self.groups[g].kind == k).unwrap_or(true)).collect()
    }
}

fn cover_uv(tex: Vec2, box_size: Vec2) -> Rect {
    let ta = tex.x / tex.y.max(1.0);
    let ba = box_size.x / box_size.y.max(1.0);
    if ta > ba {
        let w = ba / ta;
        Rect::from_min_max(pos2((1.0 - w) / 2.0, 0.0), pos2((1.0 + w) / 2.0, 1.0))
    } else {
        let h = ta / ba;
        Rect::from_min_max(pos2(0.0, (1.0 - h) / 2.0), pos2(1.0, (1.0 + h) / 2.0))
    }
}

fn paint_tex(ui: &Ui, tex: &egui::TextureHandle, rect: Rect, uv: Rect, radius: f32) {
    egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), tex.size_vec2())).uv(uv).corner_radius(radius).paint_at(ui, rect);
}

fn vfade(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(rect.left_top(), top);
    m.colored_vertex(rect.right_top(), top);
    m.colored_vertex(rect.left_bottom(), bottom);
    m.colored_vertex(rect.right_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 3, 2);
    painter.add(Shape::mesh(m));
}

fn chip(ui: &mut Ui, id: Id, label: &str, count: Option<usize>, on: bool) -> egui::Response {
    let cnt = count.map(|c| fmt_num(c as u64));
    let lw = text_w(ui, label, sb(11.5));
    let cw = cnt.as_ref().map(|c| text_w(ui, c, mono(10.5)) + 7.0).unwrap_or(0.0);
    let (rect, _) = ui.allocate_exact_size(vec2(lw + cw + 22.0, 26.0), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 99, rgba(0x6366f1, 0.22));
        p.rect_stroke(rect, 99, Stroke::new(1.0_f32, rgba(0x818cf8, 0.55)), StrokeKind::Inside);
    } else {
        p.rect_filled(rect, 99, if resp.hovered() { white(0.06) } else { white(0.03) });
        p.rect_stroke(rect, 99, Stroke::new(1.0_f32, line()), StrokeKind::Inside);
    }
    let fg = if on { hex(0xc7d2fe) } else if resp.hovered() { TEXT } else { MUTED };
    p.text(pos2(rect.left() + 11.0, rect.center().y), Align2::LEFT_CENTER, label, sb(11.5), fg);
    if let Some(c) = cnt {
        p.text(pos2(rect.right() - 11.0, rect.center().y), Align2::RIGHT_CENTER, c, mono(10.5), if on { INDIGO2 } else { DIM });
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

fn round_check(p: &egui::Painter, ui: &Ui, center: egui::Pos2, on: bool, hover: bool, color: Color32) {
    if on {
        p.circle_filled(center, 10.0, color);
        p.circle_stroke(center, 10.0, Stroke::new(1.5_f32, Color32::WHITE));
        icons::paint(ui, icons::CHECK, Rect::from_center_size(center, vec2(12.0, 12.0)), Color32::WHITE);
    } else {
        p.circle_filled(center, 10.0, if hover { Color32::from_black_alpha(140) } else { Color32::from_black_alpha(90) });
        p.circle_stroke(center, 10.0, Stroke::new(1.5_f32, white(if hover { 0.95 } else { 0.7 })));
    }
}

pub enum TileHit {
    None,
    Click,
    Toggle,
    Double,
    Context(egui::Pos2),
}

impl App {
    pub fn load_media(&mut self) {
        self.media.reset();
        self.media.stale = false;
        self.media.loading = true;
        self.media.seq = self.next_seq_pub();
        let seq = self.media.seq;
        let job = Arc::new(media::Job::default());
        self.media.job = Some(job.clone());
        let (id, sys) = (self.focus, self.media.scope);
        let (shared, tx, ctx) = (self.shared.clone(), self.sender(), self.ctx.clone());
        std::thread::spawn(move || {
            let mut items = {
                let g = shared.read().unwrap();
                match g.as_ref().filter(|t| t.valid(id)) {
                    Some(t) => media::collect(t, id, sys),
                    None => Vec::new(),
                }
            };
            let _ = tx.send(Msg::MediaList(seq, items.clone()));
            ctx.request_repaint();
            media::index(&mut items, &job);
            if job.cancel.load(Ordering::Relaxed) {
                return;
            }
            let groups = media::group(&items, &job);
            if job.cancel.load(Ordering::Relaxed) {
                return;
            }
            let _ = tx.send(Msg::MediaIndexed(seq, items, groups, true));
            ctx.request_repaint();
        });
    }

    pub fn media_after_delete(&mut self) {
        if self.media.items.is_empty() {
            return;
        }
        let gone: HashSet<u32> = {
            let g = self.shared.read().unwrap();
            let Some(t) = g.as_ref() else { return };
            self.media.items.iter().filter(|it| !t.valid(it.id)).map(|it| it.id).collect()
        };
        if gone.is_empty() {
            return;
        }
        self.thumbs.forget(&gone);
        self.media.items.retain(|it| !gone.contains(&it.id));
        self.media.sel.retain(|id| !gone.contains(id));
        self.media.marked.retain(|id| !gone.contains(id));
        self.media.lightbox = None;
        self.media.version += 1;
        if !self.media.indexed {
            return;
        }
        let seq = self.media.seq;
        let items = self.media.items.clone();
        let job = Arc::new(media::Job::default());
        self.media.groups.clear();
        self.media.indexed = false;
        self.media.job = Some(job.clone());
        let (tx, ctx) = (self.sender(), self.ctx.clone());
        std::thread::spawn(move || {
            let groups = media::group(&items, &job);
            let _ = tx.send(Msg::MediaIndexed(seq, items, groups, false));
            ctx.request_repaint();
        });
    }

    pub fn media_on_list(&mut self, seq: u64, items: Vec<Item>) {
        if seq != self.media.seq {
            return;
        }
        self.media.items = items;
        self.media.loading = false;
        self.media.version += 1;
    }

    pub fn media_on_indexed(&mut self, seq: u64, items: Vec<Item>, groups: Vec<Group>, fresh: bool) {
        if seq != self.media.seq {
            return;
        }
        let marked = std::mem::take(&mut self.media.marked);
        self.media.items = items;
        self.media.groups = groups;
        self.media.indexed = true;
        self.media.loading = false;
        self.media.version += 1;
        if !fresh {
            let alive: HashSet<u32> = self.media.items.iter().map(|i| i.id).collect();
            self.media.marked = marked.into_iter().filter(|id| alive.contains(id)).collect();
        }
    }

    pub fn media_keys(&mut self, ctx: &egui::Context) {
        let (esc, left, right, del, shift, ctrl_a, enter) = ctx.input(|i| {
            (
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::Delete),
                i.modifiers.shift,
                i.modifiers.command && i.key_pressed(Key::A),
                i.key_pressed(Key::Enter),
            )
        });
        if let Some(pos) = self.media.lightbox {
            let n = self.media.order.len();
            if esc {
                self.media.lightbox = None;
            } else if left && pos > 0 {
                self.media.lightbox = Some(pos - 1);
            } else if right && pos + 1 < n {
                self.media.lightbox = Some(pos + 1);
            } else if del {
                if let Some(&i) = self.media.order.get(pos) {
                    let id = self.media.items[i].id;
                    self.confirm_delete(vec![id], shift);
                }
            }
            return;
        }
        if esc {
            self.menu = None;
            self.media.sel.clear();
        }
        if ctrl_a && self.media.view == MView::Gallery {
            self.media.rebuild();
            let ids: Vec<u32> = self.media.flat().into_iter().map(|i| self.media.items[i].id).collect();
            self.media.sel = ids.into_iter().collect();
        }
        if del {
            let ids: Vec<u32> = match self.media.view {
                MView::Gallery => self.media.sel.iter().copied().collect(),
                MView::Similar => self.media.marked.iter().copied().collect(),
            };
            if !ids.is_empty() {
                self.confirm_delete(ids, shift);
            }
        }
        if enter && self.media.view == MView::Gallery {
            let flat = self.media.flat();
            if let Some(p) = flat.iter().position(|&i| self.media.sel.contains(&self.media.items[i].id)) {
                self.media.order = flat;
                self.media.lightbox = Some(p);
            }
        }
    }

    fn media_tile(&mut self, ui: &mut Ui, r: Rect, idx: usize, px: u32, on: bool, check_color: Color32, show_check: bool) -> TileHit {
        let it = &self.media.items[idx];
        let (id, image, path) = (it.id, it.kind == Kind::Image, it.path.clone());
        let video = it.kind == Kind::Video;
        let dur = it.meta.as_ref().map(|m| m.duration_ms).unwrap_or(0);
        let name = it.name.clone();
        let resp = ui.interact(r, Id::new(("mtile", id, r.min.x as i32, r.min.y as i32)), Sense::click());
        let p = ui.painter();
        p.rect_filled(r, 9, SURFACE2);
        match self.thumbs.get(id, &path, px, image, false) {
            Some(Some(tex)) => paint_tex(ui, &tex, r, cover_uv(tex.size_vec2(), r.size()), 9.0),
            Some(None) => {
                icons::paint(ui, if video { icons::VIDEO } else { icons::PHOTO }, Rect::from_center_size(r.center() - vec2(0.0, 6.0), vec2(26.0, 26.0)), DIM);
                text(ui, p, r.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, &name, f(10.5), DIM, r.width() - 12.0);
            }
            None => {
                let t = ui.ctx().input(|i| i.time) as f32;
                let a = 0.025 + 0.02 * (t * 3.0 + r.min.x * 0.01 + r.min.y * 0.013).sin().abs();
                p.rect_filled(r, 9, white(a));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
            }
        }
        let hover = resp.hovered();
        if hover {
            let fade = Rect::from_min_max(pos2(r.left(), r.bottom() - 34.0), r.max);
            vfade(p, fade, Color32::TRANSPARENT, Color32::from_black_alpha(190));
            text(ui, p, pos2(r.left() + 8.0, r.bottom() - 9.0), Align2::LEFT_BOTTOM, &name, sb(11.0), Color32::WHITE, r.width() - 16.0);
        }
        if video {
            let label = if dur > 0 { fmt_duration(dur) } else { String::new() };
            let lw = if label.is_empty() { 0.0 } else { text_w(ui, &label, monob(10.0)) + 5.0 };
            let y = if hover { r.top() + 8.0 } else { r.bottom() - 26.0 };
            let x = if hover { r.right() - 8.0 - (lw + 24.0) } else { r.left() + 8.0 };
            let pr = Rect::from_min_size(pos2(x, y), vec2(lw + 24.0, 18.0));
            p.rect_filled(pr, 99, Color32::from_black_alpha(170));
            icons::paint(ui, icons::PLAY, Rect::from_center_size(pos2(pr.left() + 11.0, pr.center().y), vec2(10.0, 10.0)), Color32::WHITE);
            if !label.is_empty() {
                p.text(pos2(pr.left() + 19.0, pr.center().y), Align2::LEFT_CENTER, label, monob(10.0), Color32::WHITE);
            }
        }
        if on {
            p.rect_stroke(r, 9, Stroke::new(2.5_f32, check_color), StrokeKind::Inside);
            p.rect_filled(r.shrink(2.5), 7, with_alpha_pm(check_color, 0.12));
        }
        let cc = r.left_top() + vec2(17.0, 17.0);
        let check_r = Rect::from_center_size(cc, vec2(26.0, 26.0));
        let over_check = ui.ctx().pointer_hover_pos().map(|p| check_r.contains(p)).unwrap_or(false);
        if on || hover || show_check {
            round_check(p, ui, cc, on, over_check, check_color);
        }
        if hover {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if resp.secondary_clicked() {
            return TileHit::Context(resp.interact_pointer_pos().unwrap_or(r.center()));
        }
        if resp.double_clicked() {
            return TileHit::Double;
        }
        if resp.clicked() {
            let ctrl = ui.ctx().input(|i| i.modifiers.command || i.modifiers.shift);
            return if over_check || ctrl { TileHit::Toggle } else { TileHit::Click };
        }
        TileHit::None
    }

    fn node_menu(&mut self, pos: egui::Pos2, id: u32) {
        if let Some(d) = self.node(id) {
            self.menu = Some(Menu { pos, kind: MenuKind::Node(Box::new(d)), opened: self.ctx.input(|i| i.time) });
        }
    }

    pub fn media_tab(&mut self, ui: &mut Ui, body: Rect) {
        if self.media.stale && !self.media.loading {
            self.load_media();
        }
        if self.media.busy() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
        self.media.rebuild();
        let p = ui.painter().clone();
        let tb = Rect::from_min_size(body.min, vec2(body.width(), 51.0));
        hline(&p, body.left(), body.right(), tb.bottom() - 1.0, line());
        let inner = tb.shrink2(vec2(12.0, 10.0));
        let mut left = hchild(ui, inner);
        left.spacing_mut().item_spacing.x = 10.0;
        let sim_label = if self.media.indexed { format!("Duplicates & similar  {}", fmt_num(self.media.groups.len() as u64)) } else { "Duplicates & similar".to_string() };
        let views = [("Gallery", Some(icons::PHOTO)), (sim_label.as_str(), Some(icons::DUP))];
        let cur = if self.media.view == MView::Gallery { 0 } else { 1 };
        if let Some(i) = segmented(&mut left, "mview", &views, cur, true) {
            self.media.view = if i == 0 { MView::Gallery } else { MView::Similar };
            self.media.lightbox = None;
        }
        let mut right = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)));
        right.spacing_mut().item_spacing.x = 10.0;
        if self.media.view == MView::Gallery {
            if let Some(z) = segmented(&mut right, "mzoom", &[("S", None), ("M", None), ("L", None)], self.media.zoom, true) {
                self.media.zoom = z;
            }
            if let Some(g) = segmented(&mut right, "mgroup", &[("By date", None), ("By folder", None)], self.media.by_folder as usize, true) {
                self.media.by_folder = g == 1;
                self.media.jump = Some(0.0);
            }
        }
        let cur_scope = if self.media.scope == media::Scope::Personal { 0 } else { 1 };
        let scope_resp = right.horizontal(|ui| segmented(ui, "mscope", &[("My photos", None), ("All media", None)], cur_scope, true));
        scope_resp.response.on_hover_text("My photos: your Pictures, Videos, Screenshots, Camera Roll, Desktop and Downloads, plus camera and phone files anywhere. Games, apps and code projects are left out.\nAll media: every picture and video file in this scan.");
        if let Some(i) = scope_resp.inner {
            let want = if i == 0 { media::Scope::Personal } else { media::Scope::All };
            if want != self.media.scope {
                self.media.scope = want;
                self.save_settings();
                self.load_media();
            }
        }

        let foot = Rect::from_min_max(pos2(body.left(), body.bottom() - 40.0), body.max);
        let main = Rect::from_min_max(pos2(body.left(), tb.bottom()), pos2(body.right(), foot.top()));
        if self.media.busy() {
            let job = self.media.job.clone();
            let frac = job.as_ref().map(|j| {
                let (d, t) = (j.done.load(Ordering::Relaxed), j.total.load(Ordering::Relaxed));
                if t == 0 { 0.0 } else { d as f32 / t as f32 }
            }).unwrap_or(0.0);
            let bar = Rect::from_min_size(pos2(main.left(), main.top()), vec2(main.width(), 2.0));
            p.rect_filled(bar, 0, white(0.05));
            prism_bar(&p, Rect::from_min_size(bar.min, vec2(main.width() * frac.clamp(0.02, 1.0), 2.0)), 1.0);
        }
        match self.media.view {
            MView::Gallery => self.gallery_ui(ui, main),
            MView::Similar => self.similar_ui(ui, main),
        }
        self.media_footer(ui, foot);
    }

    fn status_line(&self) -> Option<String> {
        if self.media.loading {
            return Some("Finding photos and videos…".into());
        }
        let j = self.media.job.as_ref()?;
        if self.media.indexed {
            return None;
        }
        match j.stage.load(Ordering::Relaxed) {
            0 => {
                let (d, t) = (j.done.load(Ordering::Relaxed), j.total.load(Ordering::Relaxed));
                Some(format!("Analyzing photos and videos… {} of {}", fmt_num(d), fmt_num(t)))
            }
            1 => {
                let (d, t) = (j.hashing.done.load(Ordering::Relaxed), j.hashing.total.load(Ordering::Relaxed));
                Some(format!("Comparing file contents… {} of {}", fmt_bytes(d), fmt_bytes(t.max(d))))
            }
            _ => Some("Matching similar photos and videos…".into()),
        }
    }

    fn media_footer(&mut self, ui: &mut Ui, foot: Rect) {
        let p = ui.painter().clone();
        hline(&p, foot.left(), foot.right(), foot.top(), line());
        let inner = foot.shrink2(vec2(14.0, 6.0));
        let mut row = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)));
        row.spacing_mut().item_spacing.x = 8.0;
        let (ids, label) = match self.media.view {
            MView::Gallery => (self.media.sel.iter().copied().collect::<Vec<u32>>(), "Recycle selected"),
            MView::Similar => (self.media.marked.iter().copied().collect::<Vec<u32>>(), "Recycle marked"),
        };
        let bytes: u64 = {
            let set: HashSet<u32> = ids.iter().copied().collect();
            self.media.items.iter().filter(|it| set.contains(&it.id)).map(|it| it.size).sum()
        };
        if !ids.is_empty() {
            if btn(&mut row, label, Some(icons::TRASH), Btn::Danger, true, true).clicked() {
                self.confirm_delete(ids.clone(), false);
            }
            if btn(&mut row, "Clear", None, Btn::Ghost, true, true).clicked() {
                match self.media.view {
                    MView::Gallery => self.media.sel.clear(),
                    MView::Similar => self.media.marked.clear(),
                }
            }
            let g = rich(&row, &[(&format!("{} ", fmt_num(ids.len() as u64)), monob(12.0), TEXT), (if self.media.view == MView::Gallery { "selected · " } else { "marked · " }, f(12.0), MUTED), (&fmt_bytes(bytes), monob(12.0), hex(0xfca5a5))], 400.0);
            let (gr, _) = row.allocate_exact_size(g.size(), Sense::hover());
            row.painter().galley(gr.min, g, TEXT);
        }
        let right_edge = row.min_rect().left() - 12.0;
        let msg = match self.status_line() {
            Some(s) => s,
            None => {
                let total = self.media.items.len();
                let bytes: u64 = self.media.items.iter().map(|i| i.size).sum();
                if self.media.view == MView::Similar {
                    let wasted: u64 = self.media.groups.iter().map(|g| g.wasted).sum();
                    format!("{} groups · up to {} reclaimable · click a tile to mark it, double-click to preview", fmt_num(self.media.groups.len() as u64), fmt_bytes(wasted))
                } else if total == 0 {
                    String::new()
                } else {
                    format!("{} items · {} · double-click to preview, Ctrl+click to select", fmt_num(total as u64), fmt_bytes(bytes))
                }
            }
        };
        text(ui, &p, pos2(inner.left(), inner.center().y), Align2::LEFT_CENTER, &msg, f(11.5), DIM, (right_edge - inner.left()).max(40.0));
    }

    fn empty_state(&mut self, ui: &mut Ui, area: Rect, title: &str, msg: &str) {
        let p = ui.painter();
        let c = area.center_top() + vec2(0.0, (area.height() * 0.28).clamp(60.0, 160.0));
        let ib = Rect::from_center_size(c, vec2(54.0, 54.0));
        p.rect_filled(ib, 16, rgba(0x6366f1, 0.12));
        p.rect_stroke(ib, 16, Stroke::new(1.0_f32, rgba(0x818cf8, 0.3)), StrokeKind::Inside);
        icons::paint(ui, icons::PHOTO, Rect::from_center_size(c, vec2(26.0, 26.0)), INDIGO2);
        p.text(c + vec2(0.0, 48.0), Align2::CENTER_CENTER, title, sb(14.0), TEXT);
        let g = rich(ui, &[(msg, f(12.5), MUTED)], 420.0);
        let gw = g.size().x;
        p.galley(c + vec2(-gw / 2.0, 64.0), g, MUTED);
    }

    fn gallery_ui(&mut self, ui: &mut Ui, area: Rect) {
        if self.media.items.is_empty() {
            if self.media.loading {
                self.empty_state(ui, area, "Looking for photos and videos", "Reading the scan for pictures and video files.");
            } else {
                self.empty_state(ui, area, "No photos or videos here", "Your own photos, videos and screenshots show up here. Switch to All media to see every picture and video file in this scan.");
            }
            return;
        }
        let chips_r = Rect::from_min_size(area.min + vec2(12.0, 10.0), vec2(area.width() - 24.0, 28.0));
        let mut chips = hchild(ui, chips_r);
        chips.spacing_mut().item_spacing.x = 6.0;
        for (k, (flt, label)) in FILTERS.iter().enumerate() {
            let count = self.media.counts[k];
            let pending = matches!(flt, MFilter::Blurry | MFilter::NoDate) && !self.media.indexed;
            if count == 0 && *flt != MFilter::All && self.media.filter != *flt && !pending {
                continue;
            }
            if chip(&mut chips, Id::new(("mchip", k)), label, if pending { None } else { Some(count) }, self.media.filter == *flt).clicked() {
                self.media.filter = *flt;
                self.media.jump = Some(0.0);
            }
        }
        let list = Rect::from_min_max(pos2(area.left(), chips_r.bottom() + 8.0), area.max);
        let timeline = !self.media.by_folder && self.media.sections.len() > 1;
        let rail_w = if timeline { 58.0 } else { 0.0 };
        let grid = Rect::from_min_max(pos2(list.left() + 12.0, list.top()), pos2(list.right() - rail_w - 4.0, list.bottom()));
        if self.media.sections.is_empty() {
            let msg = match self.media.filter {
                MFilter::Blurry => "No blurry photos found. Nice and sharp.",
                MFilter::NoDate => "Every photo here has a date taken.",
                _ => "Nothing matches this filter.",
            };
            self.empty_state(ui, list, "Nothing here", msg);
            return;
        }
        let ts_base = ZOOM[self.media.zoom];
        let content_w = grid.width() - 14.0;
        let cols = (((content_w + GAP) / (ts_base + GAP)).floor() as usize).max(1);
        let ts = (content_w - GAP * (cols as f32 - 1.0)) / cols as f32;
        let ppp = ui.ctx().pixels_per_point();
        let px = (((ts * ppp).ceil() as u32).div_ceil(64) * 64).clamp(64, 512);
        let mut offsets = Vec::with_capacity(self.media.sections.len());
        let mut y = 4.0;
        for s in &self.media.sections {
            offsets.push(y);
            let rows = s.items.len().div_ceil(cols);
            y += HEAD_H + rows as f32 * (ts + GAP) + SECTION_PAD;
        }
        let total_h = y;
        let mut c = child(ui, grid);
        let mut sa = egui::ScrollArea::vertical().id_salt(("gallery", self.media.by_folder, self.media.filter as u8)).auto_shrink([false, false]);
        if let Some(j) = self.media.jump.take() {
            sa = sa.vertical_scroll_offset(j);
        }
        let mut hits: Vec<(usize, usize, TileHit)> = Vec::new();
        let mut tiles: Vec<(usize, usize, usize, Rect)> = Vec::new();
        let out = sa.show_viewport(&mut c, |ui, vp| {
            let origin = ui.max_rect().min;
            ui.set_height(total_h);
            ui.set_width(content_w);
            for (si, s) in self.media.sections.iter().enumerate() {
                let sy = offsets[si];
                let rows = s.items.len().div_ceil(cols);
                let sh = HEAD_H + rows as f32 * (ts + GAP);
                if sy + sh < vp.top() || sy > vp.bottom() {
                    continue;
                }
                let hp = ui.painter();
                let hy = origin.y + sy;
                hp.text(pos2(origin.x + 2.0, hy + 14.0), Align2::LEFT_CENTER, &s.title, sb(15.0), TEXT);
                text(ui, hp, pos2(origin.x + 2.0, hy + 33.0), Align2::LEFT_CENTER, &s.sub, f(11.5), DIM, content_w - 4.0);
                let first_row = ((vp.top() - sy - HEAD_H) / (ts + GAP)).floor().max(0.0) as usize;
                let last_row = (((vp.bottom() - sy - HEAD_H) / (ts + GAP)).ceil().max(0.0) as usize).min(rows);
                for row in first_row..last_row {
                    for col in 0..cols {
                        let k = row * cols + col;
                        let Some(&idx) = s.items.get(k) else { break };
                        let r = Rect::from_min_size(pos2(origin.x + col as f32 * (ts + GAP), hy + HEAD_H + row as f32 * (ts + GAP)), vec2(ts, ts));
                        tiles.push((si, k, idx, r));
                    }
                }
            }
            let any = !self.media.sel.is_empty();
            for (si, k, idx, r) in tiles.drain(..) {
                let on = self.media.sel.contains(&self.media.items[idx].id);
                let hit = self.media_tile(ui, r, idx, px, on, INDIGO, any);
                hits.push((si, k, hit));
            }
        });
        let flat_pos = |media: &crate::gallery::MediaState, si: usize, k: usize| -> usize { media.sections[..si].iter().map(|s| s.items.len()).sum::<usize>() + k };
        for (si, k, hit) in hits {
            let idx = self.media.sections[si].items[k];
            let id = self.media.items[idx].id;
            let pos = flat_pos(&self.media, si, k);
            match hit {
                TileHit::None => {}
                TileHit::Toggle => {
                    let shift = ui.ctx().input(|i| i.modifiers.shift);
                    if shift {
                        if let Some(a) = self.media.anchor {
                            let flat = self.media.flat();
                            let (lo, hi) = (a.min(pos), a.max(pos));
                            for &i in &flat[lo..=hi.min(flat.len() - 1)] {
                                self.media.sel.insert(self.media.items[i].id);
                            }
                        } else {
                            self.media.sel.insert(id);
                        }
                    } else if !self.media.sel.remove(&id) {
                        self.media.sel.insert(id);
                    }
                    self.media.anchor = Some(pos);
                }
                TileHit::Click => {
                    if self.media.sel.is_empty() {
                        self.media.order = self.media.flat();
                        self.media.lightbox = Some(pos);
                    } else if !self.media.sel.remove(&id) {
                        self.media.sel.insert(id);
                        self.media.anchor = Some(pos);
                    }
                }
                TileHit::Double => {
                    self.media.order = self.media.flat();
                    self.media.lightbox = Some(pos);
                }
                TileHit::Context(p) => self.node_menu(p, id),
            }
        }
        if timeline {
            let rail = Rect::from_min_max(pos2(grid.right() + 4.0, list.top() + 4.0), pos2(list.right() - 6.0, list.bottom() - 8.0));
            self.timeline_ui(ui, rail, &offsets, total_h, out.state.offset.y, out.inner_rect.height());
        }
    }

    fn timeline_ui(&mut self, ui: &mut Ui, rail: Rect, offsets: &[f32], total_h: f32, scroll: f32, view_h: f32) {
        let p = ui.painter().clone();
        let resp = ui.interact(rail, Id::new("mtimeline"), Sense::click_and_drag());
        let max_scroll = (total_h - view_h).max(1.0);
        let track = rail.shrink2(vec2(0.0, 6.0));
        let y_of = |sy: f32| track.top() + (sy / max_scroll).clamp(0.0, 1.0) * track.height();
        let line_x = track.right() - 8.0;
        p.line_segment([pos2(line_x, track.top()), pos2(line_x, track.bottom())], Stroke::new(1.0_f32, line()));
        let mut last_year = i32::MIN;
        let mut last_label_y = f32::MIN;
        for (si, s) in self.media.sections.iter().enumerate() {
            let y = y_of(offsets[si]);
            p.line_segment([pos2(line_x - 3.0, y), pos2(line_x, y)], Stroke::new(1.0_f32, white(0.18)));
            if s.year != last_year && y - last_label_y >= 16.0 {
                let label = if s.year == 0 { "?".to_string() } else { s.year.to_string() };
                p.text(pos2(line_x - 7.0, y), Align2::RIGHT_CENTER, label, monob(10.5), MUTED);
                p.circle_filled(pos2(line_x, y), 2.5, INDIGO2);
                last_label_y = y;
            }
            last_year = s.year;
        }
        let thumb_top = y_of(scroll);
        let thumb_h = (view_h / total_h.max(1.0) * track.height()).clamp(14.0, track.height());
        let tr = Rect::from_min_size(pos2(line_x - 2.0, thumb_top.min(track.bottom() - thumb_h)), vec2(4.0, thumb_h));
        prism_bar(&p, tr, 2.0);
        if let Some(hp) = resp.hover_pos() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            let frac = ((hp.y - track.top()) / track.height()).clamp(0.0, 1.0);
            let target = frac * max_scroll;
            let si = offsets.iter().rposition(|&o| o <= target + 1.0).unwrap_or(0);
            let label = &self.media.sections[si].title;
            let w = text_w(ui, label, sb(11.5)) + 18.0;
            let lr = Rect::from_min_size(pos2(rail.left() - w - 6.0, hp.y - 12.0), vec2(w, 24.0));
            let lp = ui.ctx().layer_painter(egui::LayerId::new(Order::Tooltip, Id::new("tl-tip")));
            shadow(&lp, lr, 8, 16, 6, Color32::from_black_alpha(160));
            lp.rect_filled(lr, 8, rgba(0x182030, 0.98));
            lp.rect_stroke(lr, 8, Stroke::new(1.0_f32, line2()), StrokeKind::Inside);
            lp.text(lr.center(), Align2::CENTER_CENTER, label, sb(11.5), TEXT);
            if resp.clicked() || resp.dragged() {
                self.media.jump = Some(offsets[si].max(0.0));
            }
        }
    }

    fn similar_ui(&mut self, ui: &mut Ui, area: Rect) {
        if !self.media.indexed {
            let msg = self.status_line().unwrap_or_default();
            let total = self.media.items.len();
            if total == 0 && !self.media.loading {
                self.empty_state(ui, area, "No photos or videos here", "Scan a drive or folder that holds pictures to look for duplicates.");
                return;
            }
            let job = self.media.job.clone();
            let frac = job.as_ref().map(|j| {
                let (d, t) = (j.done.load(Ordering::Relaxed), j.total.load(Ordering::Relaxed));
                if t == 0 { 0.0 } else { d as f32 / t as f32 }
            }).unwrap_or(0.0);
            self.empty_state(ui, area, "Comparing your photos and videos", "PrismDisk looks at what each picture shows, so copies that were resized, re-compressed, converted or renamed by chat apps are found too. Results are cached, so the next time is much faster.");
            let c = area.center_top() + vec2(0.0, (area.height() * 0.28).clamp(60.0, 160.0) + 132.0);
            let bar = Rect::from_center_size(c, vec2(320.0, 6.0));
            let p = ui.painter();
            p.rect_filled(bar, 3, white(0.07));
            prism_bar(p, Rect::from_min_size(bar.min, vec2((320.0 * frac).max(6.0), 6.0)), 3.0);
            p.text(c + vec2(0.0, 20.0), Align2::CENTER_TOP, msg, f(11.5), DIM);
            return;
        }
        if self.media.groups.is_empty() {
            self.empty_state(ui, area, "No duplicates found", "None of these photos or videos look like copies of each other.");
            return;
        }
        let bar_r = Rect::from_min_size(area.min + vec2(12.0, 10.0), vec2(area.width() - 24.0, 28.0));
        let mut bar = hchild(ui, bar_r);
        bar.spacing_mut().item_spacing.x = 6.0;
        let kinds = [(None, "All groups"), (Some(Match::Exact), "Exact copies"), (Some(Match::Same), "Same, re-saved"), (Some(Match::Similar), "Similar")];
        for (k, (kind, label)) in kinds.iter().enumerate() {
            let n = self.media.groups.iter().filter(|g| kind.map(|x| g.kind == x).unwrap_or(true)).count();
            if n == 0 && kind.is_some() {
                continue;
            }
            if chip(&mut bar, Id::new(("mkind", k)), label, Some(n), self.media.group_filter == *kind).clicked() {
                self.media.group_filter = *kind;
                self.media.jump = Some(0.0);
            }
        }
        let mut right = ui.new_child(egui::UiBuilder::new().max_rect(bar_r).layout(egui::Layout::right_to_left(egui::Align::Center)));
        right.spacing_mut().item_spacing.x = 6.0;
        if btn(&mut right, "Mark all extras", Some(icons::CHECK), Btn::Accent, true, true).on_hover_text("Marks every copy except the best one in exact and re-saved groups. Similar groups are left for you to decide.").clicked() {
            self.media.auto_mark();
        }

        let list = Rect::from_min_max(pos2(area.left() + 12.0, bar_r.bottom() + 8.0), pos2(area.right() - 6.0, area.bottom()));
        let content_w = list.width() - 14.0;
        let cols = (((content_w - 24.0 + 10.0) / (SIM_TILE + 10.0)).floor() as usize).max(1);
        let groups = self.media.visible_groups();
        let mut offsets = Vec::with_capacity(groups.len());
        let mut y = 0.0;
        for &g in &groups {
            offsets.push(y);
            let rows = self.media.groups[g].items.len().div_ceil(cols);
            y += 44.0 + rows as f32 * (SIM_TILE + SIM_META_H + 10.0) + 6.0 + 10.0;
        }
        let total_h = y;
        let ppp = ui.ctx().pixels_per_point();
        let px = (((SIM_TILE * ppp).ceil() as u32).div_ceil(64) * 64).clamp(64, 512);
        let mut c = child(ui, list);
        let mut sa = egui::ScrollArea::vertical().id_salt(("similar", self.media.group_filter.map(|k| k as u8))).auto_shrink([false, false]);
        if let Some(j) = self.media.jump.take() {
            sa = sa.vertical_scroll_offset(j);
        }
        enum SA {
            Toggle(u32),
            Preview(usize, usize),
            Keep(usize),
            Unmark(usize),
            Ctx(egui::Pos2, u32),
        }
        let mut acts: Vec<SA> = Vec::new();
        sa.show_viewport(&mut c, |ui, vp| {
            let origin = ui.max_rect().min;
            ui.set_height(total_h);
            ui.set_width(content_w);
            for (gi, &g) in groups.iter().enumerate() {
                let gy = offsets[gi];
                let grp = self.media.groups[g].clone();
                let rows = grp.items.len().div_ceil(cols);
                let gh = 44.0 + rows as f32 * (SIM_TILE + SIM_META_H + 10.0) + 6.0;
                if gy + gh < vp.top() || gy > vp.bottom() {
                    continue;
                }
                let card = Rect::from_min_size(origin + vec2(0.0, gy), vec2(content_w, gh));
                let p = ui.painter().clone();
                p.rect_filled(card, 13, white(0.018));
                p.rect_stroke(card, 13, Stroke::new(1.0_f32, line()), StrokeKind::Inside);
                let (badge_c, badge_fg) = match grp.kind {
                    Match::Exact => (rgba(0x6366f1, 0.2), hex(0xc7d2fe)),
                    Match::Same => (rgba(0x0ea5e9, 0.18), hex(0x7dd3fc)),
                    Match::Similar => (rgba(0xf59e0b, 0.16), hex(0xfcd34d)),
                };
                let label = grp.kind.label();
                let bw = text_w(ui, label, sb(11.0)) + 18.0;
                let br = Rect::from_min_size(card.min + vec2(12.0, 11.0), vec2(bw, 22.0));
                p.rect_filled(br, 99, badge_c);
                p.text(br.center(), Align2::CENTER_CENTER, label, sb(11.0), badge_fg);
                let kind_word = if grp.media == Kind::Video { "videos" } else { "photos" };
                let info = rich(ui, &[(&format!("{} {}  ·  ", grp.items.len(), kind_word), f(12.0), MUTED), (&fmt_bytes(grp.wasted), monob(12.0), hex(0xfca5a5)), (" reclaimable", f(12.0), MUTED)], 600.0);
                p.galley(pos2(br.right() + 10.0, br.center().y - info.size().y / 2.0), info, MUTED);
                let mut hdr = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(card.min + vec2(12.0, 6.0), pos2(card.right() - 12.0, card.top() + 38.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
                hdr.spacing_mut().item_spacing.x = 6.0;
                let any_marked = grp.items.iter().any(|&i| self.media.marked.contains(&self.media.items[i].id));
                if any_marked && btn(&mut hdr, "Unmark", None, Btn::Ghost, true, true).clicked() {
                    acts.push(SA::Unmark(g));
                }
                if btn(&mut hdr, "Keep best only", None, Btn::Ghost, true, true).clicked() {
                    acts.push(SA::Keep(g));
                }
                for (k, &idx) in grp.items.iter().enumerate() {
                    let (row, col) = (k / cols, k % cols);
                    let tr = Rect::from_min_size(card.min + vec2(12.0 + col as f32 * (SIM_TILE + 10.0), 44.0 + row as f32 * (SIM_TILE + SIM_META_H + 10.0)), vec2(SIM_TILE, SIM_TILE));
                    let it_id = self.media.items[idx].id;
                    let marked = self.media.marked.contains(&it_id);
                    let hit = self.media_tile(ui, tr, idx, px, marked, ROSE, true);
                    let p = ui.painter();
                    if idx == grp.keep {
                        let kr = Rect::from_min_size(pos2(tr.right() - 50.0, tr.top() + 8.0), vec2(42.0, 18.0));
                        p.rect_filled(kr, 99, EMERALD);
                        p.text(kr.center(), Align2::CENTER_CENTER, "BEST", sb(9.5), hex(0x04130d));
                    }
                    let it = &self.media.items[idx];
                    let m = it.meta.as_ref();
                    let dims = m.filter(|m| m.w > 0).map(|m| format!("{}×{}", m.w, m.h)).unwrap_or_default();
                    let l1 = if dims.is_empty() { fmt_bytes(it.size) } else { format!("{dims} · {}", fmt_bytes(it.size)) };
                    let ty = tr.bottom() + 6.0;
                    text(ui, p, pos2(tr.left() + 1.0, ty), Align2::LEFT_TOP, &l1, monob(10.5), if marked { hex(0xfda4af) } else { TEXT2 }, SIM_TILE);
                    let mut l2 = fmt_date(it.date_ms());
                    if let Some(m) = m {
                        if m.exif {
                            l2.push_str(" · EXIF");
                        } else if it.kind == Kind::Image {
                            l2.push_str(" · no EXIF");
                        }
                    }
                    text(ui, p, pos2(tr.left() + 1.0, ty + 16.0), Align2::LEFT_TOP, &l2, f(10.5), MUTED, SIM_TILE);
                    text_left_elide(ui, p, Rect::from_min_size(pos2(tr.left() + 1.0, ty + 31.0), vec2(SIM_TILE, 14.0)), &it.path, mono(10.0), DIM);
                    match hit {
                        TileHit::Click | TileHit::Toggle => acts.push(SA::Toggle(it_id)),
                        TileHit::Double => acts.push(SA::Preview(g, k)),
                        TileHit::Context(pos) => acts.push(SA::Ctx(pos, it_id)),
                        TileHit::None => {}
                    }
                }
            }
        });
        for a in acts {
            match a {
                SA::Toggle(id) => {
                    if !self.media.marked.remove(&id) {
                        self.media.marked.insert(id);
                    }
                }
                SA::Preview(g, k) => {
                    self.media.order = self.media.groups[g].items.clone();
                    self.media.lightbox = Some(k);
                }
                SA::Keep(g) => {
                    let grp = self.media.groups[g].clone();
                    for &i in &grp.items {
                        let id = self.media.items[i].id;
                        if i == grp.keep {
                            self.media.marked.remove(&id);
                        } else {
                            self.media.marked.insert(id);
                        }
                    }
                }
                SA::Unmark(g) => {
                    for &i in &self.media.groups[g].items.clone() {
                        self.media.marked.remove(&self.media.items[i].id);
                    }
                }
                SA::Ctx(pos, id) => self.node_menu(pos, id),
            }
        }
    }

    pub fn lightbox_ui(&mut self, ctx: &egui::Context) {
        let Some(pos) = self.media.lightbox else { return };
        let Some(&idx) = self.media.order.get(pos) else {
            self.media.lightbox = None;
            return;
        };
        if idx >= self.media.items.len() {
            self.media.lightbox = None;
            return;
        }
        let it = self.media.items[idx].clone();
        let screen = ctx.screen_rect();
        let ppp = ctx.pixels_per_point();
        let mut action: Option<&'static str> = None;
        let mut goto: Option<usize> = None;
        egui::Area::new(Id::new("lightbox")).order(Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
            ui.interact(screen, Id::new("lb-block"), Sense::click_and_drag());
            let p = ui.painter().clone();
            p.rect_filled(screen, 0, rgba(0x05070b, 0.985));
            let top = Rect::from_min_size(screen.min, vec2(screen.width(), 60.0));
            let strip_h = if self.media.order.len() > 1 { 74.0 } else { 0.0 };
            let stage = Rect::from_min_max(pos2(screen.left() + 70.0, top.bottom() + 6.0), pos2(screen.right() - 70.0, screen.bottom() - strip_h - 12.0));
            let image = it.kind == Kind::Image;
            let big_px = ((stage.width().max(stage.height()) * ppp).ceil() as u32).clamp(512, 2560);
            let big_px = (big_px / 256 + 1) * 256;
            let tex = match self.thumbs.get(it.id, &it.path, big_px, image, true) {
                Some(Some(t)) => Some(t),
                _ => self.thumbs.any_small(it.id),
            };
            let loading = self.thumbs.peek(it.id, big_px, true).is_none();
            if let Some(t) = &tex {
                let ts = t.size_vec2();
                let natural = it.meta.as_ref().filter(|m| m.w > 0).map(|m| vec2(m.w as f32, m.h as f32) / ppp).unwrap_or(ts / ppp);
                let scale = (stage.width() / ts.x).min(stage.height() / ts.y).min(natural.x / ts.x.max(1.0));
                let r = Rect::from_center_size(stage.center(), ts * scale);
                shadow(&p, r, 6, 40, 16, Color32::from_black_alpha(200));
                paint_tex(ui, t, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), 6.0);
                if it.kind == Kind::Video {
                    let pr = Rect::from_center_size(r.center(), vec2(74.0, 74.0));
                    let resp = ui.interact(pr, Id::new("lb-play"), Sense::click());
                    p.circle_filled(pr.center(), 37.0, Color32::from_black_alpha(if resp.hovered() { 200 } else { 150 }));
                    p.circle_stroke(pr.center(), 37.0, Stroke::new(2.0_f32, white(0.85)));
                    icons::paint(ui, icons::PLAY, Rect::from_center_size(pr.center() + vec2(3.0, 0.0), vec2(30.0, 30.0)), Color32::WHITE);
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        action = Some("open");
                    }
                }
            } else {
                icons::paint(ui, if image { icons::PHOTO } else { icons::VIDEO }, Rect::from_center_size(stage.center(), vec2(48.0, 48.0)), DIM);
            }
            if loading && image {
                let t = ui.ctx().input(|i| i.time) as f32;
                spinner_ring(&p, pos2(stage.right() - 14.0, stage.top() + 14.0), 9.0, 6.5, t * 5.0);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
            }

            vfade(&p, top.expand2(vec2(0.0, 30.0)).translate(vec2(0.0, 15.0)), Color32::from_black_alpha(160), Color32::TRANSPARENT);
            p.text(pos2(top.left() + 22.0, top.top() + 19.0), Align2::LEFT_CENTER, &it.name, sb(14.5), TEXT);
            let mut facts: Vec<String> = Vec::new();
            facts.push(fmt_date(it.date_ms()));
            if let Some(m) = &it.meta {
                if m.w > 0 {
                    facts.push(format!("{} × {}", m.w, m.h));
                }
                if m.duration_ms > 0 {
                    facts.push(fmt_duration(m.duration_ms));
                }
                if !m.camera.is_empty() {
                    facts.push(m.camera.clone());
                }
            }
            facts.push(fmt_bytes(it.size));
            facts.push(format!("{} of {}", pos + 1, self.media.order.len()));
            text(ui, &p, pos2(top.left() + 22.0, top.top() + 40.0), Align2::LEFT_CENTER, &facts.join("   ·   "), f(11.5), MUTED, top.width() * 0.6);
            let mut row = ui.new_child(egui::UiBuilder::new().max_rect(top.shrink2(vec2(18.0, 14.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
            row.spacing_mut().item_spacing.x = 6.0;
            if btn(&mut row, "", Some(icons::X), Btn::Ghost, false, true).on_hover_text("Close (Esc)").clicked() {
                action = Some("close");
            }
            if btn(&mut row, "Recycle", Some(icons::TRASH), Btn::Danger, true, true).clicked() {
                action = Some("recycle");
            }
            if btn(&mut row, "Show in Explorer", Some(icons::FOLDER_O), Btn::Ghost, true, true).clicked() {
                action = Some("reveal");
            }
            if btn(&mut row, "Open", Some(icons::OPEN), Btn::Accent, true, true).clicked() {
                action = Some("open");
            }

            let n = self.media.order.len();
            for (dir, icon, enabled) in [(-1i32, icons::CHEV_LEFT, pos > 0), (1, icons::CHEV, pos + 1 < n)] {
                if !enabled {
                    continue;
                }
                let cx = if dir < 0 { screen.left() + 36.0 } else { screen.right() - 36.0 };
                let r = Rect::from_center_size(pos2(cx, stage.center().y), vec2(44.0, 44.0));
                let resp = ui.interact(r, Id::new(("lb-nav", dir)), Sense::click());
                p.circle_filled(r.center(), 22.0, if resp.hovered() { white(0.14) } else { white(0.06) });
                p.circle_stroke(r.center(), 22.0, Stroke::new(1.0_f32, line2()));
                icons::paint(ui, icon, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), TEXT);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    goto = Some((pos as i32 + dir) as usize);
                }
            }

            if strip_h > 0.0 {
                let th = 54.0;
                let strip = Rect::from_min_max(pos2(screen.left(), screen.bottom() - strip_h), screen.max);
                let fit = ((strip.width() - 40.0) / (th + 6.0)).floor().max(1.0) as usize;
                let half = fit / 2;
                let start = pos.saturating_sub(half).min(n.saturating_sub(fit));
                let end = (start + fit).min(n);
                let total_w = (end - start) as f32 * (th + 6.0) - 6.0;
                let mut x = strip.center().x - total_w / 2.0;
                let spx = (((th * ppp).ceil() as u32).div_ceil(64) * 64).clamp(64, 256);
                for k in start..end {
                    let i = self.media.order[k];
                    let (iid, ipath, iimg) = (self.media.items[i].id, self.media.items[i].path.clone(), self.media.items[i].kind == Kind::Image);
                    let r = Rect::from_min_size(pos2(x, strip.top() + 6.0), vec2(th, th));
                    let resp = ui.interact(r, Id::new(("lb-strip", k)), Sense::click());
                    p.rect_filled(r, 7, SURFACE2);
                    if let Some(Some(t)) = self.thumbs.get(iid, &ipath, spx, iimg, false) {
                        paint_tex(ui, &t, r, cover_uv(t.size_vec2(), r.size()), 7.0);
                    }
                    if k == pos {
                        p.rect_stroke(r.expand(2.0), 9, Stroke::new(2.0_f32, INDIGO2), StrokeKind::Outside);
                    } else {
                        p.rect_filled(r, 7, Color32::from_black_alpha(if resp.hovered() { 40 } else { 110 }));
                    }
                    if resp.clicked() {
                        goto = Some(k);
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    x += th + 6.0;
                }
            }
        });
        if let Some(g) = goto {
            self.media.lightbox = Some(g);
        }
        match action {
            Some("close") => self.media.lightbox = None,
            Some("open") => {
                if let Err(e) = crate::sys::open(&it.path) {
                    self.toast("Could not open", e.to_string(), true);
                }
            }
            Some("reveal") => {
                if let Err(e) = crate::sys::reveal(&it.path) {
                    self.toast("Could not open Explorer", e.to_string(), true);
                }
            }
            Some("recycle") => self.confirm_delete(vec![it.id], false),
            _ => {}
        }
    }
}
