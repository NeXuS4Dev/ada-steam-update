use std::collections::HashSet;
use std::time::{Duration, Instant};

use egui::{Color32, CornerRadius, CursorIcon, Frame, Margin, RichText, Sense, Stroke, StrokeKind, Ui, vec2};
use egui_phosphor::regular as ph;

use super::library::placeholder;
use super::theme::{self, Palette};
use super::widgets::{self, Btn};
use super::{Action, AdaApp};
use crate::model::AppId;
use crate::steam::{self, SearchHit};
use crate::worker::{Command, ImageKey, Shared};

const DEBOUNCE: Duration = Duration::from_millis(350);

#[derive(Default)]
pub struct AddDialog {
    pub query: String,
    pub added: HashSet<AppId>,
    last_edit: Option<Instant>,
    sent_query: String,
    request: u64,
    pending: Option<u64>,
    results: Option<Result<Vec<SearchHit>, String>>,
    focused_once: bool,
}

impl AddDialog {
    pub fn on_results(&mut self, request: u64, r: Result<Vec<SearchHit>, String>) {
        if Some(request) == self.pending {
            self.pending = None;
            self.results = Some(r);
        }
    }
}

impl AdaApp {
    pub(super) fn add_dialog(&mut self, ctx: &egui::Context, shared: &Shared, actions: &mut Vec<Action>) {
        let p = Palette::of(ctx);
        let Some(mut d) = self.add.take() else { return };
        let mut close = false;

        // Debounced search.
        let term = d.query.trim().to_owned();
        let direct = steam::parse_app_id(&term);
        if term != d.sent_query {
            if d.last_edit.is_none() {
                d.last_edit = Some(Instant::now());
            }
            let elapsed = d.last_edit.map(|t| t.elapsed()).unwrap_or_default();
            if term.chars().count() < 2 || direct.is_some() {
                d.sent_query = term.clone();
                d.pending = None;
                d.results = None;
                d.last_edit = None;
            } else if elapsed >= DEBOUNCE {
                d.request += 1;
                d.pending = Some(d.request);
                d.sent_query = term.clone();
                d.last_edit = None;
                self.worker.send(Command::Search { request: d.request, term: term.clone() });
            } else {
                ctx.request_repaint_after(DEBOUNCE - elapsed);
            }
        }

        let modal = egui::Modal::new(egui::Id::new("add-game"))
            .backdrop_color(Color32::from_black_alpha(if p.dark { 150 } else { 90 }))
            .frame(
                Frame::new()
                    .fill(p.elevated)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(theme::RADIUS_LG))
                    .inner_margin(Margin::same(24))
                    .shadow(ctx.global_style().visuals.window_shadow),
            )
            .show(ctx, |ui| {
                ui.set_width(540.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new("Add a game").font(theme::semibold(19.0)).color(p.text));
                        widgets::muted(ui, "Search by name, paste a Steam link, or enter an App ID.");
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if widgets::icon_button(ui, ph::X, "Close (Esc)").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(16.0);

                // Search input
                let input = Frame::new()
                    .fill(ui.visuals().text_edit_bg_color())
                    .stroke(Stroke::new(1.0, p.accent))
                    .corner_radius(CornerRadius::same(theme::RADIUS_SM + 2))
                    .inner_margin(Margin::symmetric(12, 0))
                    .show(ui, |ui| {
                        ui.set_height(42.0);
                        ui.horizontal_centered(|ui| {
                            ui.label(RichText::new(ph::MAGNIFYING_GLASS).size(17.0).color(p.text_faint));
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut d.query)
                                    .hint_text("e.g. Cyberpunk 2077, 730, or store.steampowered.com/app/…")
                                    .frame(Frame::NONE)
                                    .font(theme::regular(15.0))
                                    .desired_width(f32::INFINITY),
                            );
                            if !d.focused_once {
                                r.request_focus();
                                d.focused_once = true;
                            }
                            if r.changed() {
                                d.last_edit = Some(Instant::now());
                            }
                            if r.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                && let Some(id) = direct
                            {
                                actions.push(Action::Add(id));
                            }
                        });
                    });
                let _ = input;
                ui.add_space(12.0);

