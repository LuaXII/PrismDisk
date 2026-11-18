#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[path = "../../app/src/icons.rs"]
#[allow(dead_code)]
mod icons;
mod install;
#[path = "../../app/src/theme.rs"]
#[allow(dead_code)]
mod theme;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CursorIcon, Id, Mesh, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TextEdit, Ui, UiBuilder, Vec2,
    ViewportCommand,
};
use egui::text::{LayoutJob, TextFormat, TextWrapping};

use theme::*;

const W: f32 = 900.0;
const H: f32 = 600.0;
const RAIL: f32 = 290.0;
const PANEL: Color32 = hex(0x10151f);

const ORB: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 150 150"><defs><linearGradient id="og" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#6366f1"/><stop offset=".5" stop-color="#0ea5e9"/><stop offset="1" stop-color="#34d399"/></linearGradient></defs><circle cx="75" cy="75" r="62" fill="none" stroke="#ffffff" stroke-opacity=".06" stroke-width="16"/><circle cx="75" cy="75" r="62" fill="none" stroke="url(#og)" stroke-width="16" stroke-dasharray="120 14 70 14 46 14 31 40"/><circle cx="75" cy="75" r="42" fill="none" stroke="#818cf8" stroke-opacity=".35" stroke-width="5" stroke-dasharray="40 10 22 10 60 30"/></svg>"##;
const ORB_SMALL: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 150 150"><defs><linearGradient id="og" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#6366f1"/><stop offset=".5" stop-color="#0ea5e9"/><stop offset="1" stop-color="#34d399"/></linearGradient></defs><circle cx="75" cy="75" r="62" fill="none" stroke="#ffffff" stroke-opacity=".06" stroke-width="16"/><circle cx="75" cy="75" r="62" fill="none" stroke="url(#og)" stroke-width="16" stroke-dasharray="120 14 70 14 46 14 31 40"/></svg>"##;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Welcome,
    License,
    Options,
    Progress,
    Finish,
    UConfirm,
}

#[derive(Default)]
struct Job {
    pct: f64,
    label: String,
    stage: u8,
    result: Option<Result<Option<PathBuf>, String>>,
}

struct Setup {
    uninstall: bool,
    flow: Vec<Page>,
    idx: usize,
    changed_at: f64,
    back: bool,
    existing: Option<install::Existing>,
    required: u64,
    payload_ok: bool,
    dir: String,
    free: Option<u64>,
    free_for: String,
    accept: bool,
    opts: [bool; 4],
    keep_data: bool,
    launch: bool,
    job: Option<Arc<Mutex<Job>>>,
    shown_pct: f64,
    error: Option<String>,
    finished: bool,
    removed_dir: Option<PathBuf>,
    veil: bool,
    pick: Option<Receiver<Option<PathBuf>>>,
}

