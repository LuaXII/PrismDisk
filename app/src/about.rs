use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, Id, Layout, Order, Rect, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder};

use crate::app::{fmt_bytes, App, UpdateState};
use crate::icons;
use crate::theme::{self, *};
use crate::ui::{hchild, hgrad_rrect, prism_bar, rich};
use crate::update;

impl App {
    pub(crate) fn update_banner_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        hgrad_rrect(&p, rect, 0.0, rgba(0x6366f1, 0.16), rgba(0x0ea5e9, 0.05));
        hline(&p, rect.left(), rect.right(), rect.bottom() - 1.0, rgba(0x818cf8, 0.3));
        let inner = rect.shrink2(vec2(16.0, 8.0));
        let state = self.update.clone();
        let (lead, rest) = match &state {
            UpdateState::Downloading(rel, _) => (format!("PrismDisk {} is available. ", rel.version), "Downloading in the background…".to_string()),
            UpdateState::Ready(rel, _) => (format!("PrismDisk {} is ready to install. ", rel.version), "It takes a few seconds and reopens the app.".to_string()),
            UpdateState::Failed(e, Some(rel)) => (format!("Couldn't download PrismDisk {}. ", rel.version), e.clone()),
            _ => return,
        };

        let mut right = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
        right.spacing_mut().item_spacing.x = 8.0;
        let (xr, xresp) = right.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
        if xresp.hovered() {
            right.painter().rect_filled(xr, 8, white(0.08));
        }
        icons::paint(&right, icons::X, Rect::from_center_size(xr.center(), vec2(14.0, 14.0)), if xresp.hovered() { TEXT } else { MUTED });
        if xresp.on_hover_text("Remind me later").clicked() {
            self.update_banner = false;
        }
        match &state {
            UpdateState::Downloading(rel, prog) => {
                let done = prog.load(std::sync::atomic::Ordering::Relaxed).min(rel.size.max(1));
                let frac = done as f32 / rel.size.max(1) as f32;
                let (br, _) = right.allocate_exact_size(vec2(152.0, 28.0), Sense::hover());
                let track = Rect::from_min_size(pos2(br.left(), br.center().y - 3.0), vec2(110.0, 6.0));
                right.painter().rect_filled(track, 9, white(0.08));
                if frac > 0.0 {
                    prism_bar(right.painter(), Rect::from_min_size(track.min, vec2((110.0 * frac).max(6.0), 6.0)), 3.0);
                }
                right.painter().text(pos2(br.right(), br.center().y), Align2::RIGHT_CENTER, format!("{:.0}%", frac * 100.0), mono(11.5), hex(0xc7d2fe));
            }
            UpdateState::Ready(..) => {
                if btn(&mut right, "Restart & update", Some(icons::REFRESH), Btn::Primary, true, true).clicked() {
                    self.apply_update();
                }
            }
            UpdateState::Failed(..) => {
                if btn(&mut right, "Retry", Some(icons::REFRESH), Btn::Accent, true, true).clicked() {
                    self.retry_update();
                }
            }
            _ => {}
        }
        if btn(&mut right, "What's new", None, Btn::Ghost, true, true).clicked() {
            self.about = true;
        }
        let right_edge = right.min_rect().left() - 16.0;

