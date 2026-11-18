use std::sync::Arc;

use eframe::egui::{
    self, epaint, pos2, vec2, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Mesh, Painter, Pos2,
    Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};
use egui::text::{LayoutJob, TextFormat, TextWrapping};

use crate::icons::{self, Icon};

pub const fn hex(h: u32) -> Color32 {
    Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
}
pub fn css(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    let a = a.clamp(0.0, 1.0);
    let m = |c: u8| (c as f32 * a).round() as u8;
    Color32::from_rgba_premultiplied(m(r), m(g), m(b), (a * 255.0).round() as u8)
}
pub fn rgba(h: u32, a: f32) -> Color32 {
    css((h >> 16) as u8, (h >> 8) as u8, h as u8, a)
}
pub fn white(a: f32) -> Color32 {
    css(255, 255, 255, a)
}

pub const BG: Color32 = hex(0x0b0e14);
pub const BG2: Color32 = hex(0x0e121a);
pub const SURFACE: Color32 = hex(0x131822);
pub const SURFACE2: Color32 = hex(0x182030);
pub const TEXT: Color32 = hex(0xe6eaf2);
pub const TEXT2: Color32 = hex(0xcbd5e1);
pub const MUTED: Color32 = hex(0x94a3b8);
pub const DIM: Color32 = hex(0x64748b);
pub const INDIGO: Color32 = hex(0x6366f1);
pub const INDIGO2: Color32 = hex(0x818cf8);
pub const SKY: Color32 = hex(0x0ea5e9);
pub const EMERALD: Color32 = hex(0x10b981);
pub const MINT: Color32 = hex(0x34d399);
pub const AMBER: Color32 = hex(0xf59e0b);
pub const ROSE: Color32 = hex(0xf43f5e);
pub const FOLDER: Color32 = hex(0xfbbf24);

pub fn line() -> Color32 {
    white(0.07)
}
pub fn line2() -> Color32 {
    white(0.12)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

pub fn prism(t: f32) -> Color32 {
    if t < 0.55 { lerp(INDIGO, SKY, t / 0.55) } else { lerp(SKY, MINT, (t - 0.55) / 0.45) }
}

pub fn shade(c: Color32, f: f32) -> Color32 {
    let m = |v: u8| {
        let v = v as f32;
        (if f >= 0.0 { v + (255.0 - v) * f } else { v * (1.0 + f) }).clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

pub fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let (s, l) = (s / 100.0, l / 100.0);
    let a = s * l.min(1.0 - l);
    let f = |n: f32| {
        let k = (n + h / 30.0) % 12.0;
        let v = l - a * (k - 3.0).min(9.0 - k).min(1.0).max(-1.0);
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(f(0.0), f(8.0), f(4.0))
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let dir = std::path::PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into())).join("Fonts");
    let mut load = |name: &str, files: &[&str]| -> bool {
        for f in files {
            if let Ok(bytes) = std::fs::read(dir.join(f)) {
                defs.font_data.insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
                return true;
            }
        }
        false
    };
    let has_ui = load("ui", &["segoeui.ttf"]);
    let has_sb = load("ui-sb", &["seguisb.ttf", "segoeui.ttf"]);
    let has_b = load("ui-b", &["segoeuib.ttf", "seguisb.ttf"]);
    let has_mono = load("mono", &["CascadiaMono.ttf", "consola.ttf"]);
    let has_monob = load("mono-b", &["consolab.ttf", "CascadiaMono.ttf"]);
    let has_sym = load("sym", &["seguisym.ttf"]);

    let base_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let base_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let fam = |primary: Option<&str>, base: &Vec<String>| {
        let mut v: Vec<String> = primary.into_iter().map(str::to_owned).collect();
        if has_sym {
            v.push("sym".into());
        }
        v.extend(base.iter().cloned());
        v
    };
    defs.families.insert(FontFamily::Proportional, fam(has_ui.then_some("ui"), &base_prop));
    defs.families.insert(FontFamily::Name("sb".into()), fam(has_sb.then_some("ui-sb"), &base_prop));
    defs.families.insert(FontFamily::Name("b".into()), fam(has_b.then_some("ui-b"), &base_prop));
    defs.families.insert(FontFamily::Monospace, fam(has_mono.then_some("mono"), &base_mono));
    defs.families.insert(FontFamily::Name("mb".into()), fam(has_monob.then_some("mono-b"), &base_mono));
    ctx.set_fonts(defs);
}

pub fn f(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn sb(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("sb".into()))
}
pub fn b(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("b".into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}
pub fn monob(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("mb".into()))
}

pub fn style(ctx: &egui::Context) {
    ctx.style_mut(|s| {
        s.visuals = egui::Visuals::dark();
        s.visuals.panel_fill = BG;
        s.visuals.window_fill = SURFACE2;
        s.visuals.extreme_bg_color = BG;
        s.visuals.selection.bg_fill = rgba(0x6366f1, 0.45);
        s.visuals.selection.stroke = Stroke::new(1.0_f32, INDIGO2);
        s.visuals.text_cursor.stroke = Stroke::new(1.5_f32, INDIGO2);
        s.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        s.visuals.override_text_color = Some(TEXT);
        s.spacing.scroll = egui::style::ScrollStyle {
            bar_width: 6.0,
            floating: true,
            floating_width: 6.0,
            floating_allocated_width: 0.0,
            foreground_color: false,
            bar_inner_margin: 3.0,
            bar_outer_margin: 2.0,
            dormant_background_opacity: 0.0,
            active_background_opacity: 0.0,
            interact_background_opacity: 0.0,
            dormant_handle_opacity: 0.35,
            active_handle_opacity: 0.55,
            interact_handle_opacity: 0.75,
            ..s.spacing.scroll
        };
        s.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, white(0.14));
        s.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, white(0.25));
        s.visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, white(0.35));
        s.interaction.tooltip_delay = 0.35;
    });
}

