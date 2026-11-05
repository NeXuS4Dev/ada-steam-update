//! Native rendering of Steam patch notes.

use std::collections::HashMap;

use egui::text::{LayoutJob, TextFormat};
use egui::{RichText, Stroke, Ui};

use super::Action;
use super::theme::{self, Palette};
use crate::bbcode::{self, Block, Span};

#[derive(Default)]
pub struct NotesCache {
    parsed: HashMap<String, Vec<Block>>,
}

impl NotesCache {
    pub fn get(&mut self, key: &str, body: &str) -> &[Block] {
        self.parsed.entry(key.to_owned()).or_insert_with(|| bbcode::parse(body))
    }
}

fn job(spans: &[Span], size: f32, color: egui::Color32, p: &Palette) -> LayoutJob {
    let mut job = LayoutJob::default();
    for s in spans {
        let font = if s.bold { theme::semibold(size) } else { theme::regular(size) };
        let mut fmt = TextFormat::simple(font, if s.link.is_some() { p.accent } else { color });
        fmt.italics = s.italic;
        if s.strike {
            fmt.strikethrough = Stroke::new(1.0, color);
        }
        fmt.line_height = Some(size * 1.5);
        job.append(&s.text, 0.0, fmt);
    }
    job
}

fn spans_text(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

/// Render one block; links become clickable.
fn inline(ui: &mut Ui, spans: &[Span], size: f32, color: egui::Color32, actions: &mut Vec<Action>) {
    let p = Palette::of(ui.ctx());
    if spans.iter().all(|s| s.link.is_none()) {
        ui.add(egui::Label::new(job(spans, size, color, p)).wrap());
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for s in spans {
            let single = std::slice::from_ref(s);
            match &s.link {
                Some(url) if !url.is_empty() => {
                    let resp = ui
                        .add(egui::Label::new(job(single, size, color, p)).sense(egui::Sense::click()))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(url);
                    if resp.clicked() {
                        actions.push(Action::OpenUrl(url.clone()));
                    }
                }
                _ => {
                    ui.add(egui::Label::new(job(single, size, color, p)).wrap());
                }
            }
        }
    });
}

pub fn render(ui: &mut Ui, blocks: &[Block], actions: &mut Vec<Action>) {
    let p = Palette::of(ui.ctx());
    if blocks.is_empty() {
        ui.label(RichText::new("This announcement has no text.").color(p.text_faint));
        return;
    }
    ui.spacing_mut().item_spacing.y = 6.0;
    let mut prev_was_bullet = false;
    for (i, block) in blocks.iter().enumerate() {
        match block {
            Block::Heading(level, spans) => {
                if i > 0 {
                    ui.add_space(10.0);
                }
                let size = match level {
                    1 => 19.0,
                    2 => 17.0,
                    _ => 15.0,
                };
                ui.label(RichText::new(spans_text(spans)).font(theme::semibold(size)).color(p.text));
                prev_was_bullet = false;
            }
            Block::Paragraph(spans) => {
                let text = spans_text(spans);
                let t = text.trim();
                // Valve-style section markers: "[ GAMEPLAY ]".
                if t.starts_with('[') && t.ends_with(']') && t.len() < 48 {
                    if i > 0 {
                        ui.add_space(10.0);
                    }
                    let label = t.trim_matches(['[', ']', ' ']).to_uppercase();
                    ui.label(RichText::new(label).font(theme::semibold(12.0)).color(p.accent));
                } else {
                    if prev_was_bullet {
                        ui.add_space(4.0);
                    }
                    inline(ui, spans, 14.0, p.text, actions);
                }
                prev_was_bullet = false;
            }
            Block::Bullet(depth, spans) => {
                ui.horizontal_top(|ui| {
                    ui.add_space(4.0 + 18.0 * (*depth as f32 - 1.0));
                    let bullet = if *depth > 1 { "◦" } else { "•" };
                    ui.label(RichText::new(bullet).size(14.0).color(p.text_faint));
                    ui.add_space(2.0);
                    ui.vertical(|ui| inline(ui, spans, 14.0, p.text, actions));
                });
                prev_was_bullet = true;
            }
            Block::Rule => {
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                prev_was_bullet = false;
            }
        }
    }
}