impl Setup {
    fn new(cc: &eframe::CreationContext<'_>, uninstall: bool) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        theme::style(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        style_window(cc);
        let existing = install::existing_install();
        let dir = existing.as_ref().map(|e| e.dir.clone()).unwrap_or_else(|| install::default_dir().display().to_string());
        let required = install::PAYLOAD_LEN + std::env::current_exe().ok().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0);
        Setup {
            uninstall,
            flow: if uninstall { vec![Page::UConfirm, Page::Progress, Page::Finish] } else { vec![Page::Welcome, Page::License, Page::Options, Page::Progress, Page::Finish] },
            idx: 0,
            changed_at: 0.0,
            back: false,
            existing,
            required,
            payload_ok: !install::PAYLOAD.is_empty(),
            dir,
            free: None,
            free_for: String::new(),
            accept: false,
            opts: [true; 4],
            keep_data: false,
            launch: true,
            job: None,
            shown_pct: 0.0,
            error: None,
            finished: false,
            removed_dir: None,
            veil: false,
            pick: None,
        }
    }

    fn page(&self) -> Page {
        self.flow[self.idx]
    }

    fn go(&mut self, ctx: &egui::Context, idx: usize) {
        self.back = idx < self.idx;
        self.idx = idx;
        self.changed_at = ctx.input(|i| i.time);
    }

    fn busy(&self) -> bool {
        self.job.as_ref().map(|j| j.lock().unwrap().result.is_none()).unwrap_or(false)
    }

    fn start_job(&mut self, ctx: &egui::Context) {
        let job = Arc::new(Mutex::new(Job { label: "Starting".into(), ..Default::default() }));
        self.job = Some(job.clone());
        self.error = None;
        self.shown_pct = 0.0;
        let ctx2 = ctx.clone();
        let uninstall = self.uninstall;
        let keep = self.keep_data;
        let o = install::Options { dir: self.dir.trim().to_string(), start_menu: self.opts[0], desktop: self.opts[1], context_menu: self.opts[2], run_as_admin: self.opts[3] };
        std::thread::spawn(move || {
            let report = |pct: f64, label: &str, stage: u8| {
                let mut j = job.lock().unwrap();
                j.pct = pct;
                j.label = label.to_string();
                j.stage = stage;
                ctx2.request_repaint();
            };
            let r = if uninstall { install::do_uninstall(keep, &report).map(Some) } else { install::do_install(&o, &report).map(|_| None) };
            job.lock().unwrap().result = Some(r);
            ctx2.request_repaint();
        });
    }

    fn quit(&mut self, ctx: &egui::Context) {
        if let Some(dir) = self.removed_dir.take() {
            install::schedule_self_delete(&dir);
        }
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    fn next(&mut self, ctx: &egui::Context) {
        match self.page() {
            Page::Options | Page::UConfirm => {
                self.go(ctx, self.idx + 1);
                self.start_job(ctx);
            }
            Page::Progress => {
                if !self.busy() && !self.finished {
                    self.start_job(ctx);
                }
            }
            Page::Finish => {
                if !self.uninstall && self.launch {
                    let exe = PathBuf::from(self.dir.trim()).join(install::APP_EXE);
                    let _ = std::process::Command::new("explorer.exe").arg(exe).spawn();
                }
                self.quit(ctx);
            }
            _ => self.go(ctx, self.idx + 1),
        }
    }

    fn next_enabled(&self) -> bool {
        match self.page() {
            Page::Welcome => self.payload_ok,
            Page::License => self.accept,
            Page::Options => self.free.map(|f| f as f64 > self.required as f64 * 1.2).unwrap_or(false) && is_abs_path(&self.dir),
            Page::Progress => !self.busy() && !self.finished,
            Page::Finish | Page::UConfirm => true,
        }
    }

    fn close_request(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        if self.finished || self.page() == Page::Finish {
            self.quit(ctx);
        } else {
            self.veil = true;
        }
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn wrap_galley(ui: &Ui, text: &str, font: egui::FontId, color: Color32, width: f32, line_h: Option<f32>) -> Arc<egui::Galley> {
    let mut fmt = TextFormat::simple(font, color);
    fmt.line_height = line_h;
    let mut job = LayoutJob::single_section(text.to_owned(), fmt);
    job.wrap = TextWrapping { max_width: width, ..Default::default() };
    ui.fonts(|f| f.layout_job(job))
}

fn spaced(ui: &Ui, s: &str, font: egui::FontId, color: Color32, spacing: f32) -> Arc<egui::Galley> {
    let mut fmt = TextFormat::simple(font, color);
    fmt.extra_letter_spacing = spacing;
    ui.fonts(|f| f.layout_job(LayoutJob::single_section(s.to_owned(), fmt)))
}

fn eyebrow(ui: &Ui, p: &egui::Painter, pos: Pos2, s: &str) -> f32 {
    let n = s.chars().count().max(2);
    let mut job = LayoutJob::default();
    for (i, ch) in s.chars().enumerate() {
        let mut fmt = TextFormat::simple(monob(10.5), prism(i as f32 / (n - 1) as f32));
        fmt.extra_letter_spacing = 1.5;
        job.append(&ch.to_string(), 0.0, fmt);
    }
    let g = ui.fonts(|f| f.layout_job(job));
    let h = g.size().y;
    p.galley(pos, g, TEXT);
    h
}

fn svg(ui: &Ui, uri: &'static str, svg: &'static str, rect: Rect, angle: f32) {
    egui::Image::new(egui::ImageSource::Bytes { uri: uri.into(), bytes: egui::load::Bytes::Static(svg.as_bytes()) })
        .rotate(angle, Vec2::splat(0.5))
        .paint_at(ui, rect);
}

fn reflow(text: &str) -> String {
    let t = text.replace("\r\n", "\n");
    let mut out = String::new();
    for para in t.split("\n\n") {
        let mut cur = String::new();
        for l in para.lines() {
            let tl = l.trim_start();
            let rule = !l.is_empty() && l.chars().all(|ch| ch == '=' || ch == '-');
            if cur.is_empty() || tl.starts_with("- ") || rule {
                if !cur.is_empty() {
                    out.push_str(&cur);
                    out.push('\n');
                }
                cur = l.to_string();
            } else {
                cur.push(' ');
                cur.push_str(tl);
            }
        }
        out.push_str(&cur);
        out.push_str("\n\n");
    }
    out.trim_end().to_string()
}

fn field_label(ui: &Ui, p: &egui::Painter, pos: Pos2, s: &str) {
    let g = spaced(ui, s, sb(11.0), DIM, 0.9);
    p.galley(pos, g, DIM);
}

fn info_icon(p: &egui::Painter, c: Pos2, col: Color32) {
    p.circle_stroke(c, 8.0, Stroke::new(2.0_f32, col));
    p.line_segment([c + vec2(0.0, -4.0), c + vec2(0.0, 1.0)], Stroke::new(2.2_f32, col));
    p.circle_filled(c + vec2(0.0, 4.2), 1.2, col);
}

fn check_mark(p: &egui::Painter, c: Pos2, s: f32, col: Color32, w: f32) {
    p.add(Shape::line(vec![c + vec2(-4.5, 0.2) * s, c + vec2(-1.3, 3.2) * s, c + vec2(4.6, -3.2) * s], Stroke::new(w, col)));
}

fn big_btn(ui: &mut Ui, rect: Rect, id: &str, label: &str, kind: u8, enabled: bool) -> bool {
    let resp = ui.interact(rect, Id::new(id), if enabled { Sense::click() } else { Sense::hover() });
    let hov = resp.hovered() && enabled;
    let p = ui.painter();
    let a = if enabled { 1.0 } else { 0.35 };
    let r = if resp.is_pointer_button_down_on() { rect.shrink(0.6) } else { rect };
    match kind {
        0 => {
            if enabled {
                shadow(p, r.shrink(6.0), 11, 26, 10, if hov { rgba(0x0ea5e9, 0.8) } else { rgba(0x6366f1, 0.95) });
            }
            let shift = if hov { 0.25 } else { 0.0 };
            grad_rrect(p, r, 11.0, |pt| with_alpha_pm(prism(((pt.x - r.left()) + (pt.y - r.top())) / (r.width() + r.height()) * 0.75 + shift), a));
            p.text(r.center(), Align2::CENTER_CENTER, label, sb(13.0), with_alpha_pm(Color32::WHITE, a));
        }
        1 => {
            shadow(p, r.shrink(6.0), 11, 26, 10, rgba(0xf43f5e, 0.9));
            grad_rrect(p, r, 11.0, |pt| lerp(hex(0xf43f5e), hex(0xe11d48), ((pt.x - r.left()) + (pt.y - r.top())) / (r.width() + r.height())));
            p.text(r.center(), Align2::CENTER_CENTER, label, sb(13.0), Color32::WHITE);
        }
        _ => {
            if hov {
                p.rect_filled(r, 11, white(0.05));
            }
            p.text(r.center(), Align2::CENTER_CENTER, label, sb(13.0), with_alpha_pm(if hov { TEXT } else { MUTED }, a));
        }
    }
    if hov {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn checkbox_row(ui: &mut Ui, pos: Pos2, id: &str, label: &str, on: &mut bool) {
    let tw = text_w(ui, label, sb(13.0));
    let rect = Rect::from_min_size(pos, vec2(20.0 + 11.0 + tw, 22.0));
    let resp = ui.interact(rect, Id::new(id), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let p = ui.painter();
    let bx = Rect::from_min_size(pos2(pos.x, rect.center().y - 10.0), vec2(20.0, 20.0));
    if *on {
        shadow(p, bx, 7, 14, 0, rgba(0x6366f1, 0.6));
        p.rect_filled(bx, 7, INDIGO);
        check_mark(p, bx.center(), 0.95, Color32::WHITE, 2.0);
    } else {
        p.rect_stroke(bx, 7, Stroke::new(1.5_f32, DIM), StrokeKind::Inside);
    }
    p.text(pos2(bx.right() + 11.0, rect.center().y), Align2::LEFT_CENTER, label, sb(13.0), TEXT);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
}

fn notice(ui: &Ui, p: &egui::Painter, rect: Rect, text: &str, err: bool) {
    let (bg, st, fg, ic) = if err { (rgba(0xf43f5e, 0.08), rgba(0xf43f5e, 0.3), hex(0xfecdd3), ROSE) } else { (rgba(0x0ea5e9, 0.08), rgba(0x0ea5e9, 0.25), hex(0xbae6fd), SKY) };
    p.rect_filled(rect, 12, bg);
    p.rect_stroke(rect, 12, Stroke::new(1.0_f32, st), StrokeKind::Inside);
    info_icon(p, pos2(rect.left() + 14.0 + 9.0, rect.center().y), ic);
    let g = wrap_galley(ui, text, f(12.0), fg, rect.width() - 57.0, None);
    p.galley(pos2(rect.left() + 43.0, rect.center().y - g.size().y / 2.0), g, fg);
}

fn switch(p: &egui::Painter, r: Rect, k: f32) {
    if k > 0.01 {
        shadow(p, r, 11, 14, 0, with_alpha_pm(rgba(0x6366f1, 0.5), k));
    }
    p.rect_filled(r, 11, lerp(white(0.12), INDIGO, k));
    let kk = if k > 0.0 && k < 1.0 { k + (k * std::f32::consts::PI).sin() * 0.08 } else { k };
    let c = pos2(r.left() + 3.0 + 7.5 + 15.0 * kk, r.center().y);
    p.circle_filled(c, 7.5, lerp(TEXT2, Color32::WHITE, k));
}

fn ring(p: &egui::Painter, c: Pos2, r: f32, w: f32, frac: f32) {
    p.circle_stroke(c, r, Stroke::new(w, white(0.06)));
    let frac = frac.clamp(0.0, 1.0);
    if frac <= 0.0 {
        return;
    }
    let seg = (frac * 160.0).ceil().max(2.0) as u32;
    let mut m = Mesh::default();
    let start = -std::f32::consts::FRAC_PI_2;
    let col_at = |q: Pos2| prism((((q.x - (c.x - r)) + (q.y - (c.y - r))) / (4.0 * r)).clamp(0.0, 1.0));
    for k in 0..=seg {
        let a = start + frac * std::f32::consts::TAU * k as f32 / seg as f32;
        let d = vec2(a.cos(), a.sin());
        let col = col_at(c + d * r);
        m.colored_vertex(c + d * (r + w / 2.0), col);
        m.colored_vertex(c + d * (r - w / 2.0), col);
    }
    for k in 0..seg {
        let i = k * 2;
        m.add_triangle(i, i + 1, i + 2);
        m.add_triangle(i + 1, i + 3, i + 2);
    }
    p.add(Shape::mesh(m));
    for a in [start, start + frac * std::f32::consts::TAU] {
        let q = c + vec2(a.cos(), a.sin()) * r;
        p.circle_filled(q, w / 2.0, col_at(q));
    }
}

fn is_abs_path(s: &str) -> bool {
    let b = s.trim().as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

fn fmt_bytes(n: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (n as f64, 0);
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else if v >= 100.0 { format!("{v:.0} {}", U[i]) } else { format!("{v:.1} {}", U[i]) }
}

impl eframe::App for Setup {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.043, 0.055, 0.078, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut finished_dir: Option<Option<PathBuf>> = None;
        if let Some(job) = &self.job {
            let j = job.lock().unwrap();
            match &j.result {
                Some(Ok(dir)) if !self.finished => finished_dir = Some(dir.clone()),
                Some(Err(e)) if self.error.is_none() => self.error = Some(e.clone()),
                _ => {}
            }
        }
        if let Some(dir) = finished_dir {
            self.finished = true;
            self.removed_dir = dir;
            let i = self.flow.iter().position(|p| *p == Page::Finish).unwrap();
            self.go(ctx, i);
        }
        if let Some(rx) = &self.pick {
            if let Ok(r) = rx.try_recv() {
                if let Some(p) = r {
                    let is_app = p.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case(install::APP)).unwrap_or(false);
                    self.dir = if is_app { p } else { p.join(install::APP) }.display().to_string();
                }
                self.pick = None;
            }
        }
        let (esc, enter) = ctx.input(|i| (i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::Enter)));
        if esc {
            if self.veil {
                self.veil = false;
            } else {
                self.close_request(ctx);
            }
        }
        if enter && !self.veil && !ctx.wants_keyboard_input() && self.next_enabled() {
            self.next(ctx);
        }

        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ctx, |ui| {
            let full = ui.max_rect();
            self.rail(ui, Rect::from_min_size(full.min, vec2(RAIL, full.height())));
            self.main(ui, Rect::from_min_max(pos2(full.left() + RAIL, full.top()), full.max));
        });
        if self.veil {
            self.veil_ui(ctx);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
}

impl Setup {
    fn rail(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, 0, hex(0x090c12));
        let t = ui.input(|i| i.time) as f32;
        let wave = |period: f32| (1.0 - (t / period * std::f32::consts::TAU).cos()) / 2.0;
        let (a, b_, c) = (wave(14.0), wave(17.0), wave(19.0));
        let o = rect.min;
        for (center, r, col) in [
            (o + vec2(40.0 + 60.0 * a, 70.0 + 50.0 * a), 150.0 * (1.0 + 0.15 * a), rgba(0x6366f1, 0.55)),
            (o + vec2(290.0 - 70.0 * b_, 270.0 + 60.0 * b_), 130.0 * (1.0 - 0.1 * b_), rgba(0x0ea5e9, 0.42)),
            (o + vec2(80.0 + 70.0 * c, rect.height() - 50.0 * c), 140.0 * (1.0 + 0.1 * c), rgba(0x34d399, 0.32)),
        ] {
            glow(&p, center, Vec2::splat(r * 1.45), with_alpha_pm(col, 0.75));
            glow(&p, center, Vec2::splat(r * 0.9), with_alpha_pm(col, 0.8));
        }
        let mut grain = Mesh::default();
        for gy in (0..240).step_by(3) {
            let col = white(0.03 * (1.0 - gy as f32 / 240.0));
            for gx in (0..290).step_by(3) {
                let i = grain.vertices.len() as u32;
                let q = o + vec2(gx as f32, gy as f32);
                grain.colored_vertex(q, col);
                grain.colored_vertex(q + vec2(1.0, 0.0), col);
                grain.colored_vertex(q + vec2(0.0, 1.0), col);
                grain.colored_vertex(q + vec2(1.0, 1.0), col);
                grain.add_triangle(i, i + 1, i + 2);
                grain.add_triangle(i + 1, i + 3, i + 2);
            }
        }
        p.add(Shape::mesh(grain));
        p.vline(rect.right() - 0.5, rect.y_range(), Stroke::new(1.0_f32, line()));

        let mark = Rect::from_min_size(o + vec2(28.0, 34.0), vec2(46.0, 46.0));
        shadow(&p, mark.shrink(4.0), 14, 30, 10, rgba(0x6366f1, 0.9));
        prism_rrect(&p, mark, 14.0);
        icons::paint(ui, icons::LOGO, Rect::from_center_size(mark.center(), vec2(26.0, 26.0)), Color32::WHITE);
        p.text(pos2(mark.right() + 13.0, mark.center().y + 1.0), Align2::LEFT_BOTTOM, "PrismDisk", b(19.0), TEXT);
        p.text(
            pos2(mark.right() + 13.0, mark.center().y + 2.0),
            Align2::LEFT_TOP,
            format!("{} · v{}", if self.uninstall { "Uninstaller" } else { "Setup" }, install::VERSION),
            f(11.5),
            MUTED,
        );

        let top = o.y + 124.0;
        let n = self.flow.len();
        let line_x = o.x + 28.0 + 13.0;
        let span = 52.0 * (n as f32 - 1.0);
        p.rect_filled(Rect::from_min_max(pos2(line_x, top + 14.0), pos2(line_x + 2.0, top + 14.0 + span + 24.0)), 2, white(0.07));
        let since = (ui.input(|i| i.time) - self.changed_at) as f32;
        let prev = if self.back { (self.idx + 1).min(n - 1) } else { self.idx.saturating_sub(1) };
        let k = if self.changed_at == 0.0 { 1.0 } else { ease(since / 0.5) };
        let fill_idx = prev as f32 + (self.idx as f32 - prev as f32) * k;
        let fill_h = if n > 1 { fill_idx / (n - 1) as f32 * span } else { 0.0 };
        if fill_h > 0.5 {
            let fr = Rect::from_min_size(pos2(line_x, top + 14.0), vec2(2.0, fill_h));
            grad_rrect(&p, fr, 1.0, |pt| prism((pt.y - fr.top()) / span.max(1.0)));
        }
        for (i, pg) in self.flow.iter().enumerate() {
            let (title, sub) = match pg {
                Page::Welcome => ("Welcome", "Get started"),
                Page::License => ("License", "MIT, free forever"),
                Page::Options => ("Options", "Location & extras"),
                Page::Progress if self.uninstall => ("Remove", "Deleting files"),
                Page::Progress => ("Install", "Copying files"),
                Page::Finish if self.uninstall => ("Finish", "All cleaned up"),
                Page::Finish => ("Finish", "Ready to go"),
                Page::UConfirm => ("Confirm", "Review removal"),
            };
            let cy = top + 26.0 + 52.0 * i as f32;
            let c = pos2(o.x + 28.0 + 14.0, cy);
            let done = i < self.idx;
            let on = i == self.idx;
            if done {
                p.circle_filled(c, 14.0, INDIGO);
                check_mark(&p, c, 0.95, Color32::WHITE, 2.0);
            } else if on {
                glow(&p, c, Vec2::splat(30.0), rgba(0x6366f1, 0.5));
                p.circle_filled(c, 19.0, rgba(0x6366f1, 0.15));
                p.circle_filled(c, 13.0, hex(0x0f141d));
                p.circle_filled(c, 13.0, rgba(0x6366f1, 0.25));
                p.circle_stroke(c, 13.0, Stroke::new(2.0_f32, INDIGO2));
                p.text(c, Align2::CENTER_CENTER, (i + 1).to_string(), monob(11.0), Color32::WHITE);
            } else {
                p.circle_filled(c, 14.0, hex(0x0f141d));
                p.circle_stroke(c, 13.0, Stroke::new(2.0_f32, white(0.1)));
                p.text(c, Align2::CENTER_CENTER, (i + 1).to_string(), monob(11.0), DIM);
            }
            let col = if on { TEXT } else if done { MUTED } else { DIM };
            p.text(pos2(o.x + 70.0, cy + 1.0), Align2::LEFT_BOTTOM, title, sb(13.0), col);
            p.text(pos2(o.x + 70.0, cy + 2.0), Align2::LEFT_TOP, sub, f(11.0), DIM);
        }
        p.text(pos2(o.x + 28.0, rect.bottom() - 24.0), Align2::LEFT_BOTTOM, "© 2026 PrismDisk · MIT License", f(11.0), DIM);
    }

    fn main(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, 0, PANEL);
        glow(&p, rect.right_top(), vec2(rect.width() * 1.2, rect.height() * 0.7), rgba(0x6366f1, 0.07));

        let tb = Rect::from_min_size(rect.min, vec2(rect.width(), 44.0));
        let close_r = Rect::from_min_size(pos2(tb.right() - 8.0 - 40.0, tb.top() + 7.0), vec2(40.0, 30.0));
        let min_r = close_r.translate(vec2(-40.0, 0.0));
        let drag = ui.interact(Rect::from_min_max(tb.min, pos2(min_r.left(), tb.bottom())), Id::new("drag"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        for (r, is_close) in [(min_r, false), (close_r, true)] {
            let resp = ui.interact(r, Id::new(("wbtn", is_close)), Sense::click());
            let col = if resp.hovered() { if is_close { Color32::WHITE } else { TEXT } } else { MUTED };
            if resp.hovered() {
                p.rect_filled(r, 8, if is_close { hex(0xe81123) } else { white(0.07) });
            }
            let c = r.center();
            if is_close {
                p.line_segment([c + vec2(-4.5, -4.5), c + vec2(4.5, 4.5)], Stroke::new(1.4_f32, col));
                p.line_segment([c + vec2(4.5, -4.5), c + vec2(-4.5, 4.5)], Stroke::new(1.4_f32, col));
            } else {
                p.line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], Stroke::new(1.4_f32, col));
            }
            if resp.clicked() {
                if is_close {
                    self.close_request(ui.ctx());
                } else {
                    ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
                }
            }
        }

        let foot = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 76.0), rect.max);
        let since = (ui.input(|i| i.time) - self.changed_at) as f32;
        let k = if self.changed_at == 0.0 { 1.0 } else { ease(since / 0.4) };
        if k < 1.0 {
            ui.ctx().request_repaint();
        }
        let dx = (1.0 - k) * if self.back { -26.0 } else { 26.0 };
        let content = Rect::from_min_max(pos2(rect.left() + 44.0 + dx, tb.bottom() + 4.0), pos2(rect.right() - 44.0 + dx, foot.top()));
        let mut page_ui = ui.new_child(UiBuilder::new().max_rect(content).id_salt(("page", self.idx)));
        page_ui.set_opacity(k);
        match self.page() {
            Page::Welcome => self.welcome(&mut page_ui, content),
            Page::License => self.license(&mut page_ui, content),
            Page::Options => self.options(&mut page_ui, content),
            Page::Progress => self.progress(&mut page_ui, content),
            Page::Finish => self.finish(&mut page_ui, content),
            Page::UConfirm => self.uconfirm(&mut page_ui, content),
        }

        let fin = Rect::from_min_max(pos2(foot.left() + 44.0, foot.top() + 16.0), pos2(foot.right() - 44.0, foot.bottom() - 22.0));
        let page = self.page();
        let (label, kind) = match page {
            Page::Welcome => (if self.existing.is_some() { "Update" } else { "Get started" }, 0),
            Page::License => ("Next", 0),
            Page::Options => (if self.existing.is_some() { "Update now" } else { "Install" }, 0),
            Page::Progress => (if self.busy() { "Working…" } else { "Retry" }, 0),
            Page::Finish => ("Finish", 0),
            Page::UConfirm => ("Uninstall", 1),
        };
        let nw = text_w(ui, label, sb(13.0)) + 40.0;
        let next_r = Rect::from_min_size(pos2(fin.right() - nw, fin.top()), vec2(nw, 38.0));
        let enabled = self.next_enabled();
        if big_btn(ui, next_r, "next", label, kind, enabled) {
            self.next(ui.ctx());
        }
        let show_back = self.idx > 0 && page != Page::Finish && (page != Page::Progress || (self.error.is_some() && !self.uninstall));
        if show_back {
            let bw = text_w(ui, "Back", sb(13.0)) + 40.0;
            let back_r = Rect::from_min_size(pos2(next_r.left() - 10.0 - bw, fin.top()), vec2(bw, 38.0));
            if big_btn(ui, back_r, "back", "Back", 2, true) {
                let target = if page == Page::Progress { self.flow.iter().position(|p| *p == Page::Options).unwrap_or(0) } else { self.idx - 1 };
                self.error = None;
                self.job = None;
                self.go(ui.ctx(), target);
            }
        }
        if page == Page::Options {
            p.text(pos2(fin.left(), next_r.center().y), Align2::LEFT_CENTER, "Setup runs with administrator rights.", f(11.5), DIM);
        }
    }

    fn header(&self, ui: &Ui, p: &egui::Painter, x: f32, y: f32, eyebrow_s: &str, title: &str, lead: &str, width: f32) -> f32 {
        let mut y = y;
        y += eyebrow(ui, p, pos2(x, y), eyebrow_s) + 8.0;
        p.text(pos2(x, y), Align2::LEFT_TOP, title, b(22.0), TEXT);
        y += 36.0;
        let g = wrap_galley(ui, lead, f(13.5), MUTED, width, None);
        let h = g.size().y;
        p.galley(pos2(x, y), g, MUTED);
        y + h
    }

    fn welcome(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.painter().clone();
        let t = ui.input(|i| i.time) as f32;
        let top = r.top() + 6.0;
        let orb = Rect::from_min_size(pos2(r.left(), top), vec2(150.0, 150.0));
        glow(&p, orb.center() + vec2(0.0, 12.0), Vec2::splat(95.0), rgba(0x6366f1, 0.35));
        svg(ui, "bytes://orb.svg", ORB, orb, t / 26.0 * std::f32::consts::TAU);
        p.circle_filled(orb.center(), 30.0, hex(0x0e131d));
        glow(&p, orb.center() + vec2(-8.0, -9.0), Vec2::splat(26.0), rgba(0x2a3350, 0.9));
        p.circle_stroke(orb.center(), 30.0, Stroke::new(1.0_f32, white(0.06)));
        let ver = format!("v{}", install::VERSION.split('.').take(2).collect::<Vec<_>>().join("."));
        p.text(orb.center(), Align2::CENTER_CENTER, ver, monob(15.0), TEXT);

        let cx = r.left() + 180.0;
        let mut y = top + 18.0;
        y += eyebrow(ui, &p, pos2(cx, y), "WELCOME") + 8.0;
        let g = wrap_galley(ui, "See exactly where\nyour space went.", b(30.0), TEXT, r.right() - cx, Some(34.5));
        let h = g.size().y;
        p.galley(pos2(cx, y), g, TEXT);
        y += h + 8.0;
        let g = wrap_galley(ui, "PrismDisk maps a whole drive in seconds by reading the NTFS Master File Table directly.", f(13.5), MUTED, r.right() - cx, None);
        p.galley(pos2(cx, y), g, MUTED);

        let fy = top + 150.0 + 18.0;
        let cw = (r.width() - 24.0) / 3.0;
        let feats: [(icons::Icon, Color32, Color32, &str, &str); 3] = [
            (icons::BOLT, rgba(0x6366f1, 0.15), hex(0xa5b4fc), "Instant scans", "Millions of files in seconds, with no slow folder crawling."),
            (icons::GRID, rgba(0x0ea5e9, 0.15), hex(0x7dd3fc), "Treemap & sunburst", "Spot the giants at a glance, then drill in."),
            (icons::SPARKLE, rgba(0x10b981, 0.15), hex(0x6ee7b7), "Smart cleanup", "Finds caches, temp files and duplicates safely."),
        ];
        let texts: Vec<_> = feats.iter().map(|ft| wrap_galley(ui, ft.4, f(11.8), MUTED, cw - 30.0, Some(17.0))).collect();
        let fh = texts.iter().map(|g| g.size().y).fold(0.0, f32::max) + 15.0 + 34.0 + 10.0 + 20.0 + 15.0;
        for (i, (ft, g)) in feats.iter().zip(texts).enumerate() {
            let cr = Rect::from_min_size(pos2(r.left() + i as f32 * (cw + 12.0), fy), vec2(cw, fh));
            grad_rrect(&p, cr, 15.0, |pt| lerp(white(0.035), white(0.01), (pt.y - cr.top()) / cr.height()));
            p.rect_stroke(cr, 15, Stroke::new(1.0_f32, line()), StrokeKind::Inside);
            let ib = Rect::from_min_size(cr.min + vec2(15.0, 15.0), vec2(34.0, 34.0));
            p.rect_filled(ib, 10, ft.1);
            icons::paint(ui, ft.0, Rect::from_center_size(ib.center(), vec2(18.0, 18.0)), ft.2);
            p.text(pos2(ib.left(), ib.bottom() + 10.0), Align2::LEFT_TOP, ft.3, sb(13.0), TEXT);
            p.galley(pos2(ib.left(), ib.bottom() + 30.0), g, MUTED);
        }
        let mut ny = fy + fh + 16.0;
        if let Some(e) = &self.existing {
            let msg = format!("PrismDisk {} is already installed in {}. Setup will update it in place.", e.version, e.dir);
            let gh = wrap_galley(ui, &msg, f(12.0), TEXT, r.width() - 57.0, None).size().y;
            notice(ui, &p, Rect::from_min_size(pos2(r.left(), ny), vec2(r.width(), gh + 22.0)), &msg, false);
            ny += gh + 30.0;
        }
        if !self.payload_ok {
            notice(ui, &p, Rect::from_min_size(pos2(r.left(), ny), vec2(r.width(), 44.0)), "This setup was built without the app payload. Rebuild it with build.ps1.", true);
        }
    }

    fn license(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.painter().clone();
        let y = self.header(ui, &p, r.left(), r.top(), "STEP 2 OF 5", "License agreement", "PrismDisk is free and open source under the MIT license.", r.width());
        let check_y = r.bottom() - 22.0;
        let box_r = Rect::from_min_max(pos2(r.left(), y + 16.0), pos2(r.right(), check_y - 14.0));
        p.rect_filled(box_r, 14, hex(0x0b0f17));
        p.rect_stroke(box_r, 14, Stroke::new(1.0_f32, line()), StrokeKind::Inside);
        let mut c = ui.new_child(UiBuilder::new().max_rect(box_r.shrink2(vec2(20.0, 16.0))).layout(egui::Layout::top_down(Align::Min)));
        egui::ScrollArea::vertical().id_salt("lic").auto_shrink([false, false]).show(&mut c, |ui| {
            let g = wrap_galley(ui, &reflow(install::LICENSE), mono(11.8), hex(0xb6c0d1), ui.available_width() - 10.0, Some(19.5));
            let (rr, _) = ui.allocate_exact_size(g.size(), Sense::hover());
            ui.painter().galley(rr.min, g, TEXT);
        });
        let mut accept = self.accept;
        checkbox_row(ui, pos2(r.left(), check_y - 11.0), "accept", "I accept the license agreement", &mut accept);
        self.accept = accept;
    }

    fn options(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.painter().clone();
        if self.free_for != self.dir {
            self.free_for = self.dir.clone();
            self.free = install::free_space(std::path::Path::new(self.dir.trim()));
        }
        let mut y = self.header(ui, &p, r.left(), r.top(), "STEP 3 OF 5", "Make it yours", "Choose where PrismDisk lives and how it fits into Windows.", r.width());
        y += 20.0;
        field_label(ui, &p, pos2(r.left(), y), "INSTALL LOCATION");
        y += 22.0;
        let bw = text_w(ui, "Browse…", sb(13.0)) + 40.0;
        let inp = Rect::from_min_size(pos2(r.left(), y), vec2(r.width() - bw - 8.0, 40.0));
        let focused = ui.ctx().memory(|m| m.has_focus(Id::new("dir")));
        if focused {
            p.rect_stroke(inp.expand(4.0), 14, Stroke::new(4.0_f32, rgba(0x6366f1, 0.15)), StrokeKind::Inside);
        }
        p.rect_filled(inp, 11, hex(0x0b0f17));
        p.rect_stroke(inp, 11, Stroke::new(1.0_f32, if focused { rgba(0x818cf8, 0.7) } else { line2() }), StrokeKind::Inside);
        icons::paint(ui, icons::FOLDER, Rect::from_center_size(pos2(inp.left() + 20.5, inp.center().y), vec2(17.0, 17.0)), FOLDER);
        ui.put(
            Rect::from_min_max(pos2(inp.left() + 38.0, inp.top() + 2.0), pos2(inp.right() - 12.0, inp.bottom() - 2.0)),
            TextEdit::singleline(&mut self.dir).id(Id::new("dir")).frame(false).font(mono(12.5)).text_color(TEXT).vertical_align(Align::Center).margin(vec2(0.0, 0.0)),
        );
        let br = Rect::from_min_size(pos2(inp.right() + 8.0, y), vec2(bw, 40.0));
        let bresp = ui.interact(br, Id::new("browse"), Sense::click());
        p.rect_filled(br, 11, white(if bresp.hovered() { 0.08 } else { 0.04 }));
        p.rect_stroke(br, 11, Stroke::new(1.0_f32, line2()), StrokeKind::Inside);
        p.text(br.center(), Align2::CENTER_CENTER, "Browse…", sb(13.0), TEXT);
        if bresp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if bresp.clicked() && self.pick.is_none() {
            let (tx, rx) = channel();
            self.pick = Some(rx);
            let cur = PathBuf::from(self.dir.trim());
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let mut d = rfd::FileDialog::new().set_title("Choose where to install PrismDisk");
                if let Some(par) = cur.parent().filter(|p| p.exists()) {
                    d = d.set_directory(par);
                }
                let _ = tx.send(d.pick_folder());
                ctx.request_repaint();
            });
        }
        y += 50.0;
        let nr = p.text(pos2(r.left(), y + 8.0), Align2::LEFT_CENTER, "Needs ", f(12.0), MUTED);
        let nr2 = p.text(pos2(nr.right(), y + 8.0), Align2::LEFT_CENTER, fmt_bytes(self.required), monob(12.0), TEXT);
        let (avail, ok) = match self.free {
            Some(fr) => (format!("{} free", fmt_bytes(fr)), fr as f64 > self.required as f64 * 1.2),
            None => ("Drive not found".to_string(), false),
        };
        let ar = p.text(pos2(r.right(), y + 8.0), Align2::RIGHT_CENTER, &avail, f(12.0), if ok { hex(0x6ee7b7) } else { hex(0xfda4af) });
        let meter = Rect::from_min_max(pos2(nr2.right() + 12.0, y + 5.0), pos2(ar.left() - 12.0, y + 11.0));
        p.rect_filled(meter, 9, white(0.07));
        if let Some(fr) = self.free {
            let frac = ((self.required as f32 / fr as f32) * 50.0).clamp(0.03, 1.0);
            let m = Rect::from_min_size(meter.min, vec2(meter.width() * frac, 6.0));
            grad_rrect(&p, m, 3.0, |pt| prism((pt.x - m.left()) / m.width().max(1.0)));
        }
        y += 36.0;
        field_label(ui, &p, pos2(r.left(), y), "OPTIONS");
        y += 22.0;
        let opts: [(&str, &str, bool); 4] = [
            ("Start menu shortcut", "Find PrismDisk from Start.", false),
            ("Desktop shortcut", "Quick launch from your desktop.", false),
            ("Explorer integration", "“Analyze with PrismDisk” when you right-click a folder or drive.", false),
            ("Always run as admin", "Needed for instant MFT scans. Windows asks once per launch.", true),
        ];
        let cw = (r.width() - 10.0) / 2.0;
        let descs: Vec<_> = opts.iter().map(|o| wrap_galley(ui, o.1, f(11.5), MUTED, cw - 76.0, Some(16.0))).collect();
        for row in 0..2 {
            let h = (0..2).map(|c| descs[row * 2 + c].size().y).fold(0.0, f32::max) + 46.0;
            for col in 0..2 {
                let i = row * 2 + col;
                let cr = Rect::from_min_size(pos2(r.left() + col as f32 * (cw + 10.0), y), vec2(cw, h));
                let mut on = self.opts[i];
                option_card(ui, &p, cr, i, opts[i].0, descs[i].clone(), opts[i].2, &mut on);
                self.opts[i] = on;
            }
            y += h + 10.0;
        }
    }

    fn progress(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.painter().clone();
        let (eye, title, lead) = if self.uninstall {
            ("UNINSTALLING", "Removing PrismDisk", "Cleaning everything up.")
        } else if self.error.is_some() {
            ("STEP 4 OF 5", "Something went wrong", "Nothing was changed that can't be retried.")
        } else {
            ("STEP 4 OF 5", "Installing PrismDisk", "Sit tight, this only takes a moment.")
        };
        let y = self.header(ui, &p, r.left(), r.top(), eye, title, lead, r.width());
        let (pct, label, stage) = self
            .job
            .as_ref()
            .map(|j| {
                let j = j.lock().unwrap();
                (j.pct, j.label.clone(), j.stage)
            })
            .unwrap_or((0.0, "Starting".into(), 0));
        self.shown_pct += (pct - self.shown_pct) * 0.25;
        if (pct - self.shown_pct).abs() < 0.2 {
            self.shown_pct = pct;
        }
        let area = Rect::from_min_max(pos2(r.left(), y), pos2(r.right(), r.bottom() - 20.0));
        let c = pos2(area.left() + 105.0, area.center().y);
        let t = ui.input(|i| i.time) as f32;
        let busy = self.busy();
        if busy {
            spinner_ring(&p, c, 113.0, 106.0, t * std::f32::consts::TAU / 2.4);
        }
        glow(&p, c, Vec2::splat(110.0), with_alpha_pm(rgba(0x6366f1, 0.16), (self.shown_pct / 100.0).min(1.0) as f32));
        ring(&p, c, 85.0, 12.0, (self.shown_pct / 100.0) as f32);
        let pr = p.text(c + vec2(-6.0, -6.0), Align2::CENTER_CENTER, format!("{}", self.shown_pct.round() as u32), b(40.0), TEXT);
        p.text(pos2(pr.right() + 1.0, pr.bottom() - 10.0), Align2::LEFT_BOTTOM, "%", b(18.0), MUTED);
        p.text(c + vec2(0.0, 26.0), Align2::CENTER_CENTER, &label, f(11.5), MUTED);

        let stages: [&str; 5] = if self.uninstall {
            ["Closing PrismDisk", "Removing program files", "Removing shortcuts", "Cleaning up the registry", "Settings & cache"]
        } else {
            ["Preparing install folder", "Extracting PrismDisk", "Installing uninstaller", "Creating shortcuts", "Registering with Windows"]
        };
        let sx = area.left() + 246.0;
        let total_h = 200.0 + if self.error.is_some() { 70.0 } else { 0.0 };
        let mut sy = c.y - total_h / 2.0;
        for (i, s) in stages.iter().enumerate() {
            let cy = sy + 20.0;
            let sc = pos2(sx + 11.0, cy);
            let done = (i as u8) < stage || self.finished;
            let on = i as u8 == stage && busy;
            if done {
                p.circle_filled(sc, 11.0, EMERALD);
                check_mark(&p, sc, 0.85, hex(0x04130d), 2.0);
            } else if on {
                p.circle_stroke(sc, 10.0, Stroke::new(2.0_f32, white(0.08)));
                let a0 = t * std::f32::consts::TAU / 0.8;
                let pts: Vec<Pos2> = (0..=16)
                    .map(|k| {
                        let a = a0 + k as f32 / 16.0 * 1.6;
                        sc + vec2(a.cos(), a.sin()) * 10.0
                    })
                    .collect();
                p.add(Shape::line(pts, Stroke::new(2.0_f32, INDIGO2)));
            } else {
                p.circle_stroke(sc, 10.0, Stroke::new(2.0_f32, white(0.12)));
            }
            p.text(pos2(sx + 34.0, cy), Align2::LEFT_CENTER, *s, sb(13.0), if on { TEXT } else if done { MUTED } else { DIM });
            sy += 40.0;
        }
        if let Some(e) = &self.error {
            notice(ui, &p, Rect::from_min_size(pos2(sx, sy + 10.0), vec2(area.right() - sx, 56.0)), e, true);
        }
    }

    fn finish(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.painter().clone();
        let since = (ui.input(|i| i.time) - self.changed_at) as f32;
        if since < 1.6 {
            ui.ctx().request_repaint();
        }
        let c = pos2(r.center().x, r.top() + 104.0);
        let td = ((since - 0.1) / 0.55).clamp(0.0, 1.0);
        let s = if td <= 0.0 { 0.0 } else { 1.0 + 2.70158 * (td - 1.0).powi(3) + 1.70158 * (td - 1.0).powi(2) };
        if s > 0.01 {
            let disc = Rect::from_center_size(c, Vec2::splat(120.0 * s));
            if self.uninstall {
                shadow(&p, disc.shrink(10.0), 60, 50, 18, rgba(0xf43f5e, 0.8));
                grad_rrect(&p, disc, 60.0 * s, |pt| lerp(hex(0xf43f5e), hex(0xbe123c), ((pt.x - disc.left()) + (pt.y - disc.top())) / (disc.width() * 2.0).max(1.0)));
            } else {
                shadow(&p, disc.shrink(10.0), 60, 50, 18, rgba(0x6366f1, 0.85));
                prism_rrect(&p, disc, 60.0 * s);
            }
        }
        let tc = ((since - 0.45) / 0.45).clamp(0.0, 1.0);
        if tc > 0.0 {
            let pts = [c + vec2(-22.0, 2.0), c + vec2(-7.0, 17.0), c + vec2(23.0, -16.0)];
            let l1 = (pts[1] - pts[0]).length();
            let l2 = (pts[2] - pts[1]).length();
            let len = (l1 + l2) * tc;
            let mut line = vec![pts[0]];
            if len <= l1 {
                line.push(pts[0] + (pts[1] - pts[0]) * (len / l1));
            } else {
                line.push(pts[1]);
                line.push(pts[1] + (pts[2] - pts[1]) * ((len - l1) / l2));
            }
            p.add(Shape::line(line.clone(), Stroke::new(9.0_f32, Color32::WHITE)));
            for q in line {
                p.circle_filled(q, 4.5, Color32::WHITE);
            }
        }
        let colors = [0x6366f1, 0x0ea5e9, 0x34d399, 0xa855f7, 0xf59e0b, 0xec4899];
        for i in 0..22 {
            let delay = 0.35 + (i as f32 * 0.37).fract() * 0.15;
            let tp = (since - delay).clamp(0.0, 1.0);
            if tp <= 0.0 || tp >= 1.0 {
                continue;
            }
            let e = 1.0 - (1.0 - tp).powi(3);
            let a = i as f32 / 22.0 * std::f32::consts::TAU + (i as f32 * 1.7).sin() * 0.3;
            let d = 80.0 + ((i * 37) % 60) as f32;
            let q = c + vec2(a.cos(), a.sin()) * d * e;
            p.rect_filled(Rect::from_center_size(q, Vec2::splat(8.0)), 3, with_alpha_pm(hex(colors[i % colors.len()]), 1.0 - tp));
        }
        let mut y = c.y + 82.0;
        let (eye, title, lead) = if self.uninstall {
            ("UNINSTALLED", "PrismDisk was removed", "Thanks for trying it out. The install folder is deleted when you close this window.")
        } else {
            ("ALL DONE", "PrismDisk is ready", "Find it in your Start menu, or launch it right now.")
        };
        let ew = spaced(ui, eye, monob(10.5), TEXT, 1.5).size().x;
        eyebrow(ui, &p, pos2(c.x - ew / 2.0, y), eye);
        y += 22.0;
        p.text(pos2(c.x, y), Align2::CENTER_TOP, title, b(30.0), TEXT);
        y += 44.0;
        let mut job = LayoutJob::single_section(lead.to_string(), TextFormat::simple(f(13.5), MUTED));
        job.wrap = TextWrapping { max_width: 500.0, ..Default::default() };
        job.halign = Align::Center;
        let g = ui.fonts(|fo| fo.layout_job(job));
        let gh = g.size().y;
        p.galley(pos2(c.x, y), g, MUTED);
        y += gh + 18.0;
        if !self.uninstall {
            let w = 31.0 + text_w(ui, "Launch PrismDisk now", sb(13.0));
            let mut launch = self.launch;
            checkbox_row(ui, pos2(c.x - w / 2.0, y), "launch", "Launch PrismDisk now", &mut launch);
            self.launch = launch;
        }
    }

    fn uconfirm(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.painter().clone();
        let t = ui.input(|i| i.time) as f32;
        let top = r.top() + 6.0;
        let orb = Rect::from_min_size(pos2(r.left(), top), vec2(128.0, 128.0));
        glow(&p, orb.center() + vec2(0.0, 10.0), Vec2::splat(80.0), rgba(0x6366f1, 0.3));
        svg(ui, "bytes://orb-small.svg", ORB_SMALL, orb, t / 26.0 * std::f32::consts::TAU);
        p.circle_filled(orb.center(), 25.0, hex(0x0e131d));
        glow(&p, orb.center() + vec2(-7.0, -8.0), Vec2::splat(22.0), rgba(0x2a3350, 0.9));
        let cx = r.left() + 150.0;
        let mut y = top + 10.0;
        y += eyebrow(ui, &p, pos2(cx, y), "UNINSTALL") + 8.0;
        p.text(pos2(cx, y), Align2::LEFT_TOP, "Remove PrismDisk?", b(27.0), TEXT);
        y += 38.0;
        let g = wrap_galley(ui, "Sorry to see you go. This removes the app, its shortcuts and Explorer integration. Your files are never touched.", f(13.5), MUTED, r.right() - cx, None);
        p.galley(pos2(cx, y), g, MUTED);
        let ny = top + 136.0;
        let text = match &self.existing {
            Some(e) => format!("Installed in {}", e.dir),
            None => "PrismDisk does not appear to be installed, so there is nothing to remove.".to_string(),
        };
        let nh = wrap_galley(ui, &text, f(12.0), TEXT, r.width() - 57.0, None).size().y + 22.0;
        notice(ui, &p, Rect::from_min_size(pos2(r.left(), ny), vec2(r.width(), nh)), &text, false);
        let desc = wrap_galley(ui, "Leave PrismDisk's preferences and cache so a reinstall picks up where you left off.", f(11.5), MUTED, r.width() - 76.0, Some(16.0));
        let cr = Rect::from_min_size(pos2(r.left(), ny + nh + 16.0), vec2(r.width(), desc.size().y + 46.0));
        let mut keep = self.keep_data;
        option_card(ui, &p, cr, 9, "Keep my settings", desc, false, &mut keep);
        self.keep_data = keep;
    }

    fn veil_ui(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let mut close = None;
        egui::Area::new(Id::new("veil")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            ui.interact(screen, Id::new("veil-block"), Sense::click());
            let p = ui.painter();
            p.rect_filled(screen, 0, rgba(0x05070b, 0.66));
            let d = Rect::from_center_size(screen.center(), vec2(380.0, 150.0));
            shadow(p, d, 18, 80, 40, Color32::from_black_alpha(230));
            grad_rrect(p, d, 18.0, |pt| lerp(SURFACE2, hex(0x151b27), (pt.y - d.top()) / d.height()));
            p.rect_stroke(d, 18, Stroke::new(1.0_f32, line2()), StrokeKind::Inside);
            let (title, body, yes) = if self.uninstall {
                ("Cancel uninstall?", "PrismDisk will stay installed.", "Exit")
            } else {
                ("Cancel setup?", "PrismDisk hasn't been installed yet. You can run setup again any time.", "Exit setup")
            };
            p.text(d.min + vec2(24.0, 24.0), Align2::LEFT_TOP, title, sb(16.0), TEXT);
            let g = wrap_galley(ui, body, f(12.5), MUTED, 332.0, None);
            p.galley(d.min + vec2(24.0, 50.0), g, MUTED);
            let yw = text_w(ui, yes, sb(13.0)) + 40.0;
            let yr = Rect::from_min_size(pos2(d.right() - 24.0 - yw, d.bottom() - 62.0), vec2(yw, 38.0));
            let nw = text_w(ui, "Keep going", sb(13.0)) + 40.0;
            let nr = Rect::from_min_size(pos2(yr.left() - 8.0 - nw, yr.top()), vec2(nw, 38.0));
            if big_btn(ui, nr, "dlg-no", "Keep going", 2, true) {
                close = Some(false);
            }
            if big_btn(ui, yr, "dlg-yes", yes, 1, true) {
                close = Some(true);
            }
        });
        match close {
            Some(true) => self.quit(ctx),
            Some(false) => self.veil = false,
            None => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn option_card(ui: &mut Ui, p: &egui::Painter, cr: Rect, i: usize, title: &str, desc: Arc<egui::Galley>, tag: bool, on: &mut bool) {
    let resp = ui.interact(cr, Id::new(("opt", i)), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let k = ui.ctx().animate_bool_with_time(Id::new(("optk", i)), *on, 0.22);
    p.rect_filled(cr, 13, lerp(if resp.hovered() { white(0.035) } else { white(0.02) }, rgba(0x6366f1, 0.07), k));
    p.rect_stroke(cr, 13, Stroke::new(1.0_f32, lerp(if resp.hovered() { line2() } else { line() }, rgba(0x818cf8, 0.35), k)), StrokeKind::Inside);
    switch(p, Rect::from_min_size(cr.min + vec2(14.0, 14.0), vec2(36.0, 21.0)), k);
    let x = cr.left() + 62.0;
    let tr = p.text(pos2(x, cr.top() + 13.0), Align2::LEFT_TOP, title, sb(12.8), TEXT);
    if tag {
        let tg = Rect::from_min_size(pos2(tr.right() + 6.0, tr.center().y - 7.5), vec2(text_w(ui, "FAST", monob(9.5)) + 12.0, 15.0));
        p.rect_filled(tg, 5, rgba(0x10b981, 0.15));
        p.text(tg.center(), Align2::CENTER_CENTER, "FAST", monob(9.5), hex(0x6ee7b7));
    }
    p.galley(pos2(x, tr.bottom() + 2.0), desc, MUTED);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
}

fn style_window(cc: &eframe::CreationContext<'_>) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows::Win32::Foundation::{BOOL, HWND};
        use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
        let Ok(h) = cc.window_handle() else { return };
        let RawWindowHandle::Win32(w) = h.as_raw() else { return };
        let hwnd = HWND(w.hwnd.get() as _);
        unsafe {
            let pref = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const _ as _, std::mem::size_of_val(&pref) as u32);
            let dark = BOOL(1);
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
        }
    }
}