pub fn galley(ui: &Ui, text: &str, font: FontId, color: Color32, max_w: f32) -> Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap = TextWrapping::truncate_at_width(max_w.max(1.0));
    ui.fonts(|f| f.layout_job(job))
}

pub fn text_w(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_owned(), font, TEXT).size().x)
}

pub fn text(ui: &Ui, painter: &Painter, pos: Pos2, align: Align2, s: &str, font: FontId, color: Color32, max_w: f32) -> Rect {
    let g = galley(ui, s, font, color, max_w);
    let rect = align.anchor_size(pos, g.size());
    painter.galley(rect.min, g, color);
    rect
}

pub fn text_left_elide(ui: &Ui, painter: &Painter, rect: Rect, s: &str, font: FontId, color: Color32) {
    let full = ui.fonts(|f| f.layout_no_wrap(s.to_owned(), font.clone(), color));
    if full.size().x <= rect.width() {
        painter.galley(pos2(rect.left(), rect.center().y - full.size().y / 2.0), full, color);
        return;
    }
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        let cand: String = std::iter::once('…').chain(chars[mid..].iter().copied()).collect();
        if text_w(ui, &cand, font.clone()) <= rect.width() { hi = mid } else { lo = mid + 1 }
    }
    let cand: String = std::iter::once('…').chain(chars[lo.min(chars.len())..].iter().copied()).collect();
    let g = ui.fonts(|f| f.layout_no_wrap(cand, font, color));
    painter.galley(pos2(rect.left(), rect.center().y - g.size().y / 2.0), g, color);
}

pub fn prism_text(ui: &Ui, painter: &Painter, pos: Pos2, align: Align2, s: &str, font: FontId) -> Rect {
    let n = s.chars().count().max(1);
    let mut job = LayoutJob::default();
    for (i, ch) in s.chars().enumerate() {
        let t = if n == 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
        job.append(&ch.to_string(), 0.0, TextFormat::simple(font.clone(), prism(t)));
    }
    let g = ui.fonts(|f| f.layout_job(job));
    let rect = align.anchor_size(pos, g.size());
    painter.galley(rect.min, g, TEXT);
    rect
}

