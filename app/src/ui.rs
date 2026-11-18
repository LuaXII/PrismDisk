use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CursorIcon, Id, Key, Layout, Mesh, Order, Pos2, Rect, Sense, Shape, Stroke, StrokeKind,
    TextEdit, Ui, UiBuilder, Vec2,
};
use egui::text::{LayoutJob, TextFormat, TextWrapping};

use crate::app::*;
use crate::icons::{self, Icon};
use crate::theme::{self, *};
use crate::tree::Mode;
use crate::viz::{self, SunGeom};

const ROW_H: f32 = 26.0;

enum A {
    Select(u32),
    SelectFind(u32),
    Toggle(u32),
    DblTree(u32),
    Open(u32),
    Ctx(Pos2, u32),
    Reveal(u32),
}

fn child(ui: &mut Ui, rect: Rect) -> Ui {
    ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)))
}

pub(crate) fn hchild(ui: &mut Ui, rect: Rect) -> Ui {
    ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)))
}

pub(crate) fn rich(ui: &Ui, parts: &[(&str, egui::FontId, Color32)], wrap: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::default();
    for (t, font, c) in parts {
        job.append(t, 0.0, TextFormat::simple(font.clone(), *c));
    }
    job.wrap = TextWrapping { max_width: wrap, ..Default::default() };
    ui.fonts(|f| f.layout_job(job))
}

fn spaced(ui: &Ui, s: &str, font: egui::FontId, color: Color32, spacing: f32) -> std::sync::Arc<egui::Galley> {
    let mut fmt = TextFormat::simple(font, color);
    fmt.extra_letter_spacing = spacing;
    ui.fonts(|f| f.layout_job(LayoutJob::single_section(s.to_owned(), fmt)))
}

fn paint_icon_rot(ui: &Ui, icon: Icon, rect: Rect, color: Color32, angle: f32) {
    egui::Image::new(egui::ImageSource::Bytes { uri: icon.0.into(), bytes: egui::load::Bytes::Static(icon.1.as_bytes()) })
        .tint(color)
        .rotate(angle, Vec2::splat(0.5))
        .paint_at(ui, rect);
}

fn vgrad(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(rect.left_top(), top);
    m.colored_vertex(rect.right_top(), top);
    m.colored_vertex(rect.left_bottom(), bottom);
    m.colored_vertex(rect.right_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 3, 2);
    painter.add(Shape::mesh(m));
}

pub(crate) fn hgrad_rrect(painter: &egui::Painter, rect: Rect, radius: f32, a: Color32, b: Color32) {
    theme::grad_rrect(painter, rect, radius, |p| lerp(a, b, (p.x - rect.left()) / rect.width().max(1.0)));
}

pub(crate) fn prism_bar(painter: &egui::Painter, rect: Rect, radius: f32) {
    theme::grad_rrect(painter, rect, radius, |p| prism((p.x - rect.left()) / rect.width().max(1.0)));
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump();
        self.keys(ctx);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ctx, |ui| {
            let full = ui.max_rect();
            let top = Rect::from_min_size(full.min, vec2(full.width(), 57.0));
            self.topbar(ui, top);
            let mut y = top.bottom();
            if self.banner {
                let r = Rect::from_min_size(pos2(full.left(), y), vec2(full.width(), 45.0));
                self.banner_ui(ui, r);
                y = r.bottom();
            }
            let show_update = self.update_banner && matches!(self.update, UpdateState::Ready(..) | UpdateState::Downloading(..) | UpdateState::Failed(_, Some(_)));
            if show_update {
                let r = Rect::from_min_size(pos2(full.left(), y), vec2(full.width(), 45.0));
                self.update_banner_ui(ui, r);
                y = r.bottom();
            }
            let body = Rect::from_min_max(pos2(full.left(), y), full.max);
            if self.home || self.info.is_none() {
                self.home_ui(ui, body);
            } else {
                self.workspace(ui, body);
            }
        });
        self.overlays(ctx);
    }
}

