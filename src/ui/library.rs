use egui::{
    Align2, Color32, CornerRadius, CursorIcon, Frame, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use egui_phosphor::regular as ph;

use super::theme::{self, Palette};
use super::widgets::{self, Btn};
use super::{Action, AdaApp};
use crate::model::{AppId, Game, UpdateRecord};
use crate::worker::{ImageKey, Shared};

/// Steam header capsules are 460×215.
pub const HEADER_ASPECT: f32 = 215.0 / 460.0;
const CARD_MIN_WIDTH: f32 = 250.0;
const GAP: f32 = 16.0;

/// Popular games offered on the empty state.
const SUGGESTIONS: [(AppId, &str); 5] = [
    (730, "Counter-Strike 2"),
    (570, "Dota 2"),
    (1091500, "Cyberpunk 2077"),
    (1172470, "Apex Legends"),
    (252490, "Rust"),
];

impl AdaApp {
    pub(super) fn library(&mut self, ui: &mut Ui, shared: &Shared, actions: &mut Vec<Action>) {
        let p = Palette::of(ui.ctx());
        let lib = &shared.library;
        let subtitle = match lib.games.len() {
            0 => "Track Steam games and get notified the moment they update.".to_string(),
            1 => "1 game tracked".to_string(),
            n => format!("{n} games tracked"),
        };

        let has_games = !lib.games.is_empty();
        Self::page_header(ui, "Library", &subtitle, |ui| {
            if !has_games {
                return; // the empty state carries the call to action
            }
            if Btn::primary("Add game").icon(ph::PLUS).show(ui).on_hover_text("Add a game (Ctrl+N)").clicked() {
                actions.push(Action::ShowAdd(None));
            }
            search_field(ui, &mut self.filter);
        });
        ui.add_space(20.0);

        if lib.games.is_empty() {
            self.library_empty(ui, actions);
            return;
        }

        // "Updated since you were last here" — the first thing the user sees on launch.
        let unseen: Vec<&UpdateRecord> = lib.unseen().collect();
        if !unseen.is_empty() {
            self.unseen_banner(ui, shared, &unseen, actions);
            ui.add_space(20.0);
        } else if !shared.status.startup_check_done && shared.status.checking.is_some() {
            Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(theme::RADIUS))
                .inner_margin(Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(14.0).color(p.accent));
                        ui.label(
                            RichText::new("Looking for updates released while Ada was closed…").color(p.text_muted),
                        );
                    });
                });
            ui.add_space(20.0);
        }

        let filter = self.filter.trim().to_lowercase();
        let mut games: Vec<&Game> = lib
            .games
            .iter()
            .filter(|g| filter.is_empty() || g.name.to_lowercase().contains(&filter) || g.app_id.to_string() == filter)
            .collect();
        // Most recently updated first; unseen updates float to the top.
        games.sort_by_key(|g| {
            let latest = lib.latest_update(g.app_id);
            let unseen = latest.is_some_and(|u| !u.seen);
            let t = latest.map(|u| u.released_at).or(g.snapshot.as_ref().map(|s| s.time_updated)).unwrap_or(0);
            (std::cmp::Reverse(unseen), std::cmp::Reverse(t))
        });

        if games.is_empty() {
            ui.add_space(40.0);
            widgets::empty_state(
                ui,
                ph::MAGNIFYING_GLASS,
                "No matches",
                &format!("No tracked game matches “{}”.", self.filter.trim()),
            );
            return;
        }

        let avail = ui.available_width();
        let cols = (((avail + GAP) / (CARD_MIN_WIDTH + GAP)).floor() as usize).clamp(1, 6);
        let card_w = ((avail - GAP * (cols as f32 - 1.0)) / cols as f32).floor();

        for row in games.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for g in row {
                    let latest = lib.latest_update(g.app_id);
                    let checking = shared.status.checking == Some(g.app_id);
                    if self.game_card(ui, g, latest, checking, card_w).clicked() {
                        actions.push(Action::OpenGame(g.app_id, None));
                    }
                }
            });
            ui.add_space(GAP - ui.spacing().item_spacing.y);
        }
    }

    fn library_empty(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let p = Palette::of(ui.ctx());
        ui.add_space(48.0);
        widgets::empty_state(
            ui,
            ph::BELL,
            "Never miss a game update",
            "Add a game by name, App ID or Steam link. Ada checks it in the background and tells you what changed.",
        );
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            if Btn::primary("Add your first game").icon(ph::PLUS).show(ui).clicked() {
                actions.push(Action::ShowAdd(None));
            }
            ui.add_space(28.0);
            ui.label(RichText::new("POPULAR").font(theme::semibold(11.0)).color(p.text_faint));
            ui.add_space(4.0);
        });
        // Center a row of suggestion chips.
        let chips: Vec<(AppId, &str)> = SUGGESTIONS.to_vec();
        let widths: Vec<f32> = chips
            .iter()
            .map(|(_, n)| {
                ui.painter().layout_no_wrap(format!("{} {n}", ph::PLUS), theme::regular(13.0), Color32::WHITE).size().x
                    + 24.0
            })
            .collect();
        let total: f32 = widths.iter().sum::<f32>() + 8.0 * (chips.len() as f32 - 1.0);
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
            for ((id, name), w) in chips.into_iter().zip(widths) {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
                let hovered = resp.hovered();
                ui.painter().rect(
                    rect,
                    CornerRadius::same(255),
                    if hovered { p.surface_hover } else { p.surface },
                    Stroke::new(1.0, if hovered { p.border_strong } else { p.border }),
                    StrokeKind::Inside,
                );
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    format!("{} {name}", ph::PLUS),
                    theme::regular(13.0),
                    if hovered { p.text } else { p.text_muted },
                );
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    actions.push(Action::Add(id));
                }
            }
        });
    }

    fn unseen_banner(&mut self, ui: &mut Ui, shared: &Shared, unseen: &[&UpdateRecord], actions: &mut Vec<Action>) {
        let p = Palette::of(ui.ctx());
        let mut games: Vec<AppId> = Vec::new();
        for u in unseen {
            if !games.contains(&u.app_id) {
                games.push(u.app_id);
            }
        }
        let title = match games.len() {
            1 => "1 game updated since you last looked".to_string(),
            n => format!("{n} games updated since you last looked"),
        };
        Frame::new()
            .fill(p.tint(p.accent))
            .stroke(Stroke::new(1.0, p.tint_strong(p.accent)))
            .corner_radius(CornerRadius::same(theme::RADIUS))
            .inner_margin(Margin::symmetric(16, 14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(ph::BELL_RINGING).size(20.0).color(p.accent));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new(title).font(theme::semibold(15.0)).color(p.text));
                        let names: Vec<String> = games
                            .iter()
                            .take(4)
                            .filter_map(|id| shared.library.game(*id).map(|g| g.name.clone()))
                            .collect();
                        let more = games.len().saturating_sub(4);
                        let mut line = names.join(" · ");
                        if more > 0 {
                            line.push_str(&format!(" · +{more} more"));
                        }
                        ui.add(egui::Label::new(RichText::new(line).size(13.0).color(p.text_muted)).truncate());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if Btn::new("Mark all as seen").icon(ph::CHECKS).small().show(ui).clicked() {
                            actions.push(Action::MarkSeen(None));
                        }
                        if Btn::primary("Review").small().show(ui).clicked() {
                            actions.push(Action::Navigate(super::Page::Activity));
                        }
                    });
                });
            });
    }

    fn game_card(
        &mut self,
        ui: &mut Ui,
        g: &Game,
        latest: Option<&UpdateRecord>,
        checking: bool,
        width: f32,
    ) -> egui::Response {
        let p = Palette::of(ui.ctx());
        let img_h = (width * HEADER_ASPECT).round();
        let body_h = 74.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(width, img_h + body_h), Sense::click());
        if !ui.is_rect_visible(rect) {
            return resp;
        }
        let hovered = resp.hovered();
        let t = ui.ctx().animate_bool_with_time(resp.id.with("hover"), hovered, 0.12);
        let r = theme::RADIUS;
        let painter = ui.painter();

        // Lift + shadow on hover.
        let rect_draw = rect.translate(vec2(0.0, -2.0 * t));
        if t > 0.0 {
            let shadow =
                egui::epaint::Shadow { offset: [0, 8], blur: 24, spread: 0, color: p.shadow.gamma_multiply(t) };
            painter.add(shadow.as_shape(rect_draw, CornerRadius::same(r)));
        }
        painter.rect(
            rect_draw,
            CornerRadius::same(r),
            p.surface,
            Stroke::new(1.0, widgets::lerp_color(p.border, p.border_strong, t)),
            StrokeKind::Inside,
        );

        // Header image.
        let img_rect = Rect::from_min_size(rect_draw.min + vec2(1.0, 1.0), vec2(width - 2.0, img_h - 1.0));
        let img_radius = CornerRadius { nw: r - 1, ne: r - 1, sw: 0, se: 0 };
        match self.texture(ImageKey::Header(g.app_id)) {
            Some(tex) => {
                egui::Image::new(&tex).corner_radius(img_radius).paint_at(ui, img_rect);
            }
            None => placeholder(ui, img_rect, img_radius, &g.name, p),
        }
        let painter = ui.painter();

        // Badges over the image.
        let unseen = latest.is_some_and(|u| !u.seen);
        if unseen {
            let label = "NEW";
            let galley = painter.layout_no_wrap(label.into(), theme::semibold(11.0), p.on_accent);
            let pill = Rect::from_min_size(img_rect.min + vec2(10.0, 10.0), galley.size() + vec2(14.0, 8.0));
            painter.rect_filled(pill, CornerRadius::same(255), p.accent);
            painter.galley(pill.center() - galley.size() / 2.0, galley, p.on_accent);
        }
        if checking {
            let c = pos2(img_rect.right() - 20.0, img_rect.top() + 20.0);
            painter.circle_filled(c, 13.0, Color32::from_black_alpha(150));
            egui::Spinner::new()
                .size(14.0)
                .color(Color32::WHITE)
                .paint_at(ui, Rect::from_center_size(c, vec2(14.0, 14.0)));
        } else if g.last_error.is_some() {
            let c = pos2(img_rect.right() - 20.0, img_rect.top() + 20.0);
            painter.circle_filled(c, 13.0, Color32::from_black_alpha(150));
            painter.text(c, Align2::CENTER_CENTER, ph::WARNING, theme::regular(15.0), p.warning);
        }
        let painter = ui.painter();

        // Body.
        let body = Rect::from_min_size(
            pos2(rect_draw.left() + 14.0, img_rect.bottom() + 12.0),
            vec2(width - 28.0, body_h - 24.0),
        );
        let name_galley = painter.layout(g.name.clone(), theme::semibold(15.0), p.text, f32::INFINITY);
        let name_galley = if name_galley.size().x > body.width() {
            let mut job = egui::text::LayoutJob::simple_singleline(g.name.clone(), theme::semibold(15.0), p.text);
            job.wrap = egui::text::TextWrapping::truncate_at_width(body.width());
            painter.layout_job(job)
        } else {
            name_galley
        };
        painter.galley(body.min, name_galley, p.text);

        let meta_y = body.top() + 28.0;
        let mut x = body.left();
        if let Some(u) = latest {
            let col = p.kind(u.kind);
            let label = format!("{}  {}", widgets::kind_icon(u.kind), u.kind.short_label());
            let galley = painter.layout_no_wrap(label, theme::semibold(12.0), col);
            let pill = Rect::from_min_size(pos2(x, meta_y), galley.size() + vec2(14.0, 6.0));
            painter.rect_filled(pill, CornerRadius::same(255), p.tint(col));
            painter.galley(pill.center() - galley.size() / 2.0, galley, col);
            x = pill.right() + 8.0;
            painter.text(
                pos2(x, pill.center().y),
                Align2::LEFT_CENTER,
                widgets::ago(u.released_at),
                theme::regular(13.0),
                p.text_muted,
            );
        } else if g.snapshot.is_none() {
            painter.text(
                pos2(x, meta_y + 10.0),
                Align2::LEFT_CENTER,
                "Waiting for first check…",
                theme::regular(13.0),
                p.text_faint,
            );
        }
        if g.muted {
            painter.text(
                pos2(body.right(), meta_y + 10.0),
                Align2::RIGHT_CENTER,
                ph::BELL_SLASH,
                theme::regular(15.0),
                p.text_faint,
            );
        }

        let mut resp = resp.on_hover_cursor(CursorIcon::PointingHand);
        if let Some(e) = &g.last_error {
            resp = resp.on_hover_text(format!("Last check failed: {e}"));
        }
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &g.name));
        resp
    }
}