fn rrect_points(rect: Rect, r: f32) -> Vec<Pos2> {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let mut pts = Vec::with_capacity(40);
    let seg = if r < 1.0 { 0 } else { ((r / 2.0).ceil() as usize).clamp(3, 12) };
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -90.0f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ];
    for (c, start) in corners {
        if seg == 0 {
            pts.push(c);
            continue;
        }
        for k in 0..=seg {
            let a = (start + 90.0 * k as f32 / seg as f32).to_radians();
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    pts
}

pub fn grad_rrect(painter: &Painter, rect: Rect, radius: f32, color: impl Fn(Pos2) -> Color32) {
    let pts = rrect_points(rect, radius);
    let n = pts.len();
    if n < 3 {
        return;
    }
    let c = rect.center();
    let mut m = Mesh::default();
    m.colored_vertex(c, color(c));
    for i in 0..n {
        let p = pts[i];
        let prev = pts[(i + n - 1) % n];
        let next = pts[(i + 1) % n];
        let d = (next - prev).normalized();
        let normal = vec2(d.y, -d.x);
        let col = color(p);
        m.colored_vertex(p - normal * 0.5, col);
        m.colored_vertex(p + normal * 0.5, Color32::TRANSPARENT);
    }
    for i in 0..n as u32 {
        let a = 1 + i * 2;
        let b = 1 + ((i + 1) % n as u32) * 2;
        m.add_triangle(0, a, b);
        m.add_triangle(a, a + 1, b);
        m.add_triangle(b, a + 1, b + 1);
    }
    painter.add(Shape::mesh(m));
}

pub fn prism_rrect(painter: &Painter, rect: Rect, radius: f32) {
    let span = rect.width() + rect.height();
    grad_rrect(painter, rect, radius, |p| prism(((p.x - rect.left()) + (p.y - rect.top())) / span.max(1.0)));
}

pub fn glow(painter: &Painter, center: Pos2, radius: Vec2, color: Color32) {
    let seg = 48;
    let rings = [(0.0, 1.0), (0.35, 0.7), (0.65, 0.32), (1.0, 0.0)];
    let mut m = Mesh::default();
    for &(rf, af) in &rings {
        for k in 0..seg {
            let a = k as f32 / seg as f32 * std::f32::consts::TAU;
            let p = center + vec2(a.cos() * radius.x, a.sin() * radius.y) * rf;
            m.colored_vertex(p, with_alpha_pm(color, af));
        }
    }
    for r in 0..rings.len() as u32 - 1 {
        for k in 0..seg as u32 {
            let a = r * seg as u32 + k;
            let b = r * seg as u32 + (k + 1) % seg as u32;
            let c = a + seg as u32;
            let d = b + seg as u32;
            m.add_triangle(a, b, c);
            m.add_triangle(b, d, c);
        }
    }
    painter.add(Shape::mesh(m));
}

pub fn with_alpha_pm(c: Color32, f: f32) -> Color32 {
    let f = f.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied((c.r() as f32 * f) as u8, (c.g() as f32 * f) as u8, (c.b() as f32 * f) as u8, (c.a() as f32 * f) as u8)
}

pub fn shadow(painter: &Painter, rect: Rect, radius: u8, blur: u8, dy: i8, color: Color32) {
    let s = epaint::Shadow { offset: [0, dy], blur, spread: 0, color };
    painter.add(s.as_shape(rect, CornerRadius::same(radius)));
}

pub fn panel(painter: &Painter, rect: Rect) {
    shadow(painter, rect, 14, 34, 14, Color32::from_black_alpha(140));
    painter.rect_filled(rect, 14, SURFACE);
    painter.rect_stroke(rect, 14, Stroke::new(1.0_f32, line()), StrokeKind::Inside);
}

pub fn hline(painter: &Painter, x0: f32, x1: f32, y: f32, color: Color32) {
    painter.hline(x0..=x1, y.round() + 0.5, Stroke::new(1.0_f32, color));
}

#[derive(Clone, Copy, PartialEq)]
pub enum Btn {
    Ghost,
    Primary,
    Accent,
    Success,
    Danger,
}

pub struct BtnOpts<'a> {
    pub label: &'a str,
    pub icon: Option<Icon>,
    pub kind: Btn,
    pub small: bool,
    pub enabled: bool,
}