        let mut c = hchild(ui, Rect::from_min_max(inner.min, pos2(right_edge, inner.bottom())));
        c.spacing_mut().item_spacing.x = 12.0;
        let (ir, _) = c.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        icons::paint(&c, icons::SPARKLE, ir, INDIGO2);
        let avail = (right_edge - ir.right() - 12.0).max(80.0);
        let g = rich(&c, &[(&lead, sb(12.0), hex(0xc7d2fe)), (&rest, f(12.0), hex(0xa5b4fc))], 10_000.0);
        let mut job = (*g.job).clone();
        job.wrap = egui::text::TextWrapping::truncate_at_width(avail);
        let g = c.fonts(|fo| fo.layout_job(job));
        c.painter().galley(pos2(ir.right() + 12.0, inner.center().y - g.size().y / 2.0), g, TEXT);
    }

    pub(crate) fn about_ui(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let w = 580.0_f32.min(screen.width() - 40.0);
        let h = 640.0_f32.min(screen.height() - 40.0);
        let card = Rect::from_center_size(screen.center(), vec2(w, h));
        let state = self.update.clone();
        let mut close = false;
        let mut action: Option<&'static str> = None;
        egui::Area::new(Id::new("about")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            let bg = ui.interact(screen, Id::new("about-block"), Sense::click());
            if bg.clicked() && !ui.ctx().pointer_interact_pos().map(|q| card.contains(q)).unwrap_or(false) {
                close = true;
            }
            let p = ui.painter().clone();
            p.rect_filled(screen, 0, rgba(0x06080c, 0.66));
            shadow(&p, card, 20, 80, 40, Color32::from_black_alpha(215));
            theme::grad_rrect(&p, card, 20.0, |pt| lerp(SURFACE2, SURFACE, (pt.y - card.top()) / card.height()));
            p.rect_stroke(card, 20, Stroke::new(1.0_f32, line2()), StrokeKind::Inside);
            ui.interact(card, Id::new("about-card"), Sense::click());

            let inner = card.shrink(24.0);
            let mark = Rect::from_min_size(inner.min, vec2(48.0, 48.0));
            shadow(&p, mark.shrink(4.0), 14, 24, 8, rgba(0x6366f1, 0.8));
            prism_rrect(&p, mark, 14.0);
            icons::paint(ui, icons::LOGO, Rect::from_center_size(mark.center(), vec2(27.0, 27.0)), Color32::WHITE);
            p.text(pos2(mark.right() + 14.0, mark.center().y + 1.0), Align2::LEFT_BOTTOM, "PrismDisk", b(20.0), TEXT);
            p.text(pos2(mark.right() + 14.0, mark.center().y + 3.0), Align2::LEFT_TOP, format!("Version {}", update::current_version()), mono(11.5), MUTED);
            let xr = Rect::from_min_size(pos2(inner.right() - 28.0, inner.top()), vec2(28.0, 28.0));
            let xresp = ui.interact(xr, Id::new("about-x"), Sense::click());
            if xresp.hovered() {
                p.rect_filled(xr, 8, white(0.08));
            }
            icons::paint(ui, icons::X, Rect::from_center_size(xr.center(), vec2(14.0, 14.0)), if xresp.hovered() { TEXT } else { MUTED });
            if xresp.clicked() {
                close = true;
            }

            let status = Rect::from_min_size(pos2(inner.left(), mark.bottom() + 18.0), vec2(inner.width(), 56.0));
            let (fill, stroke) = match &state {
                UpdateState::Ready(..) | UpdateState::Downloading(..) => (rgba(0x6366f1, 0.12), rgba(0x818cf8, 0.35)),
                UpdateState::Failed(..) => (rgba(0xf43f5e, 0.08), rgba(0xf43f5e, 0.3)),
                UpdateState::UpToDate => (rgba(0x10b981, 0.08), rgba(0x10b981, 0.28)),
                _ => (white(0.03), line()),
            };
            p.rect_filled(status, 12, fill);
            p.rect_stroke(status, 12, Stroke::new(1.0_f32, stroke), StrokeKind::Inside);
            let ic = pos2(status.left() + 26.0, status.center().y);
            let t = ui.input(|i| i.time) as f32;
            let (title, sub): (String, String) = match &state {
                UpdateState::Idle => ("Updates".into(), "PrismDisk checks GitHub for new versions when it starts.".into()),
                UpdateState::Checking => {
                    spinner_ring(&p, ic, 11.0, 8.0, t * std::f32::consts::TAU);
                    ui.ctx().request_repaint();
                    ("Checking for updates…".into(), "Asking GitHub for the latest release.".into())
                }
                UpdateState::UpToDate => {
                    p.circle_filled(ic, 11.0, EMERALD);
                    p.add(Shape::line(vec![ic + vec2(-4.5, 0.2), ic + vec2(-1.3, 3.2), ic + vec2(4.6, -3.2)], Stroke::new(2.0_f32, hex(0x04130d))));
                    ("You're up to date".into(), format!("PrismDisk {} is the latest version.", update::current_version()))
                }
                UpdateState::Downloading(rel, prog) => {
                    let done = prog.load(std::sync::atomic::Ordering::Relaxed).min(rel.size.max(1));
                    spinner_ring(&p, ic, 11.0, 8.0, t * std::f32::consts::TAU);
                    ui.ctx().request_repaint();
                    (format!("Downloading PrismDisk {}…", rel.version), format!("{} of {}", fmt_bytes(done), fmt_bytes(rel.size)))
                }
                UpdateState::Ready(rel, _) => {
                    icons::paint(ui, icons::SPARKLE, Rect::from_center_size(ic, vec2(20.0, 20.0)), INDIGO2);
                    (format!("PrismDisk {} is ready", rel.version), "Restart to install it. Your settings are kept.".into())
                }
                UpdateState::Failed(e, _) => {
                    p.circle_stroke(ic, 9.0, Stroke::new(2.0_f32, ROSE));
                    p.text(ic, Align2::CENTER_CENTER, "!", b(12.0), ROSE);
                    ("Couldn't update".into(), e.clone())
                }
            };
            let tx = status.left() + 50.0;
            let button = match &state {
                UpdateState::Ready(..) => Some(("Restart & update", Btn::Primary, "install")),
                UpdateState::Failed(..) => Some(("Retry", Btn::Accent, "retry")),
                UpdateState::Idle | UpdateState::UpToDate => Some(("Check for updates", Btn::Accent, "check")),
                _ => None,
            };
            let mut text_right = status.right() - 14.0;
            if let Some((label, kind, key)) = button {
                let o = BtnOpts { label, icon: Some(icons::REFRESH), kind, small: true, enabled: true };
                let sz = btn_size(ui, &o);
                let br = Rect::from_min_size(pos2(status.right() - 14.0 - sz.x, status.center().y - sz.y / 2.0), sz);
                if btn_at(ui, br, Id::new(("about-btn", key)), &o).clicked() {
                    action = Some(key);
                }
                text_right = br.left() - 12.0;
            }
            text(ui, &p, pos2(tx, status.center().y - 1.0), Align2::LEFT_BOTTOM, &title, sb(13.0), TEXT, text_right - tx);
            text(ui, &p, pos2(tx, status.center().y + 2.0), Align2::LEFT_TOP, &sub, f(11.5), MUTED, text_right - tx);

            let links_y = inner.bottom() - 28.0;
            let notes = Rect::from_min_max(pos2(inner.left(), status.bottom() + 16.0), pos2(inner.right(), links_y - 14.0));
            p.rect_filled(notes, 12, BG);
            p.rect_stroke(notes, 12, Stroke::new(1.0_f32, line()), StrokeKind::Inside);
            let mut nc = ui.new_child(UiBuilder::new().max_rect(notes.shrink2(vec2(18.0, 14.0))).layout(Layout::top_down(Align::Min)));
            let upcoming = match &state {
                UpdateState::Ready(rel, _) | UpdateState::Downloading(rel, _) | UpdateState::Failed(_, Some(rel)) => Some(rel.clone()),
                _ => None,
            };
            egui::ScrollArea::vertical().id_salt("about-notes").auto_shrink([false, false]).show(&mut nc, |ui| {
                ui.set_width(ui.available_width() - 8.0);
                if let Some(rel) = &upcoming {
                    let body: Vec<&str> = rel.notes.lines().filter(|l| !l.starts_with("# ") && !l.starts_with("## ")).collect();
                    let text = format!("## New in {}\n{}", rel.version, body.join("\n"));
                    crate::md::render(ui, &text, true);
                    ui.add_space(16.0);
                    let y = ui.cursor().top();
                    hline(ui.painter(), ui.min_rect().left(), ui.min_rect().right(), y, line());
                    ui.add_space(14.0);
                }
                crate::md::render(ui, update::CHANGELOG, true);
            });

            let mut x = inner.left();
            for (label, icon, key) in [("Website", icons::GLOBE, "site"), ("GitHub", icons::OPEN, "github"), ("All releases", icons::DOWNLOAD, "releases")] {
                let o = BtnOpts { label, icon: Some(icon), kind: Btn::Ghost, small: true, enabled: true };
                let sz = btn_size(ui, &o);
                let br = Rect::from_min_size(pos2(x, links_y), sz);
                if btn_at(ui, br, Id::new(("about-link", key)), &o).clicked() {
                    action = Some(key);
                }
                x = br.right() + 6.0;
            }
            p.text(pos2(inner.right(), links_y + 14.0), Align2::RIGHT_CENTER, "MIT License · © 2026 PrismDisk", f(11.0), DIM);
        });
        if close {
            self.about = false;
        }
        match action {
            Some("install") => self.apply_update(),
            Some("retry") => self.retry_update(),
            Some("check") => self.check_updates(true),
            Some("site") => self.open_url(update::WEBSITE),
            Some("github") => self.open_url(&format!("https://github.com/{}", update::REPO)),
            Some("releases") => self.open_url(&format!("https://github.com/{}/releases", update::REPO)),
            _ => {}
        }
    }
}