                ui.allocate_ui(vec2(ui.available_width(), 0.0), |ui| {
                    ui.set_min_height(280.0);
                    if let Some(id) = direct {
                        let known = shared.library.game(id).map(|g| g.name.clone());
                        let title = known.clone().unwrap_or_else(|| format!("App ID {id}"));
                        let subtitle = if known.is_some() {
                            "Already in your library".to_string()
                        } else {
                            "Add directly by App ID".to_string()
                        };
                        self.result_row(ui, shared, &d, id, &title, &subtitle, None, actions);
                    } else if d.pending.is_some() || d.last_edit.is_some() && !term.is_empty() {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.add(egui::Spinner::new().size(20.0).color(p.accent));
                            ui.add_space(6.0);
                            widgets::muted(ui, "Searching Steam…");
                        });
                    } else {
                        match &d.results {
                            None => {
                                ui.add_space(30.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new(ph::GAME_CONTROLLER).size(30.0).color(p.text_faint));
                                    ui.add_space(4.0);
                                    widgets::muted(ui, "Start typing to search the Steam store.");
                                });
                            }
                            Some(Err(e)) => {
                                ui.add_space(30.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new(ph::WIFI_SLASH).size(28.0).color(p.warning));
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new("Search is unavailable")
                                            .font(theme::semibold(14.0))
                                            .color(p.text),
                                    );
                                    widgets::muted(ui, e.as_str());
                                });
                            }
                            Some(Ok(hits)) if hits.is_empty() => {
                                ui.add_space(30.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new(ph::MAGNIFYING_GLASS).size(28.0).color(p.text_faint));
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new(format!("No games found for “{}”", d.sent_query))
                                            .font(theme::semibold(14.0))
                                            .color(p.text),
                                    );
                                    widgets::muted(ui, "Try a different name, or paste the game's Steam store link.");
                                });
                            }
                            Some(Ok(hits)) => {
                                let hits = hits.clone();
                                egui::ScrollArea::vertical().max_height(340.0).auto_shrink([false, true]).show(
                                    ui,
                                    |ui| {
                                        for h in hits.iter().take(10) {
                                            let sub = format!("App {}", h.app_id);
                                            self.result_row(
                                                ui,
                                                shared,
                                                &d,
                                                h.app_id,
                                                &h.name,
                                                &sub,
                                                h.thumbnail.as_deref(),
                                                actions,
                                            );
                                        }
                                    },
                                );
                            }
                        }
                    }
                });
            });

        if modal.should_close() || close {
            return; // dialog dropped
        }
        self.add = Some(d);
    }

    #[allow(clippy::too_many_arguments)]
    fn result_row(
        &mut self,
        ui: &mut Ui,
        shared: &Shared,
        d: &AddDialog,
        id: AppId,
        title: &str,
        subtitle: &str,
        thumb: Option<&str>,
        actions: &mut Vec<Action>,
    ) {
        let p = Palette::of(ui.ctx());
        let in_library = shared.library.contains(id) || d.added.contains(&id);
        let adding = self.adding.contains(&id);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
        let hovered = resp.hovered() && !in_library;
        if hovered {
            ui.painter().rect(
                rect,
                CornerRadius::same(theme::RADIUS_SM + 2),
                p.surface_hover,
                Stroke::NONE,
                StrokeKind::Inside,
            );
        }
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(vec2(8.0, 0.0)).with_max_x(rect.right() - 18.0))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let (img_rect, _) = row.allocate_exact_size(vec2(96.0, 36.0), Sense::hover());
        let r = CornerRadius::same(4);
        let tex = match thumb {
            Some(url) => self.texture(ImageKey::Url(url.to_owned())),
            None => self.texture(ImageKey::Header(id)),
        };
        match tex {
            Some(t) => {
                egui::Image::new(&t).corner_radius(r).paint_at(&row, img_rect);
            }
            None => placeholder(&row, img_rect, r, title, p),
        }
        row.add_space(6.0);
        row.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.add(egui::Label::new(RichText::new(title).font(theme::semibold(14.0)).color(p.text)).truncate());
            ui.label(RichText::new(subtitle).size(12.5).color(p.text_faint));
        });
        let mut clicked_add = false;
        row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if in_library && !adding {
                ui.label(
                    RichText::new(format!("{}  Tracking", ph::CHECK_CIRCLE))
                        .font(theme::semibold(13.0))
                        .color(p.success),
                );
            } else if adding {
                ui.label(RichText::new("Adding…").size(13.0).color(p.text_muted));
                ui.add(egui::Spinner::new().size(14.0).color(p.accent));
            } else if Btn::new("Track")
                .icon(ph::PLUS)
                .small()
                .variant(if hovered { widgets::Variant::Primary } else { widgets::Variant::Secondary })
                .show(ui)
                .clicked()
            {
                clicked_add = true;
            }
        });
        if !in_library && !adding && (clicked_add || resp.clicked()) {
            actions.push(Action::Add(id));
        }
        if !in_library && !adding {
            resp.on_hover_cursor(CursorIcon::PointingHand);
        }
    }
}
