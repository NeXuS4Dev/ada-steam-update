use egui::{CornerRadius, CursorIcon, Frame, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui, vec2};
use egui_phosphor::regular as ph;

use super::library::placeholder;
use super::theme::{self, Palette};
use super::widgets::{self, Btn};
use super::{Action, AdaApp};
use crate::bbcode;
use crate::model::UpdateRecord;
use crate::worker::{ImageKey, Shared};

/// Rows shown before "Show more".
const PAGE: usize = 60;

impl AdaApp {
    pub(super) fn activity(&mut self, ui: &mut Ui, shared: &Shared, actions: &mut Vec<Action>) {
        let p = Palette::of(ui.ctx());
        let lib = &shared.library;
        // Baselines (the state at the time a game was added) are not news.
        let items: Vec<&UpdateRecord> = lib.history.iter().filter(|u| u.from_build.is_some()).collect();
        let unseen = lib.unseen_count();

        let subtitle = match unseen {
            0 => "Every update Ada has detected, newest first.".to_string(),
            1 => "1 new update".to_string(),
            n => format!("{n} new updates"),
        };
        Self::page_header(ui, "Activity", &subtitle, |ui| {
            if Btn::new("Mark all as seen").icon(ph::CHECKS).enabled(unseen > 0).show(ui).clicked() {
                actions.push(Action::MarkSeen(None));
            }
        });
        ui.add_space(20.0);

        if items.is_empty() {
            ui.add_space(48.0);
            widgets::empty_state(
                ui,
                ph::PULSE,
                "No updates yet",
                if lib.games.is_empty() {
                    "Add a game to your library and its updates will show up here."
                } else {
                    "Ada is watching your games. New builds will show up here the moment they ship."
                },
            );
            return;
        }

        let show_all = ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new("activity-all")).unwrap_or(false));
        let limit = if show_all { items.len() } else { PAGE.min(items.len()) };
        let mut current_day = String::new();
        for u in &items[..limit] {
            let day = widgets::day_label(u.released_at);
            if day != current_day {
                if !current_day.is_empty() {
                    ui.add_space(14.0);
                }
                ui.label(RichText::new(day.to_uppercase()).font(theme::semibold(11.5)).color(p.text_faint));
                ui.add_space(2.0);
                current_day = day;
            }
            let name = lib.game(u.app_id).map(|g| g.name.as_str()).unwrap_or("Unknown game");
            if self.activity_row(ui, u, name).clicked() {
                actions.push(Action::OpenGame(u.app_id, Some(u.to_build)));
            }
        }
        if limit < items.len() {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                if Btn::new(&format!("Show {} older updates", items.len() - limit)).show(ui).clicked() {
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("activity-all"), true));
                }
            });
        }
    }

    fn activity_row(&mut self, ui: &mut Ui, u: &UpdateRecord, name: &str) -> egui::Response {
        let p = Palette::of(ui.ctx());
        let resp = Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(theme::RADIUS))
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(107.0, 50.0), Sense::hover());
                    let r = CornerRadius::same(theme::RADIUS_SM);
                    match self.texture(ImageKey::Header(u.app_id)) {
                        Some(t) => {
                            egui::Image::new(&t).corner_radius(r).paint_at(ui, rect);
                        }
                        None => placeholder(ui, rect, r, name, p),
                    }
                    ui.add_space(6.0);
                    // Fixed-width text column: the time/unread column on the right never overflows.
                    const RIGHT_COL: f32 = 76.0;
                    let text_w = (ui.available_width() - RIGHT_COL).max(120.0);
                    ui.allocate_ui_with_layout(vec2(text_w, 50.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(text_w);
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).font(theme::semibold(14.5)).color(p.text));
                            widgets::kind_badge(ui, u.kind);
                        });
                        let mut line = u.title();
                        if let Some(a) = &u.announcement {
                            let ex = bbcode::excerpt(&a.body, 160);
                            if !ex.is_empty() && ex != line {
                                line = format!("{line} — {ex}");
                            }
                        } else {
                            line = format!("Build {} · {}", u.to_build, widgets::platforms_text(&u.platforms));
                        }
                        ui.add(egui::Label::new(RichText::new(line).size(13.0).color(p.text_muted)).truncate());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(widgets::time_of_day(u.released_at)).size(13.0).color(p.text_muted))
                            .on_hover_text(widgets::date_time(u.released_at));
                        if !u.seen {
                            widgets::dot(ui, p.accent, 4.0).on_hover_text("New");
                        }
                    });
                });
            })
            .response;
        let resp = resp.interact(Sense::click());
        if resp.hovered() {
            ui.painter().rect_stroke(
                resp.rect,
                CornerRadius::same(theme::RADIUS),
                Stroke::new(1.0, p.border_strong),
                StrokeKind::Inside,
            );
        }
        if !u.seen {
            let stripe = Rect::from_min_size(resp.rect.min + vec2(0.0, 14.0), vec2(3.0, resp.rect.height() - 28.0));
            ui.painter().rect_filled(stripe, CornerRadius { nw: 0, sw: 0, ne: 2, se: 2 }, p.accent);
        }
        ui.add_space(2.0);
        resp.on_hover_cursor(CursorIcon::PointingHand)
    }
}
