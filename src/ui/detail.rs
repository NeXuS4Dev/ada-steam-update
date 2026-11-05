use egui::{CornerRadius, CursorIcon, Frame, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui, vec2};
use egui_phosphor::regular as ph;

use super::library::{HEADER_ASPECT, placeholder};
use super::theme::{self, Palette};
use super::widgets::{self, Btn};
use super::{Action, AdaApp, notes};
use crate::model::{AppId, UpdateKind, UpdateRecord};
use crate::worker::{ImageKey, Shared};

impl AdaApp {
    pub(super) fn detail(&mut self, ui: &mut Ui, shared: &Shared, id: AppId, actions: &mut Vec<Action>) {
        let p = Palette::of(ui.ctx());
        let lib = &shared.library;
        let Some(g) = lib.game(id) else { return };
        let checking = shared.status.checking == Some(id);
        let updates: Vec<&UpdateRecord> = lib.updates_for(id).collect();

        // Viewing a game marks its updates as seen.
        if updates.iter().any(|u| !u.seen) {
            actions.push(Action::MarkSeen(Some(id)));
        }

        if Btn::ghost("Library").icon(ph::ARROW_LEFT).small().show(ui).on_hover_text("Back (Esc)").clicked() {
            actions.push(Action::Back);
        }
        ui.add_space(12.0);

        // ---------------------------------------------------------------- hero
        widgets::card(p).inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let wide = ui.available_width() > 640.0;
            let img_w: f32 = if wide { 300.0 } else { ui.available_width() };
            let draw_image = |this: &mut Self, ui: &mut Ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(img_w, (img_w * HEADER_ASPECT).round()), Sense::hover());
                let r = CornerRadius::same(theme::RADIUS_SM + 2);
                match this.texture(ImageKey::Header(id)) {
                    Some(t) => {
                        egui::Image::new(&t).corner_radius(r).paint_at(ui, rect);
                    }
                    None => placeholder(ui, rect, r, &g.name, p),
                }
            };
            let info = |ui: &mut Ui, actions: &mut Vec<Action>| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.add(egui::Label::new(RichText::new(&g.name).font(theme::semibold(24.0)).color(p.text)).wrap());
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let id_resp = ui
                            .add(
                                egui::Label::new(
                                    RichText::new(format!("App {}", g.app_id))
                                        .font(theme::mono(13.0))
                                        .color(p.text_muted),
                                )
                                .sense(Sense::click()),
                            )
                            .on_hover_text("Click to copy the App ID")
                            .on_hover_cursor(CursorIcon::PointingHand);
                        if id_resp.clicked() {
                            actions.push(Action::Copy(g.app_id.to_string()));
                        }
                        if let Some(s) = &g.snapshot {
                            ui.label(RichText::new("·").color(p.text_faint));
                            ui.label(
                                RichText::new(format!("Build {}", s.build_id))
                                    .font(theme::mono(13.0))
                                    .color(p.text_muted),
                            );
                        }
                        ui.label(RichText::new("·").color(p.text_faint));
                        let checked = if checking {
                            "Checking now…".to_string()
                        } else {
                            g.last_checked
                                .map(|t| format!("Checked {}", widgets::ago(t)))
                                .unwrap_or_else(|| "Not checked yet".into())
                        };
                        ui.label(RichText::new(checked).size(13.0).color(p.text_muted));
                    });
                    if let Some(e) = &g.last_error {
                        ui.label(
                            RichText::new(format!("{}  Last check failed: {e}", ph::WARNING))
                                .size(13.0)
                                .color(p.warning),
                        );
                    }
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if Btn::primary(if checking { "Checking…" } else { "Check now" })
                            .icon(ph::ARROWS_CLOCKWISE)
                            .enabled(!checking)
                            .small()
                            .show(ui)
                            .clicked()
                        {
                            actions.push(Action::Check(id));
                        }
                        if Btn::new("Steam Store").icon(ph::ARROW_SQUARE_OUT).small().show(ui).clicked() {
                            actions.push(Action::OpenUrl(g.store_url()));
                        }
                        if Btn::new("SteamDB").icon(ph::ARROW_SQUARE_OUT).small().show(ui).clicked() {
                            actions.push(Action::OpenUrl(g.steamdb_url()));
                        }
                        let (icon, tip) = if g.muted {
                            (ph::BELL_SLASH, "Notifications are off for this game. Click to turn them on.")
                        } else {
                            (ph::BELL, "Notifications are on for this game. Click to mute.")
                        };
                        if Btn::new("").icon(icon).small().show(ui).on_hover_text(tip).clicked() {
                            actions.push(Action::ToggleMute(id));
                        }
                        if Btn::new("").icon(ph::TRASH).small().show(ui).on_hover_text("Stop tracking").clicked() {
                            actions.push(Action::AskRemove(id));
                        }
                    });
                });
            };
            if wide {
                ui.horizontal_top(|ui| {
                    draw_image(self, ui);
                    ui.add_space(12.0);
                    info(ui, actions);
                });
            } else {
                draw_image(self, ui);
                ui.add_space(8.0);
                info(ui, actions);
            }
        });
        ui.add_space(24.0);

        // ------------------------------------------------------ selected update
        let selected = self
            .selected_update
            .and_then(|b| updates.iter().find(|u| u.to_build == b).copied())
            .or_else(|| updates.first().copied());

        match selected {
            Some(u) => {
                let is_latest = updates.first().is_some_and(|f| f.to_build == u.to_build);
                self.update_card(ui, u, is_latest, actions);
            }
            None => {
                widgets::card(p).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(16.0).color(p.accent));
                        widgets::muted(ui, "Fetching build information from Steam…");
                    });
                });
            }
        }

        // ------------------------------------------------------------- history
        if updates.len() > 1 || updates.first().is_some_and(|u| u.from_build.is_some()) {
            ui.add_space(28.0);
            widgets::section_title(ui, "Update history", Some("Every build Ada has seen for this game."));
            ui.add_space(10.0);
            let current = selected.map(|u| u.to_build);
            Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(theme::RADIUS))
                .inner_margin(Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for u in &updates {
                        if history_row(ui, u, current == Some(u.to_build)).clicked() {
                            actions.push(Action::SelectUpdate(u.to_build));
                        }
                    }
                });
        }
    }

    fn update_card(&mut self, ui: &mut Ui, u: &UpdateRecord, is_latest: bool, actions: &mut Vec<Action>) {
        let p = Palette::of(ui.ctx());
        let accent = p.kind(u.kind);
        let resp = widgets::card(p).inner_margin(Margin::same(24)).show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal(|ui| {
                widgets::kind_badge(ui, u.kind);
                if is_latest {
                    widgets::pill(ui, None, if u.from_build.is_some() { "Latest" } else { "Current build" }, p.text_muted);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(widgets::ago(u.released_at)).size(13.0).color(p.text_muted))
                        .on_hover_text(widgets::date_time(u.released_at));
                });
            });
            ui.add_space(10.0);
            ui.add(egui::Label::new(RichText::new(u.title()).font(theme::semibold(20.0)).color(p.text)).wrap());
            ui.add_space(2.0);
            widgets::muted(ui, u.kind.description());
            ui.add_space(18.0);

            // Stats
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 36.0;
                ui.spacing_mut().item_spacing.y = 14.0;
                widgets::stat(ui, "Released", &widgets::date_time(u.released_at));
                let build = match u.from_build {
                    Some(f) => format!("{f} → {}", u.to_build),
                    None => u.to_build.to_string(),
                };
                widgets::stat(ui, "Build", &build);
                if u.from_build.is_some() {
                    widgets::stat(ui, "Platforms", &widgets::platforms_text(&u.platforms));
                    let depots = match u.changed_depots {
                        0 => "None (metadata only)".to_string(),
                        1 => format!("1 depot · up to {}", widgets::bytes(u.changed_download)),
                        n => format!("{n} depots · up to {}", widgets::bytes(u.changed_download)),
                    };
                    widgets::stat(ui, "Changed content", &depots);
                    widgets::stat(ui, "Install size change", &widgets::signed_bytes(u.download_delta));
                }
            });

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(14.0);

            match &u.announcement {
                Some(a) => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}  Patch notes", ph::FILE_TEXT)).font(theme::semibold(15.0)).color(p.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if Btn::ghost("Read on Steam").icon(ph::ARROW_SQUARE_OUT).small().show(ui).clicked() {
                                actions.push(Action::OpenUrl(a.url(u.app_id)));
                            }
                        });
                    });
                    ui.add_space(8.0);
                    let blocks = self.notes_cache.get(&a.gid, &a.body).to_vec();
                    notes::render(ui, &blocks, actions);
                }
                None => {
                    Frame::new()
                        .fill(p.tint(p.silent))
                        .corner_radius(CornerRadius::same(theme::RADIUS_SM + 2))
                        .inner_margin(Margin::symmetric(14, 12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_top(|ui| {
                                ui.label(RichText::new(ph::INFO).size(18.0).color(p.silent));
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    ui.label(RichText::new("No patch notes for this build").font(theme::semibold(14.0)).color(p.text));
                                    let msg = if u.kind == UpdateKind::Silent && crate::model::now() - u.released_at < crate::detect::LATE_NOTES_WINDOW_SECS {
                                        "The developer hasn't published an announcement for it yet. Ada keeps looking for a week and attaches it automatically."
                                    } else {
                                        "The developer didn't publish an announcement for this build. Check SteamDB for a file-level breakdown."
                                    };
                                    ui.add(egui::Label::new(RichText::new(msg).size(13.0).color(p.text_muted)).wrap());
                                });
                            });
                        });
                }
            }
        });

        // Accent stripe on the card's left edge, matching the update kind.
        let r = resp.response.rect;
        let stripe = Rect::from_min_size(r.min + vec2(0.0, 18.0), vec2(3.0, 28.0));
        ui.painter().rect_filled(stripe, CornerRadius { nw: 0, sw: 0, ne: 2, se: 2 }, accent);
    }
}