pub fn btn_size(ui: &Ui, o: &BtnOpts) -> Vec2 {
    let font = sb(if o.small { 11.5 } else { 12.0 });
    let tw = if o.label.is_empty() { 0.0 } else { text_w(ui, o.label, font) };
    let icon_w = if o.icon.is_some() { 15.0 + if o.label.is_empty() { 0.0 } else { 7.0 } } else { 0.0 };
    let pad = if o.small { 10.0 } else { 12.0 };
    vec2(tw + icon_w + pad * 2.0, if o.small { 28.0 } else { 32.0 })
}

pub fn btn_at(ui: &mut Ui, rect: Rect, id: egui::Id, o: &BtnOpts) -> Response {
    let resp = ui.interact(rect, id, if o.enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let hov = resp.hovered() && o.enabled;
    let alpha = if o.enabled { 1.0 } else { 0.4 };
    let r = 10;
    let (fill, stroke, fg) = match o.kind {
        Btn::Ghost => (if hov { white(0.06) } else { Color32::TRANSPARENT }, if hov { line2() } else { Color32::TRANSPARENT }, TEXT),
        Btn::Primary => (if hov { hex(0x7376f4) } else { INDIGO }, Color32::TRANSPARENT, Color32::WHITE),
        Btn::Accent => (rgba(0x6366f1, if hov { 0.28 } else { 0.18 }), rgba(0x818cf8, 0.4), hex(0xc7d2fe)),
        Btn::Success => (if hov { MINT } else { EMERALD }, Color32::TRANSPARENT, hex(0x04130d)),
        Btn::Danger => (if hov { hex(0xfb7185) } else { ROSE }, Color32::TRANSPARENT, Color32::WHITE),
    };
    let fill = with_alpha_pm(fill, alpha);
    if matches!(o.kind, Btn::Primary | Btn::Success) && o.enabled {
        let glow_c = if o.kind == Btn::Primary { rgba(0x6366f1, 0.55) } else { rgba(0x10b981, 0.5) };
        shadow(p, rect.shrink(4.0), r, 16, 6, glow_c);
    }
    let rect = if resp.is_pointer_button_down_on() { rect.shrink(0.6) } else { rect };
    p.rect_filled(rect, r, fill);
    if stroke != Color32::TRANSPARENT {
        p.rect_stroke(rect, r, Stroke::new(1.0_f32, with_alpha_pm(stroke, alpha)), StrokeKind::Inside);
    }
    let font = sb(if o.small { 11.5 } else { 12.0 });
    let fg = with_alpha_pm(fg, alpha);
    let tw = if o.label.is_empty() { 0.0 } else { text_w(ui, o.label, font.clone()) };
    let icon_w = if o.icon.is_some() { 15.0 + if o.label.is_empty() { 0.0 } else { 7.0 } } else { 0.0 };
    let x0 = rect.center().x - (tw + icon_w) / 2.0;
    if let Some(ic) = o.icon {
        icons::paint(ui, ic, Rect::from_center_size(pos2(x0 + 7.5, rect.center().y), vec2(15.0, 15.0)), fg);
    }
    if !o.label.is_empty() {
        ui.painter().text(pos2(x0 + icon_w, rect.center().y), Align2::LEFT_CENTER, o.label, font, fg);
    }
    if hov {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn btn(ui: &mut Ui, label: &str, icon: Option<Icon>, kind: Btn, small: bool, enabled: bool) -> Response {
    let o = BtnOpts { label, icon, kind, small, enabled };
    let size = btn_size(ui, &o);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let id = ui.id().with(("btn", label, rect.min.x as i32, rect.min.y as i32));
    btn_at(ui, rect, id, &o)
}

pub fn segmented(ui: &mut Ui, id: &str, items: &[(&str, Option<Icon>)], selected: usize, small: bool) -> Option<usize> {
    let font = sb(if small { 11.5 } else { 12.0 });
    let h_item = if small { 24.0 } else { 26.0 };
    let pad = if small { 9.0 } else { 11.0 };
    let widths: Vec<f32> = items
        .iter()
        .map(|(l, ic)| text_w(ui, l, font.clone()) + pad * 2.0 + if ic.is_some() { 20.0 } else { 0.0 })
        .collect();
    let total = widths.iter().sum::<f32>() + 2.0 * (items.len() as f32 - 1.0) + 6.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, h_item + 6.0), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(rect, 11, SURFACE);
    p.rect_stroke(rect, 11, Stroke::new(1.0_f32, line()), StrokeKind::Inside);
    let mut x = rect.left() + 3.0;
    let mut clicked = None;
    for (i, ((label, ic), w)) in items.iter().zip(&widths).enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 3.0), vec2(*w, h_item));
        let resp = ui.interact(r, ui.id().with((id, i)), Sense::click());
        let on = i == selected;
        if on {
            shadow(&p, r.shrink(2.0), 8, 10, 2, rgba(0x6366f1, 0.6));
            p.rect_filled(r, 8, INDIGO);
        }
        let fg = if on { Color32::WHITE } else if resp.hovered() { TEXT } else { MUTED };
        let mut tx = r.center().x - (w - pad * 2.0) / 2.0;
        if let Some(icon) = ic {
            icons::paint(ui, *icon, Rect::from_center_size(pos2(tx + 7.0, r.center().y), vec2(14.0, 14.0)), fg);
            tx += 20.0;
        }
        p.text(pos2(tx, r.center().y), Align2::LEFT_CENTER, *label, font.clone(), fg);
        if resp.clicked() {
            clicked = Some(i);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        x += w + 2.0;
    }
    clicked
}