/// Branded placeholder while (or if) the header image is unavailable.
pub fn placeholder(ui: &Ui, rect: Rect, radius: CornerRadius, name: &str, p: &Palette) {
    let painter = ui.painter();
    painter.rect_filled(rect, radius, p.surface_hover);
    let initials: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .collect::<String>()
        .to_uppercase();
    let size = (rect.height() * 0.32).clamp(12.0, 40.0);
    painter.text(rect.center(), Align2::CENTER_CENTER, initials, theme::semibold(size), p.text_faint);
}

fn search_field(ui: &mut Ui, text: &mut String) {
    let p = Palette::of(ui.ctx());
    Frame::new()
        .fill(ui.visuals().text_edit_bg_color())
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS_SM + 2))
        .inner_margin(Margin::symmetric(10, 0))
        .show(ui, |ui| {
            ui.set_height(36.0);
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new(ph::MAGNIFYING_GLASS).size(15.0).color(p.text_faint));
                ui.add(
                    egui::TextEdit::singleline(text)
                        .hint_text("Filter games")
                        .frame(egui::Frame::NONE)
                        .desired_width(170.0)
                        .vertical_align(egui::Align::Center),
                );
                if !text.is_empty()
                    && ui.add(egui::Button::new(RichText::new(ph::X).color(p.text_faint)).frame(false)).clicked()
                {
                    text.clear();
                }
            });
        });
}
