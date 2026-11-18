use std::sync::Arc;

use eframe::egui::{pos2, vec2, Color32, Mesh, Pos2, Rect};

use crate::app::{App, SunArc, DIR_FILL};
use crate::theme::{self, hsl, shade};
use crate::tree::TmRect;

pub const SUN_DEPTH: u8 = 6;

#[derive(Clone, Copy, PartialEq)]
pub struct MeshKey {
    pub origin: (i32, i32),
    pub layout: (u32, u8, u32, u32),
    pub filter: Option<u16>,
    pub count: usize,
}

fn quad(m: &mut Mesh, r: Rect, tl: Color32, tr: Color32, bl: Color32, br: Color32) {
    let i = m.vertices.len() as u32;
    m.colored_vertex(r.left_top(), tl);
    m.colored_vertex(r.right_top(), tr);
    m.colored_vertex(r.left_bottom(), bl);
    m.colored_vertex(r.right_bottom(), br);
    m.add_triangle(i, i + 1, i + 2);
    m.add_triangle(i + 1, i + 3, i + 2);
}

fn flat(m: &mut Mesh, r: Rect, c: Color32) {
    quad(m, r, c, c, c, c);
}

pub fn build_treemap_mesh(app: &App, origin: Pos2, rects: &[TmRect]) -> Mesh {
    let mut m = Mesh::default();
    m.reserve_vertices(rects.len() * 8);
    m.reserve_triangles(rects.len() * 4);
    let edge = Color32::from_black_alpha(90);
    let edge_small = Color32::from_black_alpha(72);
    let dimmed = theme::hex(0x1a2030);
    for r in rects {
        let rect = Rect::from_min_size(origin + vec2(r.x, r.y), vec2(r.w, r.h));
        if r.dir {
            flat(&mut m, rect, DIR_FILL);
            continue;
        }
        let mut base = app.ext_color(r.ext);
        if let Some(f) = app.ext_filter {
            if r.ext != f {
                base = dimmed;
            }
        }
        if r.w * r.h >= 300.0 && r.w > 6.0 && r.h > 6.0 {
            flat(&mut m, rect, edge);
            let inner = rect.shrink(1.0);
            quad(&mut m, inner, shade(base, 0.28), base, base, shade(base, -0.38));
        } else {
            flat(&mut m, rect, base);
            if r.w > 3.0 && r.h > 3.0 {
                flat(&mut m, Rect::from_min_max(pos2(rect.right() - 1.0, rect.top()), rect.right_bottom()), edge_small);
                flat(&mut m, Rect::from_min_max(pos2(rect.left(), rect.bottom() - 1.0), pos2(rect.right() - 1.0, rect.bottom())), edge_small);
            }
        }
    }
    m
}

pub fn tm_hit(rects: &[TmRect], p: Pos2) -> Option<usize> {
    rects.iter().rposition(|r| p.x >= r.x && p.y >= r.y && p.x < r.x + r.w && p.y < r.y + r.h)
}

pub fn tm_parent(rects: &[TmRect], i: usize) -> Option<usize> {
    let r = rects[i];
    (0..i).rev().find(|&j| {
        let p = rects[j];
        p.depth + 1 == r.depth && p.dir && r.x >= p.x - 0.01 && r.y >= p.y - 0.01 && r.x + r.w <= p.x + p.w + 0.01 && r.y + r.h <= p.y + p.h + 0.01
    })
}

pub fn get_treemap_mesh(app: &mut App, origin: Pos2) -> Option<Arc<Mesh>> {
    let tm = app.tm.as_ref()?;
    let key = MeshKey {
        origin: (origin.x.round() as i32, origin.y.round() as i32),
        layout: (tm.key.0, tm.key.1 as u8, tm.key.2, tm.key.3),
        filter: app.ext_filter,
        count: tm.rects.len(),
    };
    if let Some((k, m)) = &app.tm_mesh {
        if *k == key {
            return Some(m.clone());
        }
    }
    let origin = pos2(key.origin.0 as f32, key.origin.1 as f32);
    let mesh = Arc::new(build_treemap_mesh(app, origin, &app.tm.as_ref()?.rects));
    app.tm_mesh = Some((key, mesh.clone()));
    Some(mesh)
}

pub fn arc_color(app: &App, a: &SunArc) -> Color32 {
    if !a.dir {
        return app.ext_color(a.ext);
    }
    let mid = (a.a0 + a.a1) / 2.0 / std::f32::consts::TAU;
    let hue = (235.0 + mid * 200.0) % 360.0;
    hsl(hue, 72.0 - a.d as f32 * 4.0, 60.0 - a.d as f32 * 4.5)
}

pub struct SunGeom {
    pub center: Pos2,
    pub r0: f32,
    pub ring: f32,
}

impl SunGeom {
    pub fn new(area: Rect) -> Self {
        let size = (area.width().min(area.height()) - 16.0).max(60.0);
        let r = size / 2.0;
        let r0 = r * 0.26;
        SunGeom { center: area.center(), r0, ring: (r - r0) / SUN_DEPTH as f32 }
    }
}

pub fn build_sun_mesh(app: &App, g: &SunGeom) -> Mesh {
    let mut m = Mesh::default();
    let gap_col = theme::BG;
    for (i, a) in app.sun.iter().enumerate() {
        let inner = g.r0 + (a.d as f32 - 1.0) * g.ring;
        let outer = inner + g.ring - 1.0;
        let mut col = arc_color(app, a);
        if let Some(f) = app.ext_filter {
            if !a.dir && a.ext != f {
                col = theme::hex(0x1a2030);
            }
        }
        if app.sun_hover == Some(i) {
            col = shade(col, 0.3);
        }
        let span = a.a1 - a.a0;
        let gap = if span > 0.01 { (0.6 / outer).min(span * 0.2) } else { 0.0 };
        let (a0, a1) = (a.a0 + gap, a.a1 - gap);
        let steps = ((a1 - a0) * outer / 4.0).ceil().clamp(1.0, 96.0) as u32;
        let base = m.vertices.len() as u32;
        for k in 0..=steps {
            let t = a0 + (a1 - a0) * k as f32 / steps as f32 - std::f32::consts::FRAC_PI_2;
            let d = vec2(t.cos(), t.sin());
            m.colored_vertex(g.center + d * outer, col);
            m.colored_vertex(g.center + d * inner, col);
        }
        for k in 0..steps {
            let v = base + k * 2;
            m.add_triangle(v, v + 1, v + 2);
            m.add_triangle(v + 1, v + 3, v + 2);
        }
    }
    let _ = gap_col;
    m
}

pub fn sun_hit(app: &App, g: &SunGeom, p: Pos2) -> Option<Result<usize, ()>> {
    let d = p - g.center;
    let dist = d.length();
    if dist < g.r0 {
        return Some(Err(()));
    }
    let ring = ((dist - g.r0) / g.ring).floor() as i32 + 1;
    let mut ang = d.y.atan2(d.x) + std::f32::consts::FRAC_PI_2;
    if ang < 0.0 {
        ang += std::f32::consts::TAU;
    }
    app.sun.iter().position(|a| a.d as i32 == ring && ang >= a.a0 && ang < a.a1).map(Ok)
}