pub fn select_btn(ui: &mut Ui, label: &str, min_w: f32) -> Response {
    let w = (text_w(ui, label, f(12.0)) + 40.0).max(min_w);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 9, BG);
    p.rect_stroke(rect, 9, Stroke::new(1.0_f32, if resp.hovered() { rgba(0x818cf8, 0.6) } else { line2() }), StrokeKind::Inside);
    p.text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, label, f(12.0), TEXT);
    icons::paint(ui, icons::CHEV_DOWN, Rect::from_center_size(pos2(rect.right() - 15.0, rect.center().y), vec2(14.0, 14.0)), MUTED);
    resp
}

pub fn spinner_ring(painter: &Painter, center: Pos2, r_out: f32, r_in: f32, angle: f32) {
    let seg = 64;
    let mut m = Mesh::default();
    for k in 0..=seg {
        let t = k as f32 / seg as f32;
        let a = angle + t * std::f32::consts::TAU;
        let col = if t < 0.15 || t > 0.85 { Color32::TRANSPARENT } else { with_alpha_pm(prism((t - 0.15) / 0.7), ((t - 0.15) / 0.2).min(1.0)) };
        let d = vec2(a.cos(), a.sin());
        m.colored_vertex(center + d * r_out, col);
        m.colored_vertex(center + d * r_in, col);
    }
    for k in 0..seg as u32 {
        let i = k * 2;
        m.add_triangle(i, i + 1, i + 2);
        m.add_triangle(i + 1, i + 3, i + 2);
    }
    painter.add(Shape::mesh(m));
}

pub fn checkbox(painter: &Painter, rect: Rect, on: bool, on_color: Color32) {
    if on {
        painter.rect_filled(rect, 6, on_color);
        let c = rect.center();
        let pts = vec![c + vec2(-4.0, 0.0), c + vec2(-1.2, 2.8), c + vec2(4.2, -3.0)];
        painter.add(Shape::line(pts, Stroke::new(2.0_f32, hex(0x04130d))));
    } else {
        painter.rect_stroke(rect, 6, Stroke::new(1.5_f32, DIM), StrokeKind::Inside);
    }
}