fn history_row(ui: &mut Ui, u: &UpdateRecord, selected: bool) -> egui::Response {
    let p = Palette::of(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
    let hovered = resp.hovered();
    if selected || hovered {
        ui.painter().rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM + 2),
            if selected { p.surface_hover } else { p.tint(p.text_faint) },
            Stroke::NONE,
            StrokeKind::Inside,
        );
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(12.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let col = p.kind(u.kind);
    child.add_space(2.0);
    let (dot_rect, _) = child.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    child.painter().circle_filled(dot_rect.center(), 4.0, col);
    child.add_space(4.0);
    child.allocate_ui_with_layout(vec2(96.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.label(RichText::new(u.kind.short_label()).font(theme::semibold(13.0)).color(col));
    });
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(RichText::new(widgets::date_time(u.released_at)).size(13.0).color(p.text_muted));
        ui.add_space(12.0);
        ui.label(RichText::new(format!("#{}", u.to_build)).font(theme::mono(12.5)).color(p.text_faint));
        ui.add_space(12.0);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            if !u.seen {
                widgets::dot(ui, p.accent, 3.5);
            }
            ui.add(egui::Label::new(RichText::new(u.title()).size(14.0).color(p.text)).truncate());
        });
    });
    resp.on_hover_cursor(CursorIcon::PointingHand)
}