impl App {
    fn keys(&mut self, ctx: &egui::Context) {
        let (esc, f5, enter) = ctx.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::F5), i.key_pressed(Key::Enter)));
        if let Some(m) = self.modal.take() {
            if esc {
                return;
            }
            if enter {
                self.run_modal_action(m.action);
                return;
            }
            self.modal = Some(m);
            return;
        }
        if self.about {
            if esc {
                self.about = false;
            }
            return;
        }
        if esc {
            self.menu = None;
        }
        if f5 {
            if let Some((k, t)) = self.last_scan.clone() {
                self.start_scan(&k, &t);
            }
        }
        if self.scan.is_some() || self.home || self.info.is_none() || ctx.wants_keyboard_input() {
            return;
        }
        let (ctrl_f, back, del, shift, ctrl_c, up, down, left, right, pgup, pgdn) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(Key::F),
                i.key_pressed(Key::Backspace),
                i.key_pressed(Key::Delete),
                i.modifiers.shift,
                i.modifiers.command && i.key_pressed(Key::C),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::PageUp),
                i.key_pressed(Key::PageDown),
            )
        });
        if ctrl_f {
            self.switch_tab(Tab::Search);
        } else if back {
            self.focus_up();
        } else if del {
            if let Some(s) = self.sel {
                self.confirm_delete(vec![s], shift);
            }
        } else if ctrl_c {
            if let Some(s) = self.sel {
                self.copy_path(s);
            }
        }
        if self.tab != Tab::Tree {
            return;
        }
        let Some(i) = self.sel.and_then(|s| self.vis_index.get(&s).copied()) else { return };
        let n = self.visible.len();
        let cur = self.visible[i].row.clone();
        if down && i + 1 < n {
            let id = self.visible[i + 1].row.id;
            self.select(id, true);
        } else if up && i > 0 {
            let id = self.visible[i - 1].row.id;
            self.select(id, true);
        } else if pgdn {
            let id = self.visible[(i + 15).min(n - 1)].row.id;
            self.select(id, true);
        } else if pgup {
            let id = self.visible[i.saturating_sub(15)].row.id;
            self.select(id, true);
        } else if right && cur.dir {
            if !self.expanded.contains(&cur.id) {
                self.toggle(cur.id, Some(true));
            } else if i + 1 < n {
                let id = self.visible[i + 1].row.id;
                self.select(id, true);
            }
        } else if left {
            if cur.dir && self.expanded.contains(&cur.id) {
                self.toggle(cur.id, Some(false));
            } else {
                let d = self.visible[i].depth;
                if let Some(j) = (0..i).rev().find(|&j| self.visible[j].depth < d) {
                    let id = self.visible[j].row.id;
                    self.select(id, true);
                }
            }
        } else if enter {
            if cur.dir {
                self.set_focus(cur.id);
            } else {
                self.open_item(cur.id);
            }
        }
    }

    fn topbar(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        vgrad(&p, rect, rgba(0x131822, 0.9), rgba(0x0b0e14, 0.9));
        hline(&p, rect.left(), rect.right(), rect.bottom() - 1.0, line());
        let inner = rect.shrink2(vec2(16.0, 10.0));

        let brand = Rect::from_min_size(inner.min, vec2(122.0, 36.0));
        let br = ui.interact(brand, Id::new("brand"), Sense::click());
        let mark = Rect::from_min_size(pos2(brand.left(), brand.center().y - 16.0), vec2(32.0, 32.0));
        shadow(&p, mark.shrink(3.0), 10, 18, 6, rgba(0x6366f1, 0.7));
        prism_rrect(&p, mark, 10.0);
        icons::paint(ui, icons::LOGO, Rect::from_center_size(mark.center(), vec2(19.0, 19.0)), Color32::WHITE);
        p.text(pos2(mark.right() + 10.0, mark.center().y), Align2::LEFT_CENTER, "PrismDisk", b(15.0), TEXT);
        if br.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if br.clicked() && self.scan.is_none() {
            self.home = true;
            self.drives = crate::fsinfo::enumerate();
        }

        let mut actions = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
        actions.spacing_mut().item_spacing.x = 8.0;
        let has = self.info.is_some();
        if btn(&mut actions, "Rescan", Some(icons::REFRESH), Btn::Primary, false, has && self.last_scan.is_some()).clicked() {
            if let Some((k, t)) = self.last_scan.clone() {
                self.start_scan(&k, &t);
            }
        }
        let sel = if self.mode == Mode::Alloc { 0 } else { 1 };
        let seg_w = text_w(&actions, "Allocated", sb(12.0)) + text_w(&actions, "Size", sb(12.0)) + 44.0 + 2.0 + 6.0;
        let (seg_rect, _) = actions.allocate_exact_size(vec2(seg_w, 32.0), Sense::hover());
        let mut seg_ui = hchild(ui, seg_rect);
        if let Some(i) = segmented(&mut seg_ui, "mode", &[("Allocated", None), ("Size", None)], sel, false) {
            self.set_mode(if i == 0 { Mode::Alloc } else { Mode::Size });
        }
        if btn(&mut actions, "Export", Some(icons::EXPORT), Btn::Ghost, false, has && self.scan.is_none()).clicked() {
            self.pick(Pick::Export(self.focus));
        }
        if btn(&mut actions, "Import", Some(icons::IMPORT), Btn::Ghost, false, self.scan.is_none()).clicked() {
            self.pick(Pick::Csv);
        }
        if btn(&mut actions, "Folder", Some(icons::FOLDER_O), Btn::Ghost, false, self.scan.is_none()).clicked() {
            self.pick(Pick::Folder);
        }
        let about = btn(&mut actions, "", Some(icons::INFO), Btn::Ghost, false, true).on_hover_text("What's new & updates");
        if matches!(self.update, UpdateState::Ready(..) | UpdateState::Downloading(..)) {
            let c = about.rect.right_top() + vec2(-8.0, 8.0);
            p.circle_filled(c, 4.5, BG);
            p.circle_filled(c, 3.5, MINT);
        }
        if about.clicked() {
            self.about = true;
            if matches!(self.update, UpdateState::Idle | UpdateState::Failed(..)) {
                self.check_updates(false);
            }
        }
        let actions_left = actions.min_rect().left();

        let strip = Rect::from_min_max(pos2(brand.right() + 14.0, inner.top()), pos2(actions_left - 14.0, inner.bottom()));
        let sp = p.with_clip_rect(strip);
        let mut x = strip.left();
        let drives = self.drives.clone();
        for d in &drives {
            let free = format!("{} free", fmt_bytes(d.free));
            let fw = text_w(ui, &free, mono(10.5)).max(64.0);
            let w = 9.0 + 17.0 + 8.0 + text_w(ui, &format!("{}:", d.id), b(12.5)) + 8.0 + fw + 12.0;
            let r = Rect::from_min_size(pos2(x, strip.top()), vec2(w, 36.0));
            x += w + 6.0;
            if r.left() > strip.right() {
                break;
            }
            let resp = ui.interact(r.intersect(strip), Id::new(("chip", &d.id)), Sense::click());
            let on = self.info.as_ref().map(|i| i.kind == "drive" && i.root_path.to_uppercase().starts_with(&d.id)).unwrap_or(false);
            let (fill, stroke) = if on {
                (rgba(0x6366f1, 0.12), rgba(0x818cf8, 0.55))
            } else if resp.hovered() {
                (SURFACE2, line2())
            } else {
                (SURFACE, line())
            };
            sp.rect_filled(r, 11, fill);
            sp.rect_stroke(r, 11, Stroke::new(1.0_f32, stroke), StrokeKind::Inside);
            let mut cx = r.left() + 9.0;
            let ic = Rect::from_center_size(pos2(cx + 8.5, r.center().y), vec2(17.0, 17.0));
            if ic.right() < strip.right() {
                icons::paint(ui, if d.removable { icons::USB } else { icons::DRIVE }, ic, SKY);
            }
            cx += 25.0;
            let lr = sp.text(pos2(cx, r.center().y), Align2::LEFT_CENTER, format!("{}:", d.id), b(12.5), TEXT);
            cx = lr.right() + 8.0;
            let used = if d.capacity > 0 { (d.capacity - d.free) as f32 / d.capacity as f32 } else { 0.0 };
            let barr = Rect::from_min_size(pos2(cx, r.center().y - 8.0), vec2(64.0, 4.0));
            sp.rect_filled(barr, 9, white(0.08));
            let fr = Rect::from_min_size(barr.min, vec2(64.0 * used, 4.0));
            if used > 0.9 {
                hgrad_rrect(&sp, fr, 2.0, AMBER, ROSE);
            } else {
                prism_bar(&sp, fr, 2.0);
            }
            sp.text(pos2(cx, r.center().y + 2.0), Align2::LEFT_TOP, free, mono(10.5), MUTED);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if resp.clicked() {
                self.start_scan("drive", &d.root);
            }
        }
    }

    fn banner_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        hgrad_rrect(&p, rect, 0.0, rgba(0xf59e0b, 0.12), rgba(0xf59e0b, 0.04));
        hline(&p, rect.left(), rect.right(), rect.bottom() - 1.0, rgba(0xf59e0b, 0.25));
        let mut c = hchild(ui, rect.shrink2(vec2(16.0, 8.0)));
        c.spacing_mut().item_spacing.x = 12.0;
        let (ir, _) = c.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        icons::paint(&c, icons::SHIELD, ir, AMBER);
        let text_w_avail = rect.width() - 32.0 - 16.0 - 12.0 - 230.0 - 40.0;
        let g = rich(
            &c,
            &[
                ("Running without administrator rights. ", sb(12.0), hex(0xfde68a)),
                ("Drive scans fall back to a slower folder walk. Restart elevated for instant MFT scans.", f(12.0), hex(0xfde68a)),
            ],
            text_w_avail,
        );
        let (tr, _) = c.allocate_exact_size(vec2(text_w_avail, g.size().y), Sense::hover());
        c.painter().galley(pos2(tr.left(), tr.center().y - g.size().y / 2.0), g, TEXT);
        if btn(&mut c, "Restart as administrator", None, Btn::Accent, true, true).clicked() {
            self.relaunch_admin();
        }
        let (xr, xresp) = c.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
        if xresp.hovered() {
            c.painter().rect_filled(xr, 8, white(0.08));
        }
        icons::paint(&c, icons::X, Rect::from_center_size(xr.center(), vec2(14.0, 14.0)), if xresp.hovered() { TEXT } else { MUTED });
        if xresp.clicked() {
            self.banner = false;
        }
    }

    fn home_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().with_clip_rect(rect);
        let w = rect.width();
        glow(&p, pos2(rect.left() + w * 0.32, rect.top() + 60.0), vec2(w * 0.32, 300.0), rgba(0x6366f1, 0.16));
        glow(&p, pos2(rect.left() + w * 0.68, rect.top() + 30.0), vec2(w * 0.28, 260.0), rgba(0x0ea5e9, 0.12));
        glow(&p, pos2(rect.left() + w * 0.55, rect.top() + 240.0), vec2(w * 0.22, 200.0), rgba(0x34d399, 0.06));

        let mut area = child(ui, rect);
        egui::ScrollArea::vertical().id_salt("home").auto_shrink([false, false]).show(&mut area, |ui| {
            let cw = (w - 64.0).min(1060.0 - 64.0);
            let x0 = rect.left() + (w - cw) / 2.0;
            let mut y = rect.top() + 56.0;
            let p = ui.painter().clone();

            let mark = Rect::from_center_size(pos2(rect.center().x, y + 32.0), vec2(64.0, 64.0));
            shadow(&p, mark.shrink(6.0), 20, 40, 18, rgba(0x6366f1, 0.8));
            prism_rrect(&p, mark, 20.0);
            icons::paint(ui, icons::LOGO, Rect::from_center_size(mark.center(), vec2(36.0, 36.0)), Color32::WHITE);
            y += 64.0 + 18.0;
            p.text(pos2(rect.center().x, y), Align2::CENTER_TOP, "What would you like to analyze?", b(28.0), TEXT);
            y += 40.0;
            let mut job = LayoutJob::single_section(
                "PrismDisk reads the NTFS Master File Table directly, so even a full drive with millions of files is mapped in seconds.".into(),
                TextFormat::simple(f(14.0), MUTED),
            );
            job.wrap = TextWrapping { max_width: 560.0, ..Default::default() };
            job.halign = Align::Center;
            let g = ui.fonts(|fo| fo.layout_job(job));
            let gh = g.size().y;
            p.galley(pos2(rect.center().x, y), g, MUTED);
            y += gh + 36.0;

            let cols = ((cw + 14.0) / (300.0 + 14.0)).floor().max(1.0) as usize;
            let card_w = (cw - 14.0 * (cols as f32 - 1.0)) / cols as f32;
            let card_h = 132.0;
            let drives = self.drives.clone();
            for (i, d) in drives.iter().enumerate() {
                let (cx, cy) = (i % cols, i / cols);
                let r = Rect::from_min_size(pos2(x0 + cx as f32 * (card_w + 14.0), y + cy as f32 * (card_h + 14.0)), vec2(card_w, card_h));
                if self.drive_card(ui, r, d) {
                    self.start_scan("drive", &d.root);
                }
            }
            if drives.is_empty() {
                p.text(pos2(rect.center().x, y + 40.0), Align2::CENTER_CENTER, "No local drives found. Use “Scan a folder” instead.", f(13.0), DIM);
            }
            let rows = drives.len().div_ceil(cols).max(1);
            y += rows as f32 * (card_h + 14.0);

            let aw = (cw - 14.0) / 2.0;
            let alts = [
                (icons::FOLDER_O, "Scan a folder", "Analyze just one directory", Pick::Folder),
                (icons::IMPORT, "Open a CSV export", "WizTree or PrismDisk .csv", Pick::Csv),
            ];
            for (k, (ic, title, sub, what)) in alts.into_iter().enumerate() {
                let r = Rect::from_min_size(pos2(x0 + k as f32 * (aw + 14.0), y), vec2(aw, 64.0));
                let resp = ui.interact(r, Id::new(("alt", k)), Sense::click());
                if resp.hovered() {
                    p.rect_filled(r, 16, rgba(0x6366f1, 0.06));
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                let col = if resp.hovered() { rgba(0x818cf8, 0.5) } else { line2() };
                let mut pts = rrect_outline(r.shrink(0.5), 16.0);
                pts.push(pts[0]);
                for s in Shape::dashed_line(&pts, Stroke::new(1.0_f32, col), 4.0, 3.0) {
                    p.add(s);
                }
                icons::paint(ui, ic, Rect::from_center_size(pos2(r.left() + 18.0 + 11.0, r.center().y), vec2(22.0, 22.0)), INDIGO2);
                p.text(pos2(r.left() + 54.0, r.center().y - 1.0), Align2::LEFT_BOTTOM, title, sb(13.5), TEXT);
                p.text(pos2(r.left() + 54.0, r.center().y + 1.0), Align2::LEFT_TOP, sub, f(12.0), MUTED);
                if resp.clicked() {
                    self.pick(what);
                }
            }
            y += 64.0 + 40.0;
            ui.allocate_space(vec2(1.0, (y - rect.top()).max(0.0)));
        });
    }

    fn drive_card(&mut self, ui: &mut Ui, r0: Rect, d: &crate::fsinfo::DriveInfo) -> bool {
        let resp = ui.interact(r0, Id::new(("dcard", &d.id)), Sense::click());
        let hov = resp.hovered();
        let t = ui.ctx().animate_bool_with_time(Id::new(("dcard-h", &d.id)), hov, 0.18);
        let r = r0.translate(vec2(0.0, -2.0 * t));
        let p = ui.painter().clone();
        if t > 0.0 {
            shadow(&p, r.shrink(10.0), 18, 40, 18, with_alpha_pm(rgba(0x6366f1, 0.6), t));
        }
        theme::grad_rrect(&p, r, 18.0, |pt| lerp(SURFACE2, SURFACE, (pt.y - r.top()) / r.height()));
        p.rect_stroke(r, 18, Stroke::new(1.0_f32, lerp(line(), rgba(0x818cf8, 0.45), t)), StrokeKind::Inside);
        let inner = r.shrink(18.0);
        let ico = Rect::from_min_size(inner.min, vec2(42.0, 42.0));
        p.rect_filled(ico, 12, rgba(0x0ea5e9, 0.12));
        p.rect_stroke(ico, 12, Stroke::new(1.0_f32, rgba(0x0ea5e9, 0.25)), StrokeKind::Inside);
        icons::paint(ui, if d.removable { icons::USB } else { icons::DRIVE }, Rect::from_center_size(ico.center(), vec2(22.0, 22.0)), SKY);
        let name = format!("{} ({}:)", if d.label.is_empty() { if d.removable { "Removable Disk" } else { "Local Disk" } } else { &d.label }, d.id);
        let fast = d.fs.eq_ignore_ascii_case("NTFS") && self.admin;
        let tag_txt = if fast { "MFT" } else { "WALK" };
        let tw = text_w(ui, tag_txt, monob(10.0)) + if fast { 22.0 } else { 14.0 };
        let tag = Rect::from_min_size(pos2(inner.right() - tw, ico.center().y - 10.0), vec2(tw, 20.0));
        let (tf, ts, tc) = if fast { (rgba(0x10b981, 0.12), rgba(0x10b981, 0.25), hex(0x6ee7b7)) } else { (rgba(0x94a3b8, 0.1), line2(), MUTED) };
        p.rect_filled(tag, 6, tf);
        p.rect_stroke(tag, 6, Stroke::new(1.0_f32, ts), StrokeKind::Inside);
        if fast {
            icons::paint(ui, icons::BOLT, Rect::from_center_size(pos2(tag.left() + 11.0, tag.center().y), vec2(11.0, 11.0)), AMBER);
        }
        p.text(pos2(tag.right() - 7.0, tag.center().y), Align2::RIGHT_CENTER, tag_txt, monob(10.0), tc);
        text(ui, &p, pos2(ico.right() + 12.0, ico.center().y - 1.0), Align2::LEFT_BOTTOM, &name, b(15.0), TEXT, tag.left() - ico.right() - 20.0);
        p.text(pos2(ico.right() + 12.0, ico.center().y + 1.0), Align2::LEFT_TOP, format!("{} · {}", if d.fs.is_empty() { "Unknown" } else { &d.fs }, fmt_bytes(d.capacity)), f(11.5), MUTED);
        let used = if d.capacity > 0 { (d.capacity - d.free) as f32 / d.capacity as f32 } else { 0.0 };
        let bar_r = Rect::from_min_size(pos2(inner.left(), ico.bottom() + 16.0), vec2(inner.width(), 8.0));
        p.rect_filled(bar_r, 99, white(0.07));
        let fr = Rect::from_min_size(bar_r.min, vec2((inner.width() * used).max(8.0), 8.0));
        if used > 0.9 { hgrad_rrect(&p, fr, 4.0, AMBER, ROSE) } else { prism_bar(&p, fr, 4.0) }
        let ny = bar_r.bottom() + 9.0;
        let g = rich(ui, &[(&fmt_bytes(d.capacity - d.free), monob(12.0), TEXT), (" used", f(12.0), MUTED)], 400.0);
        p.galley(pos2(inner.left(), ny), g, TEXT);
        let g = rich(ui, &[(&fmt_bytes(d.free), monob(12.0), TEXT), (" free", f(12.0), MUTED)], 400.0);
        let gw = g.size().x;
        p.galley(pos2(inner.right() - gw, ny), g, TEXT);
        if hov {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        resp.clicked()
    }

    fn workspace(&mut self, ui: &mut Ui, rect: Rect) {
        let sub = Rect::from_min_size(rect.min, vec2(rect.width(), 43.0));
        let status = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 31.0), rect.max);
        self.subbar(ui, sub);
        self.statusbar(ui, status);

        let panes = Rect::from_min_max(pos2(rect.left(), sub.bottom()), pos2(rect.right(), status.top())).shrink(12.0);
        let gap = 12.0;
        let right_w = 300.0;
        let max_vis = (panes.height() - 160.0 - 2.0 * gap).max(140.0);
        let vis_h = if self.vis_h <= 0.0 { panes.height() * 0.38 } else { self.vis_h }.clamp(140.0, max_vis.max(140.0)).min(panes.height() - 200.0).max(120.0);
        let top_h = panes.height() - vis_h - 2.0 * gap;
        let list = Rect::from_min_size(panes.min, vec2(panes.width() - right_w - gap, top_h));
        let types = Rect::from_min_size(pos2(list.right() + gap, panes.top()), vec2(right_w, top_h));
        let visual = Rect::from_min_max(pos2(panes.left(), panes.bottom() - vis_h), panes.max);
        let split = Rect::from_min_max(pos2(panes.left(), list.bottom()), pos2(panes.right(), visual.top()));

        self.list_panel(ui, list);
        self.types_panel(ui, types);
        let sr = ui.interact(split, Id::new("splitter"), Sense::drag());
        let active = sr.hovered() || sr.dragged();
        if active {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
        }
        ui.painter().rect_filled(Rect::from_center_size(split.center(), vec2(46.0, 4.0)), 9, if active { INDIGO2 } else { white(0.1) });
        if sr.dragged() {
            if let Some(pp) = ui.ctx().pointer_interact_pos() {
                self.vis_h = (panes.bottom() - pp.y - gap / 2.0).clamp(140.0, (panes.height() - 200.0).max(140.0));
            }
        }
        if sr.drag_stopped() {
            self.save_settings();
        }
        self.visual_panel(ui, visual);
    }

    fn subbar(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        p.rect_filled(rect, 0, BG2);
        hline(&p, rect.left(), rect.right(), rect.bottom() - 1.0, line());
        let inner = rect.shrink2(vec2(16.0, 8.0));
        let Some(info) = self.info.clone() else { return };
        let Some(fd) = self.focus_detail.clone() else { return };
        let Some(root_row) = self.root_row.clone() else { return };

        let v = self.val(&fd.row);
        let (txt, used_pct, sel_pct): (Vec<(String, bool)>, f32, f32) = if let Some(cap) = info.capacity {
            let used = cap - info.free.unwrap_or(0);
            (
                vec![
                    (fmt_bytes(v), true),
                    (format!(" in view · {} used of {} · {} free", fmt_bytes(used), fmt_bytes(cap), fmt_bytes(info.free.unwrap_or(0))), false),
                ],
                used as f32 / cap as f32,
                v as f32 / cap as f32,
            )
        } else {
            let total = self.val(&root_row);
            (vec![(fmt_bytes(v), true), (format!(" in view of {}", fmt_bytes(total)), false)], 1.0, if total > 0 { v as f32 / total as f32 } else { 0.0 })
        };
        let bar_r = Rect::from_min_size(pos2(inner.right() - 220.0, inner.center().y - 3.5), vec2(220.0, 7.0));
        p.rect_filled(bar_r, 99, white(0.07));
        theme::grad_rrect(&p, Rect::from_min_size(bar_r.min, vec2((220.0 * used_pct).max(7.0), 7.0)), 3.5, |pt| with_alpha_pm(prism((pt.x - bar_r.left()) / 220.0), 0.55));
        prism_bar(&p, Rect::from_min_size(bar_r.min, vec2((220.0 * sel_pct.min(1.0)).max(7.0), 7.0)), 3.5);
        let parts: Vec<(&str, egui::FontId, Color32)> = txt.iter().map(|(s, bold)| (s.as_str(), if *bold { monob(11.5) } else { mono(11.5) }, if *bold { TEXT } else { MUTED })).collect();
        let g = rich(ui, &parts, 2000.0);
        let gw = g.size().x;
        let gh = g.size().y;
        let vol_left = bar_r.left() - 12.0 - gw;
        p.galley(pos2(vol_left, inner.center().y - gh / 2.0), g, TEXT);

        let crumbs_r = Rect::from_min_max(inner.min, pos2(vol_left - 16.0, inner.bottom()));
        let cp = p.with_clip_rect(crumbs_r);
        let mut x = crumbs_r.left();
        for (i, (id, name)) in fd.crumbs.iter().enumerate() {
            if i > 0 {
                icons::paint(ui, icons::CHEV, Rect::from_center_size(pos2(x + 6.0, inner.center().y), vec2(12.0, 12.0)), DIM);
                x += 14.0;
            }
            let label = if i == 0 && info.kind == "drive" { info.title.clone() } else { name.clone() };
            let last = i + 1 == fd.crumbs.len();
            let font = if last { sb(12.0) } else { f(12.0) };
            let tw = text_w(ui, &label, font.clone()).min(206.0);
            let icon_w = if i == 0 { 18.0 } else { 0.0 };
            let r = Rect::from_min_size(pos2(x, inner.center().y - 13.0), vec2(tw + 14.0 + icon_w, 26.0));
            let resp = ui.interact(r.intersect(crumbs_r), Id::new(("crumb", *id)), Sense::click());
            if resp.hovered() {
                cp.rect_filled(r, 7, white(0.06));
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            let col = if last || resp.hovered() { TEXT } else { MUTED };
            if i == 0 {
                icons::paint(ui, icons::HOME, Rect::from_center_size(pos2(r.left() + 7.0 + 7.0, r.center().y), vec2(14.0, 14.0)), col);
            }
            let g = galley(ui, &label, font, col, 206.0);
            cp.galley(pos2(r.left() + 7.0 + icon_w, r.center().y - g.size().y / 2.0), g, col);
            if resp.clicked() {
                self.set_focus(*id);
            }
            x = r.right() + 2.0;
        }
    }

    fn statusbar(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        p.rect_filled(rect, 0, BG2);
        hline(&p, rect.left(), rect.right(), rect.top(), line());
        let Some(info) = self.info.clone() else { return };
        let inner = rect.shrink2(vec2(16.0, 6.0));
        let mut x = inner.left();
        let fast = info.fast();
        let ew = text_w(ui, &info.engine, sb(11.5)) + 16.0 + if fast { 16.0 } else { 0.0 };
        let er = Rect::from_min_size(pos2(x, inner.center().y - 10.0), vec2(ew, 20.0));
        p.rect_filled(er, 6, if fast { rgba(0x6366f1, 0.14) } else { rgba(0x94a3b8, 0.12) });
        let mut tx = er.left() + 8.0;
        if fast {
            icons::paint(ui, icons::BOLT, Rect::from_center_size(pos2(tx + 6.0, er.center().y), vec2(12.0, 12.0)), AMBER);
            tx += 16.0;
        }
        p.text(pos2(tx, er.center().y), Align2::LEFT_CENTER, &info.engine, sb(11.5), if fast { hex(0xc7d2fe) } else { MUTED });
        x = er.right() + 14.0;
        if let Some(r) = &self.root_row {
            let s = format!("{} files · {} folders · scanned in {:.2} s", fmt_num(r.files as u64), fmt_num(r.folders as u64), info.elapsed_ms as f64 / 1000.0);
            let rr = p.text(pos2(x, inner.center().y), Align2::LEFT_CENTER, s, f(11.5), MUTED);
            x = rr.right() + 14.0;
        }
        let mut right = inner.right();
        if let Some(note) = &info.note {
            let w = text_w(ui, note, f(11.5)).min(inner.width() * 0.4);
            text(ui, &p, pos2(right, inner.center().y), Align2::RIGHT_CENTER, note, f(11.5), AMBER, w);
            right -= w + 14.0;
        }
        if let Some(s) = self.sel {
            if let Some(d) = self.node(s) {
                let path = d.row.path.unwrap_or_default();
                text_left_elide(ui, &p, Rect::from_min_max(pos2(x, inner.top()), pos2(right, inner.bottom())), &path, mono(11.5), DIM);
            }
        }
    }

    fn list_panel(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        panel(&p, rect);
        let tabs_r = Rect::from_min_size(rect.min, vec2(rect.width(), 41.0));
        hline(&p, rect.left(), rect.right(), tabs_r.bottom() - 1.0, line());
        let mut x = rect.left() + 8.0;
        let pill = if self.clean_pill > 0 { Some(fmt_bytes(self.clean_pill)) } else { None };
        let tabs: [(Tab, Icon, &str); 4] =
            [(Tab::Tree, icons::TREE, "Tree"), (Tab::Search, icons::SEARCH, "Largest & Search"), (Tab::Dups, icons::DUP, "Duplicates"), (Tab::Clean, icons::SPARKLE, "Cleanup")];
        for (tab, ic, label) in tabs {
            let mut w = 12.0 + 14.0 + 7.0 + text_w(ui, label, sb(12.0)) + 12.0;
            let pill_w = if tab == Tab::Clean { pill.as_ref().map(|s| text_w(ui, s, monob(10.0)) + 12.0 + 7.0).unwrap_or(0.0) } else { 0.0 };
            w += pill_w;
            let r = Rect::from_min_size(pos2(x, rect.top() + 8.0), vec2(w, 33.0));
            let resp = ui.interact(r, Id::new(("tab", label)), Sense::click());
            let on = self.tab == tab;
            if resp.hovered() && !on {
                p.rect_filled(r, egui::CornerRadius { nw: 9, ne: 9, sw: 0, se: 0 }, white(0.03));
            }
            let col = if on || resp.hovered() { TEXT } else { MUTED };
            icons::paint(ui, ic, Rect::from_center_size(pos2(r.left() + 12.0 + 7.0, r.center().y - 1.0), vec2(14.0, 14.0)), col);
            let tr = p.text(pos2(r.left() + 33.0, r.center().y - 1.0), Align2::LEFT_CENTER, label, sb(12.0), col);
            if tab == Tab::Clean {
                if let Some(s) = &pill {
                    let pr = Rect::from_min_size(pos2(tr.right() + 7.0, r.center().y - 9.0), vec2(pill_w - 7.0, 16.0));
                    p.rect_filled(pr, 99, rgba(0x10b981, 0.15));
                    p.text(pr.center(), Align2::CENTER_CENTER, s, monob(10.0), hex(0x6ee7b7));
                }
            }
            if on {
                prism_bar(&p, Rect::from_min_max(pos2(r.left() + 10.0, r.bottom() - 2.0), pos2(r.right() - 10.0, r.bottom())), 1.0);
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if resp.clicked() {
                self.switch_tab(tab);
            }
            x += w + 2.0;
        }
        let body = Rect::from_min_max(pos2(rect.left(), tabs_r.bottom()), rect.max);
        match self.tab {
            Tab::Tree => self.tree_tab(ui, body),
            Tab::Search => self.search_tab(ui, body),
            Tab::Dups => self.dups_tab(ui, body),
            Tab::Clean => self.clean_tab(ui, body),
        }
    }

    fn grid_head(&self, ui: &Ui, rect: Rect, cols: &[(Rect, &str, Option<&'static str>, bool)]) -> Option<&'static str> {
        let p = ui.painter();
        hline(p, rect.left(), rect.right(), rect.bottom() - 1.0, line());
        let mut clicked = None;
        for (r, label, key, right) in cols {
            let sorted = *key == Some(self.sort_key);
            let mut lbl = label.to_uppercase();
            if sorted {
                lbl.push_str(if self.sort_desc { " ▾" } else { " ▴" });
            }
            let mut col = if sorted { INDIGO2 } else { DIM };
            if let Some(k) = key {
                let resp = ui.interact(*r, Id::new(("head", *k, r.left() as i32)), Sense::click());
                if resp.hovered() {
                    if !sorted {
                        col = MUTED;
                    }
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    clicked = Some(*k);
                }
            }
            let g = spaced(ui, &lbl, sb(10.5), col, 0.6);
            let x = if *right { r.right() - g.size().x } else { r.left() };
            p.with_clip_rect(*r).galley(pos2(x, r.center().y - g.size().y / 2.0), g, col);
        }
        clicked
    }

    fn tree_cols(x0: f32, x1: f32) -> [(f32, f32); 7] {
        let fixed = [128.0, 84.0, 84.0, 70.0, 62.0, 118.0];
        let gap = 8.0;
        let name_w = (x1 - x0 - fixed.iter().sum::<f32>() - gap * 6.0).max(220.0);
        let mut out = [(0.0, 0.0); 7];
        let mut x = x0;
        out[0] = (x, x + name_w);
        x += name_w + gap;
        for (i, w) in fixed.iter().enumerate() {
            out[i + 1] = (x, x + w);
            x += w + gap;
        }
        out
    }

    fn tree_tab(&mut self, ui: &mut Ui, body: Rect) {
        let head = Rect::from_min_size(body.min, vec2(body.width(), 29.0));
        let cols = Self::tree_cols(head.left() + 12.0, head.right() - 14.0);
        let mk = |i: usize| Rect::from_x_y_ranges(cols[i].0..=cols[i].1, head.y_range());
        if let Some(k) = self.grid_head(
            ui,
            head,
            &[
                (mk(0), "Name", Some("name"), false),
                (mk(1), "% of Parent", Some("pct"), true),
                (mk(2), "Allocated", Some("alloc"), true),
                (mk(3), "Size", Some("size"), true),
                (mk(4), "Files", Some("files"), true),
                (mk(5), "Folders", Some("folders"), true),
                (mk(6), "Modified", Some("modified"), true),
            ],
        ) {
            self.set_sort(k);
        }

        let list = Rect::from_min_max(pos2(body.left(), head.bottom() + 1.0), body.max - vec2(0.0, 4.0));
        let mut c = child(ui, list);
        c.spacing_mut().item_spacing = Vec2::ZERO;
        let mut sa = egui::ScrollArea::vertical().id_salt("tree").auto_shrink([false, false]);
        if let Some(i) = self.scroll_to.take() {
            let (off, h) = c.ctx().data(|d| d.get_temp::<(f32, f32)>(Id::new("tree-scroll")).unwrap_or((0.0, list.height())));
            let top = i as f32 * ROW_H;
            let new = if top < off { Some(top) } else if top + ROW_H > off + h { Some(top + ROW_H - h) } else { None };
            if let Some(o) = new {
                sa = sa.vertical_scroll_offset(o);
            }
        }
        let mut actions: Vec<A> = Vec::new();
        let ext_colors = self.ext_colors.clone();
        let out = sa.show_rows(&mut c, ROW_H, self.visible.len(), |ui, range| {
            for i in range {
                let v = &self.visible[i];
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                self.paint_tree_row(ui, rect, v, &resp, &ext_colors, &mut actions);
            }
        });
        c.ctx().data_mut(|d| d.insert_temp(Id::new("tree-scroll"), (out.state.offset.y, out.inner_rect.height())));
        self.apply(actions);
    }

    fn paint_tree_row(&self, ui: &Ui, rect: Rect, v: &VisRow, resp: &egui::Response, ext_colors: &[Color32], actions: &mut Vec<A>) {
        let p = ui.painter();
        let r = &v.row;
        let rr = Rect::from_min_max(pos2(rect.left() + 4.0, rect.top()), pos2(rect.right() - 4.0, rect.bottom()));
        let sel = self.sel == Some(r.id);
        if sel {
            p.rect_filled(rr, 7, rgba(0x6366f1, 0.2));
            p.rect_stroke(rr, 7, Stroke::new(1.0_f32, rgba(0x818cf8, 0.35)), StrokeKind::Inside);
        } else if resp.hovered() {
            p.rect_filled(rr, 7, white(0.035));
        }
        let cols = Self::tree_cols(rr.left() + 12.0, rr.right() - 14.0);
        let cy = rect.center().y;
        let is_root = r.id == self.root;
        let mut x = cols[0].0 + v.depth as f32 * 16.0;
        let tw_r = Rect::from_min_size(pos2(x, cy - 8.0), vec2(16.0, 16.0));
        let mut on_twisty = false;
        if r.dir && r.kids > 0 {
            let open = self.expanded.contains(&r.id);
            let tr = ui.interact(tw_r, Id::new(("tw", r.id)), Sense::click());
            on_twisty = tr.hovered();
            if tr.hovered() {
                p.rect_filled(tw_r, 4, white(0.08));
            }
            let t = ui.ctx().animate_bool_with_time(Id::new(("twa", r.id)), open, 0.12);
            paint_icon_rot(ui, icons::CHEV, Rect::from_center_size(tw_r.center(), vec2(12.0, 12.0)), if tr.hovered() { TEXT } else { DIM }, t * std::f32::consts::FRAC_PI_2);
            if tr.clicked() {
                actions.push(A::Toggle(r.id));
            }
        }
        x += 22.0;
        let ic = Rect::from_center_size(pos2(x + 7.5, cy), vec2(15.0, 15.0));
        let drive_root = is_root && self.info.as_ref().map(|i| i.kind == "drive").unwrap_or(false);
        if drive_root {
            icons::paint(ui, icons::DRIVE, ic, SKY);
        } else if r.dir {
            icons::paint(ui, icons::FOLDER, ic, FOLDER);
        } else {
            icons::paint(ui, icons::FILE, ic, ext_colors.get(r.ext as usize).copied().unwrap_or(DIM));
        }
        x += 21.0;
        let label = if is_root { self.info.as_ref().map(|i| i.title.clone()).unwrap_or(r.name.clone()) } else { r.name.clone() };
        text(ui, p, pos2(x, cy), Align2::LEFT_CENTER, &label, f(12.5), if r.attrs & 0x6 != 0 && !is_root { MUTED } else { TEXT }, (cols[0].1 - x).max(10.0));
        let val = self.val(r);
        let pct = if v.pv > 0 { val as f64 / v.pv as f64 * 100.0 } else { 100.0 };
        let bar_r = Rect::from_min_max(pos2(cols[1].0, cy - 4.0), pos2(cols[1].1 - 47.0, cy + 4.0));
        p.rect_filled(bar_r, 3, white(0.06));
        let fw = bar_r.width() * (pct.min(100.0) / 100.0) as f32;
        if fw > 0.5 {
            hgrad_rrect(p, Rect::from_min_size(bar_r.min, vec2(fw.max(3.0), 8.0)), 3.0, INDIGO, SKY);
        }
        p.text(pos2(cols[1].1, cy), Align2::RIGHT_CENTER, fmt_pct(pct), mono(11.0), MUTED);
        p.text(pos2(cols[2].1, cy), Align2::RIGHT_CENTER, fmt_bytes(r.alloc), mono(11.5), TEXT2);
        p.text(pos2(cols[3].1, cy), Align2::RIGHT_CENTER, fmt_bytes(r.size), mono(11.5), DIM);
        if r.dir {
            p.text(pos2(cols[4].1, cy), Align2::RIGHT_CENTER, fmt_num(r.files as u64), mono(11.5), DIM);
            p.text(pos2(cols[5].1, cy), Align2::RIGHT_CENTER, fmt_num(r.folders as u64), mono(11.5), DIM);
        }
        p.text(pos2(cols[6].1, cy), Align2::RIGHT_CENTER, fmt_date(r.modified), mono(11.5), MUTED);

        if !on_twisty {
            if resp.double_clicked() {
                actions.push(A::DblTree(r.id));
            } else if resp.clicked() {
                actions.push(A::Select(r.id));
            }
        }
        if resp.secondary_clicked() {
            actions.push(A::Select(r.id));
            actions.push(A::Ctx(resp.interact_pointer_pos().unwrap_or(rect.center()), r.id));
        }
    }

    fn apply(&mut self, actions: Vec<A>) {
        for a in actions {
            match a {
                A::Select(id) => self.select(id, false),
                A::SelectFind(id) => self.sel = Some(id),
                A::Toggle(id) => self.toggle(id, None),
                A::DblTree(id) => {
                    let dir = self.node(id).map(|d| d.row.dir).unwrap_or(false);
                    if dir {
                        self.toggle(id, Some(true));
                        self.set_focus(id);
                    } else {
                        self.open_item(id);
                    }
                }
                A::Open(id) => self.open_item(id),
                A::Ctx(pos, id) => {
                    if let Some(d) = self.node(id) {
                        self.menu = Some(Menu { pos, kind: MenuKind::Node(Box::new(d)), opened: self.ctx.input(|i| i.time) });
                    }
                }
                A::Reveal(id) => self.reveal(id),
            }
        }
    }

    fn search_tab(&mut self, ui: &mut Ui, body: Rect) {
        let p = ui.painter().clone();
        let tb = Rect::from_min_size(body.min, vec2(body.width(), 51.0));
        hline(&p, body.left(), body.right(), tb.bottom() - 1.0, line());
        let inner = tb.shrink2(vec2(12.0, 10.0));
        let mut row = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
        row.spacing_mut().item_spacing.x = 8.0;
        let age = row.horizontal(|ui| select_btn(ui, AGES[self.q_age].1, 150.0)).inner;
        if age.clicked() {
            self.menu = Some(Menu { pos: age.rect.left_bottom() + vec2(0.0, 4.0), kind: MenuKind::Age, opened: self.ctx.input(|i| i.time) });
        }
        let min = row.horizontal(|ui| select_btn(ui, MIN_SIZES[self.q_min].1, 100.0)).inner;
        if min.clicked() {
            self.menu = Some(Menu { pos: min.rect.left_bottom() + vec2(0.0, 4.0), kind: MenuKind::MinSize, opened: self.ctx.input(|i| i.time) });
        }
        let seg_w = ["Files", "Folders", "Both"].iter().map(|s| text_w(ui, s, sb(11.5)) + 18.0).sum::<f32>() + 4.0 + 6.0;
        let (seg_r, _) = row.allocate_exact_size(vec2(seg_w, 30.0), Sense::hover());
        let mut seg_ui = hchild(ui, seg_r);
        if let Some(i) = segmented(&mut seg_ui, "qkind", &[("Files", None), ("Folders", None), ("Both", None)], self.q_kind, true) {
            self.q_kind = i;
            self.run_find();
        }
        let search_r = Rect::from_min_max(pos2(inner.left(), inner.center().y - 15.0), pos2(row.min_rect().left() - 8.0, inner.center().y + 15.0));
        let focused = ui.ctx().memory(|m| m.has_focus(Id::new("q")));
        p.rect_filled(search_r, 9, BG);
        if focused {
            p.rect_stroke(search_r.expand(3.0), 12, Stroke::new(3.0_f32, rgba(0x6366f1, 0.15)), StrokeKind::Inside);
        }
        p.rect_stroke(search_r, 9, Stroke::new(1.0_f32, if focused { rgba(0x818cf8, 0.6) } else { line2() }), StrokeKind::Inside);
        icons::paint(ui, icons::SEARCH, Rect::from_center_size(pos2(search_r.left() + 17.0, search_r.center().y), vec2(14.0, 14.0)), DIM);
        let te_r = Rect::from_min_max(pos2(search_r.left() + 31.0, search_r.top() + 1.0), pos2(search_r.right() - 8.0, search_r.bottom() - 1.0));
        let te = ui.put(
            te_r,
            TextEdit::singleline(&mut self.q)
                .id(Id::new("q"))
                .frame(false)
                .font(f(12.0))
                .text_color(TEXT)
                .vertical_align(Align::Center)
                .margin(vec2(0.0, 0.0))
                .hint_text(egui::RichText::new("Filter, e.g. *.mp4, node_modules, ext:iso, \\Downloads\\*").color(DIM).font(f(12.0))),
        );
        if self.focus_search {
            te.request_focus();
            self.focus_search = false;
        }
        if te.changed() {
            self.q_dirty_at = Some(std::time::Instant::now());
        }

        let head = Rect::from_min_size(pos2(body.left(), tb.bottom()), vec2(body.width(), 29.0));
        let cols = Self::find_cols(head.left() + 12.0, head.right() - 14.0);
        let mk = |i: usize| Rect::from_x_y_ranges(cols[i].0..=cols[i].1, head.y_range());
        self.grid_head(ui, head, &[(mk(0), "Name", None, false), (mk(1), "Folder", None, false), (mk(2), "Allocated", None, true), (mk(3), "Size", None, true), (mk(4), "Modified", None, true)]);

        let foot = Rect::from_min_max(pos2(body.left(), body.bottom() - 31.0), body.max);
        hline(&p, body.left(), body.right(), foot.top(), line());
        text(ui, &p, pos2(foot.left() + 14.0, foot.center().y), Align2::LEFT_CENTER, &self.find_status, f(11.5), DIM, foot.width() - 28.0);

        let list = Rect::from_min_max(pos2(body.left(), head.bottom() + 1.0), pos2(body.right(), foot.top() - 1.0));
        let mut c = child(ui, list);
        c.spacing_mut().item_spacing = Vec2::ZERO;
        let mut actions = Vec::new();
        let ext_colors = self.ext_colors.clone();
        egui::ScrollArea::vertical().id_salt("find").auto_shrink([false, false]).show_rows(&mut c, ROW_H, self.find.len(), |ui, range| {
            for i in range {
                let r = &self.find[i];
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                let p = ui.painter();
                let rr = Rect::from_min_max(pos2(rect.left() + 4.0, rect.top()), pos2(rect.right() - 4.0, rect.bottom()));
                if self.sel == Some(r.id) {
                    p.rect_filled(rr, 7, rgba(0x6366f1, 0.2));
                    p.rect_stroke(rr, 7, Stroke::new(1.0_f32, rgba(0x818cf8, 0.35)), StrokeKind::Inside);
                } else if resp.hovered() {
                    p.rect_filled(rr, 7, white(0.035));
                }
                let cols = Self::find_cols(rr.left() + 12.0, rr.right() - 14.0);
                let cy = rect.center().y;
                let ic = Rect::from_center_size(pos2(cols[0].0 + 7.5, cy), vec2(15.0, 15.0));
                if r.dir {
                    icons::paint(ui, icons::FOLDER, ic, FOLDER);
                } else {
                    icons::paint(ui, icons::FILE, ic, ext_colors.get(r.ext as usize).copied().unwrap_or(DIM));
                }
                text(ui, p, pos2(cols[0].0 + 21.0, cy), Align2::LEFT_CENTER, &r.name, f(12.5), TEXT, cols[0].1 - cols[0].0 - 21.0);
                let path = r.path.as_deref().unwrap_or_default();
                let folder = &path[..path.len().saturating_sub(r.name.len() + 1)];
                text_left_elide(ui, p, Rect::from_x_y_ranges(cols[1].0..=cols[1].1, rect.y_range()), folder, f(11.5), MUTED);
                p.text(pos2(cols[2].1, cy), Align2::RIGHT_CENTER, fmt_bytes(r.alloc), mono(11.5), TEXT2);
                p.text(pos2(cols[3].1, cy), Align2::RIGHT_CENTER, fmt_bytes(r.size), mono(11.5), DIM);
                p.text(pos2(cols[4].1, cy), Align2::RIGHT_CENTER, fmt_date(r.modified), mono(11.5), MUTED);
                if resp.double_clicked() {
                    actions.push(A::Open(r.id));
                } else if resp.clicked() {
                    actions.push(A::SelectFind(r.id));
                }
                if resp.secondary_clicked() {
                    actions.push(A::SelectFind(r.id));
                    actions.push(A::Ctx(resp.interact_pointer_pos().unwrap_or(rect.center()), r.id));
                }
            }
        });
        self.apply(actions);
    }

    fn find_cols(x0: f32, x1: f32) -> [(f32, f32); 5] {
        let fixed = 84.0 + 84.0 + 118.0;
        let gap = 8.0;
        let flex = (x1 - x0 - fixed - gap * 4.0).max(340.0);
        let (a, b) = (flex * 1.1 / 2.5, flex * 1.4 / 2.5);
        let mut x = x0;
        let mut out = [(0.0, 0.0); 5];
        for (i, w) in [a, b, 84.0, 84.0, 118.0].iter().enumerate() {
            out[i] = (x, x + w);
            x += w + gap;
        }
        out
    }

    fn dups_tab(&mut self, ui: &mut Ui, body: Rect) {
        let p = ui.painter().clone();
        let tb = Rect::from_min_size(body.min, vec2(body.width(), 51.0));
        hline(&p, body.left(), body.right(), tb.bottom() - 1.0, line());
        let inner = tb.shrink2(vec2(12.0, 10.0));
        let g = rich(ui, &[("Files with the same name ", f(12.0), MUTED), ("and", egui::FontId::new(12.0, egui::FontFamily::Proportional), MUTED), (" exact size, likely copies. Review before deleting.", f(12.0), MUTED)], inner.width() - 130.0);
        p.galley(pos2(inner.left(), inner.center().y - g.size().y / 2.0), g, MUTED);
        let mut row = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
        let s = select_btn(&mut row, DUP_MINS[self.dup_min].1, 100.0);
        if s.clicked() {
            self.menu = Some(Menu { pos: s.rect.left_bottom() + vec2(0.0, 4.0), kind: MenuKind::DupMin, opened: self.ctx.input(|i| i.time) });
        }
        let list = Rect::from_min_max(pos2(body.left(), tb.bottom()), body.max).shrink(8.0);
        let mut c = child(ui, list);
        let Some(groups) = self.dups.clone() else {
            p.text(list.center_top() + vec2(0.0, 56.0), Align2::CENTER_CENTER, "Looking for duplicates…", f(13.0), DIM);
            return;
        };
        if groups.is_empty() {
            p.text(list.center_top() + vec2(0.0, 48.0), Align2::CENTER_CENTER, "No likely duplicates", sb(13.0), MUTED);
            p.text(list.center_top() + vec2(0.0, 68.0), Align2::CENTER_CENTER, "Nothing with the same name and size above the threshold.", f(12.5), DIM);
            return;
        }
        let mut actions = Vec::new();
        let ext_colors = self.ext_colors.clone();
        egui::ScrollArea::vertical().id_salt("dups").auto_shrink([false, false]).show(&mut c, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let w = ui.available_width();
            let wasted: u64 = groups.iter().map(|g| g.wasted).sum();
            let (hr, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
            let g = rich(ui, &[(&format!("{} groups · up to ", groups.len()), f(11.5), DIM), (&fmt_bytes(wasted), sb(11.5), hex(0xfca5a5)), (" reclaimable", f(11.5), DIM)], w);
            ui.painter().galley(pos2(hr.left() + 6.0, hr.top() + 4.0), g, DIM);
            for grp in &groups {
                let h = 38.0 + grp.items.len() as f32 * 28.0;
                let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
                let p = ui.painter();
                let gr = Rect::from_min_max(r.min, pos2(r.right(), r.bottom()));
                p.rect_filled(gr, 12, white(0.015));
                let hdr = Rect::from_min_size(gr.min, vec2(gr.width(), 38.0));
                p.rect_filled(hdr, egui::CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }, white(0.025));
                p.rect_stroke(gr, 12, Stroke::new(1.0_f32, line()), StrokeKind::Inside);
                let first = &grp.items[0];
                icons::paint(ui, icons::FILE, Rect::from_center_size(pos2(hdr.left() + 20.0, hdr.center().y), vec2(16.0, 16.0)), ext_colors.get(first.ext as usize).copied().unwrap_or(DIM));
                let wtxt = format!("{} extra", fmt_bytes(grp.wasted));
                let ww = text_w(ui, &wtxt, monob(11.0)) + 14.0;
                let wr = Rect::from_min_size(pos2(hdr.right() - 12.0 - ww, hdr.center().y - 10.0), vec2(ww, 20.0));
                p.rect_filled(wr, 6, rgba(0xf43f5e, 0.12));
                p.text(wr.center(), Align2::CENTER_CENTER, &wtxt, monob(11.0), hex(0xfca5a5));
                let stxt = format!("{} × {}", grp.items.len(), fmt_bytes(grp.size));
                let sr = p.text(pos2(wr.left() - 10.0, hdr.center().y), Align2::RIGHT_CENTER, &stxt, mono(11.5), MUTED);
                text(ui, p, pos2(hdr.left() + 38.0, hdr.center().y), Align2::LEFT_CENTER, &first.name, sb(12.5), TEXT, sr.left() - hdr.left() - 50.0);
                for (k, it) in grp.items.iter().enumerate() {
                    let ir = Rect::from_min_size(pos2(gr.left() + 1.0, hdr.bottom() + k as f32 * 28.0), vec2(gr.width() - 2.0, 28.0));
                    let resp = ui.interact(ir, Id::new(("dup", it.id)), Sense::click());
                    if resp.hovered() {
                        p.rect_filled(ir, 0, white(0.03));
                    }
                    let col = if resp.hovered() { TEXT } else { MUTED };
                    let dt = fmt_date(it.modified);
                    let dw = text_w(ui, &dt, mono(11.0));
                    p.text(pos2(ir.right() - 12.0, ir.center().y), Align2::RIGHT_CENTER, &dt, mono(11.0), DIM);
                    text_left_elide(ui, p, Rect::from_min_max(pos2(ir.left() + 33.0, ir.top()), pos2(ir.right() - 24.0 - dw, ir.bottom())), it.path.as_deref().unwrap_or_default(), f(12.0), col);
                    if resp.double_clicked() {
                        actions.push(A::Reveal(it.id));
                    }
                    if resp.secondary_clicked() {
                        actions.push(A::SelectFind(it.id));
                        actions.push(A::Ctx(resp.interact_pointer_pos().unwrap_or(ir.center()), it.id));
                    }
                }
                ui.allocate_space(vec2(w, 8.0));
            }
        });
        self.apply(actions);
    }

    fn clean_tab(&mut self, ui: &mut Ui, body: Rect) {
        let p = ui.painter().clone();
        let head = Rect::from_min_size(body.min, vec2(body.width(), 70.0));
        hline(&p, body.left(), body.right(), head.bottom() - 1.0, line());
        let inner = head.shrink(14.0);
        let items = self.clean.clone().unwrap_or_default();
        let sel_total: u64 = items.iter().filter(|c| self.clean_sel.contains(&c.row.id)).map(|c| c.row.alloc).sum();
        let any_sel = items.iter().any(|c| self.clean_sel.contains(&c.row.id));
        let mut row = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
        row.spacing_mut().item_spacing.x = 12.0;
        if btn(&mut row, "Clean selected", Some(icons::SPARKLE), Btn::Success, false, any_sel).clicked() {
            self.confirm_clean();
        }
        let g = rich(&row, &[("Selected: ", f(12.0), MUTED), (&fmt_bytes(sel_total), monob(12.0), hex(0x6ee7b7))], 300.0);
        let (gr, _) = row.allocate_exact_size(g.size(), Sense::hover());
        row.painter().galley(gr.min, g, MUTED);
        let left_w = row.min_rect().left() - inner.left() - 16.0;
        p.text(inner.left_top(), Align2::LEFT_TOP, "Smart cleanup", sb(14.0), TEXT);
        let g = rich(ui, &[("Caches and temp folders that apps rebuild on their own. Contents are deleted permanently; files in use are skipped.", f(12.0), MUTED)], left_w);
        p.galley(inner.left_top() + vec2(0.0, 21.0), g, MUTED);

        let grid = Rect::from_min_max(pos2(body.left(), head.bottom()), body.max).shrink(12.0);
        if self.clean.is_none() {
            p.text(grid.center_top() + vec2(0.0, 48.0), Align2::CENTER_CENTER, "Looking for caches…", f(13.0), DIM);
            return;
        }
        if items.is_empty() {
            p.text(grid.center_top() + vec2(0.0, 48.0), Align2::CENTER_CENTER, "Nothing to clean here", sb(13.0), MUTED);
            p.text(grid.center_top() + vec2(0.0, 68.0), Align2::CENTER_CENTER, "No known cache or temp folders were found in this view.", f(12.5), DIM);
            return;
        }
        let mut c = child(ui, grid);
        let mut toggles = Vec::new();
        let mut ctxs = Vec::new();
        egui::ScrollArea::vertical().id_salt("clean").auto_shrink([false, false]).show(&mut c, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let w = ui.available_width();
            let cols = ((w + 10.0) / 270.0).floor().max(1.0) as usize;
            let cw = (w - 10.0 * (cols as f32 - 1.0)) / cols as f32;
            let mut y = 0.0;
            let top = ui.min_rect().top();
            let left = ui.min_rect().left();
            for chunk in items.chunks(cols) {
                let notes: Vec<_> = chunk.iter().map(|c| rich(ui, &[(c.note, f(11.5), MUTED)], cw - 26.0 - 29.0)).collect();
                let h = notes.iter().map(|g| 13.0 + 18.0 + 16.0 + 5.0 + g.size().y + 6.0 + 15.0 + 13.0).fold(0.0, f32::max);
                for (k, (it, note)) in chunk.iter().zip(notes).enumerate() {
                    let r = Rect::from_min_size(pos2(left + k as f32 * (cw + 10.0), top + y), vec2(cw, h));
                    let resp = ui.interact(r, Id::new(("cc", it.row.id)), Sense::click());
                    let on = self.clean_sel.contains(&it.row.id);
                    let p = ui.painter();
                    p.rect_filled(r, 13, if on { rgba(0x10b981, 0.07) } else { white(0.02) });
                    p.rect_stroke(r, 13, Stroke::new(1.0_f32, if on { rgba(0x10b981, 0.5) } else if resp.hovered() { line2() } else { line() }), StrokeKind::Inside);
                    let ir = r.shrink(13.0);
                    checkbox(p, Rect::from_min_size(ir.min + vec2(0.0, 1.0), vec2(18.0, 18.0)), on, EMERALD);
                    let x = ir.left() + 29.0;
                    let sz = fmt_bytes(it.row.alloc);
                    let szr = p.text(pos2(ir.right(), ir.top()), Align2::RIGHT_TOP, &sz, monob(12.0), hex(0x6ee7b7));
                    text(ui, p, pos2(x, ir.top()), Align2::LEFT_TOP, it.label, sb(13.0), TEXT, szr.left() - x - 8.0);
                    let cat = spaced(ui, &format!("{} · {} FILES", it.category.to_uppercase(), fmt_num(it.row.files as u64)), sb(10.0), DIM, 0.6);
                    p.galley(pos2(x, ir.top() + 20.0), cat, DIM);
                    let nh = note.size().y;
                    p.galley(pos2(x, ir.top() + 39.0), note, MUTED);
                    text_left_elide(ui, p, Rect::from_min_size(pos2(x, ir.top() + 45.0 + nh), vec2(ir.right() - x, 14.0)), it.row.path.as_deref().unwrap_or_default(), mono(10.5), DIM);
                    if resp.clicked() {
                        toggles.push(it.row.id);
                    }
                    if resp.secondary_clicked() {
                        ctxs.push((resp.interact_pointer_pos().unwrap_or(r.center()), it.row.id));
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                }
                y += h + 10.0;
            }
            ui.allocate_space(vec2(w, y));
        });
        for id in toggles {
            if !self.clean_sel.remove(&id) {
                self.clean_sel.insert(id);
            }
        }
        self.apply(ctxs.into_iter().map(|(p, id)| A::Ctx(p, id)).collect());
    }

    fn types_panel(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        panel(&p, rect);
        let head = Rect::from_min_size(rect.min, vec2(rect.width(), 40.0));
        p.text(pos2(head.left() + 14.0, head.top() + 12.0), Align2::LEFT_TOP, "File types", sb(13.0), TEXT);
        p.text(pos2(head.right() - 14.0, head.top() + 13.0), Align2::RIGHT_TOP, format!("{} types", fmt_num(self.types.len() as u64)), f(12.0), MUTED);
        let list = Rect::from_min_max(pos2(rect.left() + 6.0, head.bottom()), rect.max - vec2(6.0, 8.0));
        let mut c = child(ui, list);
        c.spacing_mut().item_spacing = Vec2::ZERO;
        let total: u64 = self.types.iter().map(|s| if self.mode == Mode::Alloc { s.alloc } else { s.size }).sum::<u64>().max(1);
        let n = self.types.len().min(400);
        let mut clicked: Option<(u16, bool)> = None;
        egui::ScrollArea::vertical().id_salt("types").auto_shrink([false, false]).show_rows(&mut c, 42.0, n, |ui, range| {
            for i in range {
                let s = &self.types[i];
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
                let r = r.shrink2(vec2(0.0, 1.0));
                let p = ui.painter();
                let on = self.ext_filter == Some(s.id);
                if on {
                    p.rect_filled(r, 9, rgba(0x6366f1, 0.16));
                    p.rect_stroke(r, 9, Stroke::new(1.0_f32, rgba(0x818cf8, 0.35)), StrokeKind::Inside);
                } else if resp.hovered() {
                    p.rect_filled(r, 9, white(0.04));
                }
                let c = self.ext_color(s.id);
                let v = if self.mode == Mode::Alloc { s.alloc } else { s.size };
                p.rect_filled(Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 6.0), vec2(12.0, 12.0)), 4, c);
                let label = if s.id == 0 { "No extension".to_string() } else { format!(".{}", self.ext_names.get(s.id as usize).cloned().unwrap_or_default()) };
                let vr = p.text(pos2(r.right() - 8.0, r.top() + 7.0), Align2::RIGHT_TOP, fmt_bytes(v), mono(11.5), TEXT2);
                let lr = text(ui, p, pos2(r.left() + 29.0, r.top() + 6.0), Align2::LEFT_TOP, &label, sb(12.5), TEXT, vr.left() - r.left() - 80.0);
                p.text(pos2(lr.right() + 6.0, lr.center().y + 1.0), Align2::LEFT_CENTER, fmt_num(s.count), mono(10.5), DIM);
                let bar_r = Rect::from_min_max(pos2(r.left() + 29.0, r.bottom() - 11.0), pos2(r.right() - 8.0, r.bottom() - 7.0));
                p.rect_filled(bar_r, 9, white(0.05));
                let fw = bar_r.width() * (v as f32 / total as f32);
                if fw > 0.3 {
                    p.rect_filled(Rect::from_min_size(bar_r.min, vec2(fw.max(4.0), 4.0)), 9, c);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    resp.clone().on_hover_text(format!("{label}: {} files (double-click to list them)", fmt_num(s.count)));
                }
                if resp.double_clicked() {
                    clicked = Some((s.id, true));
                } else if resp.clicked() {
                    clicked = Some((s.id, false));
                }
            }
        });
        if let Some((id, dbl)) = clicked {
            if dbl {
                self.q = if id == 0 { String::new() } else { format!("ext:{}", self.ext_names[id as usize]) };
                self.switch_tab(Tab::Search);
                self.run_find();
            } else {
                self.ext_filter = if self.ext_filter == Some(id) { None } else { Some(id) };
            }
        }
    }

    fn visual_panel(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        panel(&p, rect);
        let head = Rect::from_min_size(rect.min, vec2(rect.width(), 47.0));
        hline(&p, rect.left(), rect.right(), head.bottom() - 1.0, line());
        let hin = head.shrink2(vec2(10.0, 8.0));
        let mut row = hchild(ui, hin);
        row.spacing_mut().item_spacing.x = 12.0;
        let sel = if self.vis == Vis::Treemap { 0 } else { 1 };
        if let Some(i) = segmented(&mut row, "vis", &[("Treemap", Some(icons::GRID)), ("Sunburst", Some(icons::SUN))], sel, true) {
            self.vis = if i == 0 { Vis::Treemap } else { Vis::Sunburst };
            self.save_settings();
        }
        let info_left = row.min_rect().right() + 12.0;
        let mut right = ui.new_child(UiBuilder::new().max_rect(hin).layout(Layout::right_to_left(Align::Center)));
        if btn(&mut right, "Up", Some(icons::UP), Btn::Ghost, true, self.focus != self.root).clicked() {
            self.focus_up();
        }
        let info_r = Rect::from_min_max(pos2(info_left, hin.top()), pos2(right.min_rect().left() - 12.0, hin.bottom()));
        match &self.vis_info {
            Some(s) => {
                let (name, rest) = s.split_once('\u{1}').unwrap_or((s, ""));
                let g = rich(ui, &[(name, sb(12.0), TEXT), (rest, f(12.0), MUTED)], 10_000.0);
                let mut job = (*g.job).clone();
                job.wrap = TextWrapping::truncate_at_width(info_r.width());
                let g = ui.fonts(|fo| fo.layout_job(job));
                p.galley(pos2(info_r.left(), info_r.center().y - g.size().y / 2.0), g, TEXT);
            }
            None => {
                let hint = if self.vis == Vis::Treemap { "Hover to inspect · click to select · double-click a folder to zoom in" } else { "Click a ring to zoom in · click the center to go up" };
                text(ui, &p, pos2(info_r.left(), info_r.center().y), Align2::LEFT_CENTER, hint, f(12.0), MUTED, info_r.width());
            }
        }
        let body = Rect::from_min_max(pos2(rect.left(), head.bottom()), rect.max).shrink(8.0);
        p.rect_filled(body, 9, BG);
        match self.vis {
            Vis::Treemap => self.treemap_ui(ui, body),
            Vis::Sunburst => self.sunburst_ui(ui, body),
        }
    }

    fn treemap_ui(&mut self, ui: &mut Ui, body: Rect) {
        self.request_treemap(body.width(), body.height());
        let origin = body.min;
        let Some(mesh) = viz::get_treemap_mesh(self, origin) else { return };
        let p = ui.painter().with_clip_rect(body);
        p.add(Shape::Mesh(mesh));
        let Some(tm) = &self.tm else { return };
        let mut boxes: Vec<Rect> = Vec::new();
        for (i, name) in &tm.labels {
            let r = tm.rects[*i];
            let rect = Rect::from_min_size(origin + vec2(r.x, r.y), vec2(r.w, r.h));
            let lw = (text_w(ui, name, sb(11.0)) + 4.0).min(rect.width() - 6.0);
            let lb = Rect::from_min_size(rect.min, vec2(lw, 18.0));
            if boxes.iter().any(|b| b.intersects(lb)) || boxes.len() >= 60 {
                continue;
            }
            boxes.push(lb);
            let lp = p.with_clip_rect(Rect::from_min_size(rect.min + vec2(3.0, 2.0), vec2(rect.width() - 6.0, 16.0)).intersect(body));
            lp.text(rect.min + vec2(6.0, 5.0), Align2::LEFT_TOP, name, sb(11.0), Color32::from_black_alpha(140));
            lp.text(rect.min + vec2(5.0, 4.0), Align2::LEFT_TOP, name, sb(11.0), white(0.92));
        }

        let resp = ui.interact(body, Id::new("tm"), Sense::click());
        let hover_pos = resp.hover_pos();
        let hit = hover_pos.and_then(|hp| viz::tm_hit(&tm.rects, (hp - origin).to_pos2()));
        let rects_len = tm.rects.len();
        let mark = |i: usize, col: Color32, w: f32, dashed: bool| {
            let r = tm.rects[i];
            let rect = Rect::from_min_size(origin + vec2(r.x, r.y), vec2(r.w, r.h)).shrink(w / 2.0);
            if dashed {
                let pts = vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
                for s in Shape::dashed_line(&pts, Stroke::new(w, col), 5.0, 3.0) {
                    p.add(s);
                }
            } else {
                p.rect_stroke(rect, 0, Stroke::new(w, col), StrokeKind::Middle);
            }
        };
        if let Some(i) = hit {
            if let Some(par) = viz::tm_parent(&tm.rects, i).filter(|&j| j > 0) {
                mark(par, rgba(0x818cf8, 0.9), 1.5, true);
            }
            mark(i, Color32::WHITE, 2.0, false);
        }
        if let Some(s) = self.sel.filter(|&s| s != self.focus) {
            if let Some(i) = tm.rects.iter().position(|r| r.id == s) {
                mark(i, FOLDER, 2.0, false);
            }
        }
        let _ = rects_len;
        let hit_rect = hit.map(|i| tm.rects[i]);
        let parent_rect = hit.and_then(|i| viz::tm_parent(&tm.rects, i)).map(|j| tm.rects[j]);
        self.tm_hover = hit;
        match hit_rect {
            Some(r) => {
                if let Some(d) = self.node(r.id) {
                    let color = if r.dir { DIR_FILL } else { self.ext_color(r.ext) };
                    self.vis_info = Some(format!("{}\u{1} · {} · {}", d.row.name, fmt_bytes(self.val(&d.row)), d.row.path.clone().unwrap_or_default()));
                    if let Some(hp) = hover_pos {
                        if self.menu.is_none() {
                            self.tooltip(ui.ctx(), hp, &d, color);
                        }
                    }
                }
                if resp.clicked() && hit != Some(0) {
                    self.reveal_in_tree(r.id);
                }
                if resp.double_clicked() {
                    if r.dir {
                        self.set_focus(r.id);
                    } else if let Some(pr) = parent_rect.filter(|_| hit != Some(0)) {
                        self.set_focus(pr.id);
                    }
                }
                if resp.secondary_clicked() {
                    if let Some(d) = self.node(r.id) {
                        self.menu = Some(Menu { pos: hover_pos.unwrap_or(body.center()), kind: MenuKind::Node(Box::new(d)), opened: self.ctx.input(|i| i.time) });
                    }
                }
            }
            None => {
                if resp.hovered() || self.vis_info.is_some() && !resp.hovered() {
                    self.vis_info = None;
                }
            }
        }
    }

    fn sunburst_ui(&mut self, ui: &mut Ui, body: Rect) {
        self.request_sunburst();
        let g = SunGeom::new(body);
        let resp = ui.interact(body, Id::new("sun"), Sense::click());
        let hit = resp.hover_pos().and_then(|hp| viz::sun_hit(self, &g, hp));
        let new_hover = match hit {
            Some(Ok(i)) => Some(i),
            _ => None,
        };
        self.sun_hover = new_hover;
        let p = ui.painter().with_clip_rect(body);
        p.add(Shape::mesh(viz::build_sun_mesh(self, &g)));
        p.circle_filled(g.center, g.r0 - 3.0, hex(0x10151f));
        p.circle_stroke(g.center, g.r0 - 3.0, Stroke::new(1.0_f32, white(0.08)));
        if let Some(fd) = self.focus_detail.clone() {
            let fv = self.val(&fd.row);
            let (name, size, sub) = match new_hover.and_then(|i| self.sun.get(i).cloned()) {
                Some(a) => (a.name.clone(), fmt_bytes(a.value), format!("{} of view", fmt_pct(a.value as f64 / fv.max(1) as f64 * 100.0))),
                None => {
                    let is_root = fd.row.id == self.root;
                    (
                        if is_root { self.info.as_ref().map(|i| i.title.clone()).unwrap_or_default() } else { fd.row.name.clone() },
                        fmt_bytes(fv),
                        if is_root { format!("{} files", fmt_num(fd.row.files as u64)) } else { "click to go up".into() },
                    )
                }
            };
            let c = g.center;
            let maxw = (g.r0 - 3.0) * 2.0 - 20.0;
            if g.r0 >= 46.0 {
                text(ui, &p, c + vec2(0.0, -14.0), Align2::CENTER_BOTTOM, &name, f(11.5), MUTED, maxw);
                p.text(c + vec2(0.0, -12.0), Align2::CENTER_TOP, size, monob(18.0), TEXT);
                text(ui, &p, c + vec2(0.0, 12.0), Align2::CENTER_TOP, &sub, mono(10.5), DIM, maxw);
            } else if g.r0 >= 26.0 {
                let fs = (g.r0 * 0.42).clamp(8.0, 18.0);
                text(ui, &p, c, Align2::CENTER_CENTER, &size, monob(fs), TEXT, (g.r0 - 3.0) * 2.0 - 4.0);
            }
        }
        if hit.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if resp.clicked() {
            match hit {
                Some(Err(())) => self.focus_up(),
                Some(Ok(i)) => {
                    let a = self.sun[i].clone();
                    if a.dir { self.set_focus(a.id) } else { self.reveal_in_tree(a.id) }
                }
                None => {}
            }
        }
        if resp.secondary_clicked() {
            if let Some(Ok(i)) = hit {
                let id = self.sun[i].id;
                if let Some(d) = self.node(id) {
                    self.menu = Some(Menu { pos: resp.interact_pointer_pos().unwrap_or(body.center()), kind: MenuKind::Node(Box::new(d)), opened: self.ctx.input(|i| i.time) });
                }
            }
        }
    }

    fn tooltip(&self, ctx: &egui::Context, at: Pos2, d: &NodeDetail, color: Color32) {
        let r = &d.row;
        let pct = if d.parent_value > 0 { self.val(r) as f64 / d.parent_value as f64 * 100.0 } else { 100.0 };
        let mut rows: Vec<(&str, String)> = vec![("Allocated", fmt_bytes(r.alloc)), ("Size", fmt_bytes(r.size)), ("Of parent", fmt_pct(pct))];
        if r.dir {
            rows.push(("Files", fmt_num(r.files as u64)));
            rows.push(("Folders", fmt_num(r.folders as u64)));
        }
        if r.modified > 0 {
            rows.push(("Modified", fmt_date(r.modified)));
        }
        let w = 300.0;
        egui::Area::new(Id::new("tm-tip")).order(Order::Tooltip).interactable(false).fixed_pos(pos2(-10_000.0, 0.0)).show(ctx, |_| {});
        let path = r.path.clone().unwrap_or_default();
        let screen = ctx.screen_rect();
        let est_h = 52.0 + rows.len() as f32 * 17.0 + if path.len() > 46 { 14.0 } else { 0.0 };
        let mut pos = at + vec2(16.0, 16.0);
        if pos.x + w > screen.right() - 8.0 {
            pos.x = at.x - w - 12.0;
        }
        if pos.y + est_h > screen.bottom() - 8.0 {
            pos.y = at.y - est_h - 12.0;
        }
        egui::Area::new(Id::new("tm-tip2")).order(Order::Tooltip).interactable(false).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::new()
                .fill(rgba(0x141a26, 0.97))
                .stroke(Stroke::new(1.0_f32, line2()))
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(11, 9))
                .shadow(egui::epaint::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(200) })
                .show(ui, |ui| {
                    ui.set_width(w - 22.0);
                    ui.spacing_mut().item_spacing = vec2(0.0, 3.0);
                    ui.horizontal(|ui| {
                        let (sw, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                        ui.painter().rect_filled(sw, 3, color);
                        ui.add_space(7.0);
                        ui.label(egui::RichText::new(&r.name).font(sb(12.0)).color(TEXT));
                    });
                    ui.label(egui::RichText::new(&path).font(mono(11.0)).color(DIM));
                    ui.add_space(3.0);
                    for (k, v) in rows {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(k).font(mono(11.5)).color(MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(egui::RichText::new(v).font(monob(11.5)).color(TEXT));
                            });
                        });
                    }
                });
        });
    }

    fn overlays(&mut self, ctx: &egui::Context) {
        if self.about {
            self.about_ui(ctx);
        }
        self.menu_ui(ctx);
        self.scan_overlay(ctx);
        self.modal_ui(ctx);
        self.toasts_ui(ctx);
    }

    fn scan_overlay(&mut self, ctx: &egui::Context) {
        let Some(scan) = &self.scan else { return };
        ctx.request_repaint();
        let screen = ctx.screen_rect();
        let ctl = scan.ctl.clone();
        let title = scan.title.clone();
        let elapsed = scan.started.elapsed().as_secs_f32();
        let mut cancel = false;
        egui::Area::new(Id::new("scan-ov")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            let p = ui.painter();
            ui.interact(screen, Id::new("scan-block"), Sense::click_and_drag());
            p.rect_filled(screen, 0, rgba(0x06080c, 0.78));
            let card = Rect::from_center_size(screen.center(), vec2(440.0, 330.0));
            shadow(p, card, 22, 80, 40, Color32::from_black_alpha(200));
            theme::grad_rrect(p, card, 22.0, |pt| lerp(SURFACE2, SURFACE, (pt.y - card.top()) / card.height()));
            p.rect_stroke(card, 22, Stroke::new(1.0_f32, line2()), StrokeKind::Inside);
            let cx = card.center().x;
            let ring_c = pos2(cx, card.top() + 30.0 + 42.0);
            let t = ui.input(|i| i.time) as f32;
            spinner_ring(p, ring_c, 42.0, 35.0, t * std::f32::consts::TAU / 1.1);
            icons::paint(ui, icons::LOGO, Rect::from_center_size(ring_c, vec2(36.0, 36.0)), INDIGO2);
            let mut y = card.top() + 30.0 + 84.0 + 18.0;
            p.text(pos2(cx, y), Align2::CENTER_TOP, &title, b(18.0), TEXT);
            y += 28.0;
            let phase = ctl.phase.lock().map(|s| s.clone()).unwrap_or_default();
            p.text(pos2(cx, y), Align2::CENTER_TOP, &phase, f(13.0), MUTED);
            y += 32.0;
            let done = ctl.done.load(std::sync::atomic::Ordering::Relaxed);
            let total = ctl.total.load(std::sync::atomic::Ordering::Relaxed);
            let unit = if phase.contains("Master File Table") { " records" } else { " items" };
            let count = if done > 0 { format!("{}{}", fmt_num(done), unit) } else { "0".into() };
            prism_text(ui, p, pos2(cx, y), Align2::CENTER_TOP, &count, monob(30.0));
            y += 50.0;
            let bar_r = Rect::from_min_size(pos2(card.left() + 30.0, y), vec2(380.0, 6.0));
            p.rect_filled(bar_r, 99, white(0.07));
            if total > 0 && !phase.starts_with("Building") {
                let fw = 380.0 * (done as f32 / total as f32).min(1.0);
                if fw > 1.0 {
                    prism_bar(p, Rect::from_min_size(bar_r.min, vec2(fw.max(6.0), 6.0)), 3.0);
                }
            } else {
                let w = 114.0;
                let pos = ((t / 1.2).fract() * 4.4 - 1.0) * w;
                let seg = Rect::from_min_size(pos2(bar_r.left() + pos, bar_r.top()), vec2(w, 6.0)).intersect(bar_r);
                if seg.width() > 1.0 {
                    prism_bar(&p.with_clip_rect(bar_r), seg, 3.0);
                }
            }
            y += 16.0;
            let cur = ctl.current.lock().map(|s| s.clone()).unwrap_or_default();
            text_left_elide(ui, p, Rect::from_min_size(pos2(card.left() + 30.0, y), vec2(380.0, 16.0)), &cur, mono(11.0), DIM);
            let fy = card.bottom() - 34.0;
            p.text(pos2(card.left() + 30.0, fy), Align2::LEFT_CENTER, format!("{elapsed:.1} s"), mono(12.0), MUTED);
            let o = BtnOpts { label: "Cancel", icon: None, kind: Btn::Ghost, small: true, enabled: true };
            let sz = btn_size(ui, &o);
            let br = Rect::from_min_size(pos2(card.right() - 30.0 - sz.x, fy - sz.y / 2.0), sz);
            if btn_at(ui, br, Id::new("scan-cancel"), &o).clicked() {
                cancel = true;
            }
        });
        if cancel {
            self.cancel_scan();
        }
    }

    fn menu_ui(&mut self, ctx: &egui::Context) {
        let Some(menu) = &self.menu else { return };
        let now = ctx.input(|i| i.time);
        let pos = menu.pos;
        let kind = menu.kind.clone();
        let opened = menu.opened;
        #[derive(Clone)]
        enum Item {
            Header(String),
            Sep,
            Act(&'static str, Option<Icon>, String, Option<&'static str>, bool),
        }
        let mut items: Vec<Item> = Vec::new();
        let mut width = 250.0;
        match &kind {
            MenuKind::Node(d) => {
                let r = &d.row;
                let csv = self.info.as_ref().map(|i| i.kind == "csv").unwrap_or(true);
                items.push(Item::Header(r.path.clone().unwrap_or_default()));
                items.push(Item::Act("open", Some(icons::OPEN), "Open".into(), None, false));
                items.push(Item::Act("reveal", Some(icons::FOLDER_O), "Show in Explorer".into(), None, false));
                items.push(Item::Act("copy", Some(icons::COPY), "Copy path".into(), Some("Ctrl+C"), false));
                if r.dir {
                    items.push(Item::Sep);
                    items.push(Item::Act("focus", Some(icons::TARGET), "Zoom visuals here".into(), None, false));
                    items.push(Item::Act("largest", Some(icons::LIST), "Largest files in here".into(), None, false));
                    if !csv {
                        items.push(Item::Act("scan", Some(icons::SCAN), "Scan only this folder".into(), None, false));
                    }
                }
                if r.id != self.root && !csv {
                    items.push(Item::Sep);
                    items.push(Item::Act("recycle", Some(icons::TRASH), "Move to Recycle Bin".into(), Some("Del"), true));
                    items.push(Item::Act("delete", Some(icons::X), "Delete permanently".into(), Some("Shift+Del"), true));
                }
            }
            MenuKind::MinSize => {
                for (i, (_, l)) in MIN_SIZES.iter().enumerate() {
                    items.push(Item::Act(["0", "1", "2", "3"][i], None, l.to_string(), (i == self.q_min).then_some("✓"), false));
                }
                width = 170.0;
            }
            MenuKind::Age => {
                for (i, (_, l)) in AGES.iter().enumerate() {
                    items.push(Item::Act(["0", "1", "2", "3", "4"][i], None, l.to_string(), (i == self.q_age).then_some("✓"), false));
                }
                width = 210.0;
            }
            MenuKind::DupMin => {
                for (i, (_, l)) in DUP_MINS.iter().enumerate() {
                    items.push(Item::Act(["0", "1", "2"][i], None, l.to_string(), (i == self.dup_min).then_some("✓"), false));
                }
                width = 150.0;
            }
        }
        let h: f32 = items.iter().map(|i| match i { Item::Header(_) => 26.0, Item::Sep => 9.0, Item::Act(..) => 31.0 }).sum::<f32>() + 10.0;
        let screen = ctx.screen_rect();
        let pos = pos2(pos.x.min(screen.right() - width - 8.0), pos.y.min(screen.bottom() - h - 8.0));
        let mut chosen: Option<&'static str> = None;
        let area = egui::Area::new(Id::new("menu")).order(Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            let rect = Rect::from_min_size(pos, vec2(width, h));
            ui.allocate_rect(rect, Sense::click());
            let p = ui.painter();
            shadow(p, rect, 12, 50, 24, Color32::from_black_alpha(200));
            p.rect_filled(rect, 12, rgba(0x182030, 0.98));
            p.rect_stroke(rect, 12, Stroke::new(1.0_f32, line2()), StrokeKind::Inside);
            let mut y = rect.top() + 5.0;
            for it in &items {
                match it {
                    Item::Header(s) => {
                        text_left_elide(ui, p, Rect::from_min_size(pos2(rect.left() + 15.0, y), vec2(width - 30.0, 26.0)), s, f(11.0), DIM);
                        y += 26.0;
                    }
                    Item::Sep => {
                        hline(p, rect.left() + 11.0, rect.right() - 11.0, y + 4.0, line());
                        y += 9.0;
                    }
                    Item::Act(key, ic, label, kbd, danger) => {
                        let r = Rect::from_min_size(pos2(rect.left() + 5.0, y), vec2(width - 10.0, 31.0));
                        let resp = ui.interact(r, Id::new(("mi", *key, label.as_str())), Sense::click());
                        if resp.hovered() {
                            p.rect_filled(r, 8, if *danger { rgba(0xf43f5e, 0.18) } else { rgba(0x6366f1, 0.25) });
                        }
                        let fg = if *danger { hex(0xfda4af) } else { TEXT };
                        let mut x = r.left() + 10.0;
                        if let Some(ic) = ic {
                            icons::paint(ui, *ic, Rect::from_center_size(pos2(x + 7.5, r.center().y), vec2(15.0, 15.0)), if *danger { hex(0xfb7185) } else { MUTED });
                            x += 25.0;
                        }
                        p.text(pos2(x, r.center().y), Align2::LEFT_CENTER, label, f(12.5), fg);
                        if let Some(k) = kbd {
                            let (font, col) = if *k == "✓" { (sb(12.0), INDIGO2) } else { (mono(10.5), DIM) };
                            p.text(pos2(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, *k, font, col);
                        }
                        if resp.clicked() {
                            chosen = Some(key);
                        }
                        y += 31.0;
                    }
                }
            }
        });
        let menu_rect = area.response.rect;
        let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().map(|p| !menu_rect.contains(p)).unwrap_or(false));
        if outside && now - opened > 0.05 {
            self.menu = None;
        }
        let Some(key) = chosen else { return };
        self.menu = None;
        match kind {
            MenuKind::Node(d) => {
                let id = d.row.id;
                match key {
                    "open" => self.open_item(id),
                    "reveal" => self.reveal(id),
                    "copy" => self.copy_path(id),
                    "focus" => self.set_focus(id),
                    "largest" => {
                        self.set_focus(id);
                        self.switch_tab(Tab::Search);
                    }
                    "scan" => {
                        let p = d.row.path.clone().unwrap_or_default();
                        self.start_scan("folder", &p);
                    }
                    "recycle" => self.confirm_delete(vec![id], false),
                    "delete" => self.confirm_delete(vec![id], true),
                    _ => {}
                }
            }
            MenuKind::MinSize => {
                self.q_min = key.parse().unwrap_or(0);
                self.run_find();
            }
            MenuKind::Age => {
                self.q_age = key.parse().unwrap_or(0);
                self.run_find();
            }
            MenuKind::DupMin => {
                self.dup_min = key.parse().unwrap_or(1);
                self.load_dups();
            }
        }
    }

    fn modal_ui(&mut self, ctx: &egui::Context) {
        let Some(m) = &self.modal else { return };
        let screen = ctx.screen_rect();
        let mut result: Option<bool> = None;
        egui::Area::new(Id::new("modal")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            ui.interact(screen, Id::new("modal-block"), Sense::click_and_drag());
            let p = ui.painter();
            p.rect_filled(screen, 0, rgba(0x06080c, 0.66));
            let w = 460.0;
            let iw = w - 52.0;
            let mut parts: Vec<(&str, egui::FontId, Color32)> = Vec::new();
            let warn = m.warn.clone().map(|s| s + " ");
            if let Some(wt) = &warn {
                parts.push((wt.as_str(), sb(12.5), hex(0xfda4af)));
            }
            parts.push((m.body.as_str(), f(12.5), MUTED));
            let body_g = rich(ui, &parts, iw);
            let list_h = if m.list.is_empty() { 0.0 } else { (m.list.len() as f32 * 19.0 + 12.0).min(160.0) + 10.0 };
            let h = 26.0 + 46.0 + 14.0 + 22.0 + 8.0 + body_g.size().y + list_h + 20.0 + 38.0 + 26.0;
            let card = Rect::from_center_size(screen.center(), vec2(w, h));
            shadow(p, card, 20, 80, 40, Color32::from_black_alpha(215));
            theme::grad_rrect(p, card, 20.0, |pt| lerp(SURFACE2, SURFACE, (pt.y - card.top()) / card.height()));
            p.rect_stroke(card, 20, Stroke::new(1.0_f32, line2()), StrokeKind::Inside);
            let x = card.left() + 26.0;
            let mut y = card.top() + 26.0;
            let ib = Rect::from_min_size(pos2(x, y), vec2(46.0, 46.0));
            let (bg, st, fg) = if m.danger { (rgba(0xf43f5e, 0.14), rgba(0xf43f5e, 0.3), hex(0xfb7185)) } else { (rgba(0x10b981, 0.14), rgba(0x10b981, 0.3), MINT) };
            p.rect_filled(ib, 14, bg);
            p.rect_stroke(ib, 14, Stroke::new(1.0_f32, st), StrokeKind::Inside);
            icons::paint(ui, if m.danger { icons::TRASH } else { icons::SPARKLE }, Rect::from_center_size(ib.center(), vec2(22.0, 22.0)), fg);
            y += 46.0 + 14.0;
            text(ui, p, pos2(x, y), Align2::LEFT_TOP, &m.title, sb(16.0), TEXT, iw);
            y += 30.0;
            let bh = body_g.size().y;
            p.galley(pos2(x, y), body_g, MUTED);
            y += bh + 10.0;
            if !m.list.is_empty() {
                let lr = Rect::from_min_size(pos2(x, y), vec2(iw, list_h - 10.0));
                p.rect_filled(lr, 10, BG);
                p.rect_stroke(lr, 10, Stroke::new(1.0_f32, line()), StrokeKind::Inside);
                let lp = p.with_clip_rect(lr.shrink(2.0));
                for (k, (path, size)) in m.list.iter().enumerate() {
                    let ry = lr.top() + 6.0 + k as f32 * 19.0;
                    if ry > lr.bottom() {
                        break;
                    }
                    let s = fmt_bytes(*size);
                    let sw = text_w(ui, &s, monob(11.5));
                    lp.text(pos2(lr.right() - 10.0, ry + 9.0), Align2::RIGHT_CENTER, &s, monob(11.5), TEXT);
                    text_left_elide(ui, &lp, Rect::from_min_size(pos2(lr.left() + 10.0, ry), vec2(iw - 30.0 - sw, 18.0)), path, mono(11.5), TEXT);
                }
                y += list_h;
            }
            let _ = y;
            let ok = BtnOpts { label: &m.ok, icon: None, kind: if m.danger { Btn::Danger } else { Btn::Success }, small: false, enabled: true };
            let cancel = BtnOpts { label: "Cancel", icon: None, kind: Btn::Ghost, small: false, enabled: true };
            let ok_sz = btn_size(ui, &ok);
            let c_sz = btn_size(ui, &cancel);
            let by = card.bottom() - 26.0 - 32.0;
            let okr = Rect::from_min_size(pos2(card.right() - 26.0 - ok_sz.x, by), ok_sz);
            let cr = Rect::from_min_size(pos2(okr.left() - 8.0 - c_sz.x, by), c_sz);
            if btn_at(ui, cr, Id::new("m-cancel"), &cancel).clicked() {
                result = Some(false);
            }
            if btn_at(ui, okr, Id::new("m-ok"), &ok).clicked() {
                result = Some(true);
            }
        });
        match result {
            Some(true) => {
                if let Some(m) = self.modal.take() {
                    self.run_modal_action(m.action);
                }
            }
            Some(false) => self.modal = None,
            None => {}
        }
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        if self.toasts.is_empty() {
            return;
        }
        let screen = ctx.screen_rect();
        let mut y = screen.bottom() - 40.0;
        for (i, t) in self.toasts.iter().enumerate().rev() {
            let w = 380.0;
            let msg_g = egui::WidgetText::from(egui::RichText::new(&t.msg).font(f(12.0)).color(MUTED));
            let _ = msg_g;
            let age = t.born.elapsed().as_secs_f32();
            let life = if t.err { 7.0 } else { 4.2 };
            let alpha = (age / 0.2).min(1.0) * ((life - age) / 0.25).clamp(0.0, 1.0);
            let pos = pos2(screen.right() - 16.0 - w, y);
            let area = egui::Area::new(Id::new(("toast", i))).order(Order::Foreground).pivot(Align2::LEFT_BOTTOM).fixed_pos(pos).interactable(false).show(ctx, |ui| {
                ui.set_opacity(alpha);
                egui::Frame::new()
                    .fill(rgba(0x182030, 0.98))
                    .stroke(Stroke::new(1.0_f32, line2()))
                    .corner_radius(14)
                    .inner_margin(egui::Margin::symmetric(14, 12))
                    .shadow(egui::epaint::Shadow { offset: [0, 16], blur: 40, spread: 0, color: Color32::from_black_alpha(200) })
                    .show(ui, |ui| {
                        ui.set_width(w - 28.0);
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing = vec2(11.0, 2.0);
                            let (dr, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                            let (bg, fg, ch) = if t.err { (rgba(0xf43f5e, 0.18), hex(0xfb7185), "!") } else { (rgba(0x10b981, 0.18), MINT, "✓") };
                            ui.painter().circle_filled(dr.center(), 11.0, bg);
                            ui.painter().text(dr.center(), Align2::CENTER_CENTER, ch, b(12.0), fg);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.label(egui::RichText::new(&t.title).font(sb(12.5)).color(TEXT));
                                if !t.msg.is_empty() {
                                    ui.add(egui::Label::new(egui::RichText::new(&t.msg).font(f(12.0)).color(MUTED)).wrap());
                                }
                            });
                        });
                    });
            });
            y -= area.response.rect.height() + 8.0;
        }
    }
}

fn rrect_outline(rect: Rect, r: f32) -> Vec<Pos2> {
    let mut pts = Vec::new();
    let seg = 8;
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -90.0f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ];
    for (c, start) in corners {
        for k in 0..=seg {
            let a = (start + 90.0 * k as f32 / seg as f32).to_radians();
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    pts
}