fn silent(args: &[String], uninstall: bool) -> i32 {
    let noop = |_: f64, _: &str, _: u8| {};
    if uninstall {
        return match install::do_uninstall(args.iter().any(|a| a == "--keep-data"), &noop) {
            Ok(dir) => {
                install::schedule_self_delete(&dir);
                0
            }
            Err(_) => 1,
        };
    }
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let previous = if has("--update") { install::previous_options() } else { None };
    let mut o = previous.unwrap_or_else(|| install::Options {
        dir: install::existing_install().map(|e| e.dir).unwrap_or_else(|| install::default_dir().display().to_string()),
        desktop: !has("--no-desktop"),
        start_menu: true,
        context_menu: !has("--no-context-menu"),
        run_as_admin: !has("--no-admin"),
    });
    if let Some(dir) = args.iter().find_map(|a| a.strip_prefix("--dir=")) {
        o.dir = dir.to_string();
    }
    let ok = install::do_install(&o, &noop).is_ok();
    if ok && has("--launch") {
        install::launch_app(&o.dir);
    }
    if ok { 0 } else { 1 }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exe_name = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase())).unwrap_or_default();
    let uninstall = args.iter().any(|a| a == "--uninstall") || exe_name.starts_with("uninstall");
    if args.iter().any(|a| a == "--silent" || a == "/S") {
        std::process::exit(silent(&args, uninstall));
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(if uninstall { "Uninstall PrismDisk" } else { "PrismDisk Setup" })
        .with_inner_size([W, H])
        .with_resizable(false)
        .with_maximize_button(false)
        .with_decorations(false);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../app/icons/128x128@2x.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("PrismDisk Setup", options, Box::new(move |cc| Ok(Box::new(Setup::new(cc, uninstall)))))
}
