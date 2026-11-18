use eframe::egui::{self, pos2, vec2, Color32, FontId, Sense, Ui};
use egui::text::{LayoutJob, TextFormat, TextWrapping};

use crate::theme::*;

fn inline(job: &mut LayoutJob, s: &str, base: FontId, color: Color32) {
    let mut bold = false;
    let mut code = false;
    let mut buf = String::new();
    let flush = |job: &mut LayoutJob, buf: &mut String, bold: bool, code: bool| {
        if buf.is_empty() {
            return;
        }
        let (font, col) = if code {
            (mono(base.size - 1.0), hex(0xc7d2fe))
        } else if bold {
            (sb(base.size), TEXT)
        } else {
            (base.clone(), color)
        };
        job.append(buf, 0.0, TextFormat::simple(font, col));
        buf.clear();
    };
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            chars.next();
            flush(job, &mut buf, bold, code);
            bold = !bold;
        } else if c == '`' {
            flush(job, &mut buf, bold, code);
            code = !code;
        } else {
            buf.push(c);
        }
    }
    flush(job, &mut buf, bold, code);
}

fn paragraph(ui: &mut Ui, s: &str, font: FontId, color: Color32, indent: f32, bullet: bool) {
    let width = ui.available_width() - indent;
    let mut job = LayoutJob::default();
    inline(&mut job, s, font.clone(), color);
    job.wrap = TextWrapping { max_width: width, ..Default::default() };
    for sec in job.sections.iter_mut() {
        sec.format.line_height = Some(font.size * 1.45);
    }
    let g = ui.fonts(|f| f.layout_job(job));
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), g.size().y), Sense::hover());
    if bullet {
        ui.painter().circle_filled(pos2(rect.left() + 5.0, rect.top() + font.size * 0.75), 2.2, INDIGO2);
    }
    ui.painter().galley(pos2(rect.left() + indent, rect.top()), g, color);
}

pub fn render(ui: &mut Ui, text: &str, skip_title: bool) {
    ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
    let mut first_heading = true;
    for raw in text.replace("\r\n", "\n").lines() {
        let line = raw.trim_end();
        if let Some(h) = line.strip_prefix("# ") {
            if !skip_title {
                paragraph(ui, h, b(18.0), TEXT, 0.0, false);
            }
        } else if let Some(h) = line.strip_prefix("## ") {
            if !first_heading {
                ui.add_space(14.0);
            }
            first_heading = false;
            paragraph(ui, h, b(15.0), TEXT, 0.0, false);
        } else if let Some(h) = line.strip_prefix("### ") {
            ui.add_space(6.0);
            let mut fmt = TextFormat::simple(sb(10.5), DIM);
            fmt.extra_letter_spacing = 0.8;
            let g = ui.fonts(|f| f.layout_job(LayoutJob::single_section(h.to_uppercase(), fmt)));
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), g.size().y), Sense::hover());
            ui.painter().galley(rect.min, g, DIM);
        } else if let Some(item) = line.trim_start().strip_prefix("- ").or_else(|| line.trim_start().strip_prefix("* ")) {
            paragraph(ui, item, f(12.5), MUTED, 16.0, true);
        } else if line.trim().is_empty() {
            ui.add_space(2.0);
        } else {
            paragraph(ui, line.trim(), f(12.5), MUTED, 0.0, false);
        }
    }
}
