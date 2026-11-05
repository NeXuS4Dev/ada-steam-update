use egui::{Align2, CornerRadius, CursorIcon, Frame, Margin, RichText, Sense, Stroke, Ui, pos2, vec2};
use egui_phosphor::regular as ph;

use super::theme::{self, Palette};
use super::widgets::{self, Btn};
use super::{Action, AdaApp, Page};
use crate::worker::Shared;

impl AdaApp {
    pub(super) fn sidebar(&mut self, ui: &mut Ui, shared: &Shared, actions: &mut Vec<Action>) {
        let p = Palette::of(ui.ctx());

        // Brand
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.add(egui::Image::new(&self.logo).fit_to_exact_size(vec2(32.0, 32.0)));
            ui.add_space(2.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("Ada").font(theme::semibold(16.0)).color(p.text));
                ui.label(RichText::new("Steam Update Watcher").size(12.0).color(p.text_faint));
            });
        });
        ui.add_space(22.0);

        // Status card pinned to the bottom.
        egui::Panel::bottom("sidebar-status").frame(Frame::NONE).resizable(false).show_separator_line(false).show(
            ui,
            |ui| {
                let st = &shared.status;
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(theme::RADIUS))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.set_width(ui.available_width());
                        let checking = st.checking.is_some();
                        let empty = shared.library.games.is_empty();
                        let (color, headline) = if checking {
                            (p.accent, "Checking for updates…".to_string())
                        } else if st.offline {
                            (p.warning, "Offline".to_string())
                        } else if empty {
                            (p.text_faint, "Idle".to_string())
                        } else if st.last_check.is_some() {
                            (p.success, "Watching for updates".to_string())
                        } else {
                            (p.text_faint, "Starting…".to_string())
                        };
                        ui.horizontal(|ui| {
                            if checking {
                                ui.add(egui::Spinner::new().size(12.0).color(p.accent));
                            } else {
                                pulse_dot(ui, color, !empty && !st.offline && st.last_check.is_some());
                            }
                            ui.label(RichText::new(headline).font(theme::semibold(13.0)).color(p.text));
                        });
                        let detail = if let (true, Some(id)) = (checking, st.checking) {
                            let name = shared.library.game(id).map(|g| g.name.as_str()).unwrap_or("…");
                            format!(
                                "{name}{}",
                                if st.pending > 1 { format!(" · {} left", st.pending - 1) } else { String::new() }
                            )
                        } else if st.offline {
                            "Can't reach Steam. Retrying automatically.".into()
                        } else if empty {
                            "Add a game to start watching.".into()
                        } else {
                            match (st.last_check, st.next_check) {
                                (Some(l), Some(n)) => format!(
                                    "Checked {}
Next check {}",
                                    widgets::ago(l),
                                    widgets::until(n)
                                ),
                                _ => "Preparing first check".into(),
                            }
                        };
                        ui.add(egui::Label::new(RichText::new(detail).size(12.0).color(p.text_muted)).wrap());
                        ui.add_space(6.0);
                        let enabled = !checking && !shared.library.games.is_empty();
                        if Btn::new("Check now")
                            .icon(ph::ARROWS_CLOCKWISE)
                            .small()
                            .full_width()
                            .enabled(enabled)
                            .show(ui)
                            .on_hover_text("Check all games now (Ctrl+R)")
                            .clicked()
                        {
                            actions.push(Action::CheckAll);
                        }
                    });
            },
        );

        let unseen = shared.library.unseen_count();
        let games = shared.library.games.len();
        let current = self.page;
        let nav = [
            (Page::Library, ph::GAME_CONTROLLER, "Library", (games > 0).then(|| games.to_string()), false),
            (Page::Activity, ph::PULSE, "Activity", (unseen > 0).then(|| unseen.to_string()), unseen > 0),
            (Page::Settings, ph::GEAR_SIX, "Settings", None, false),
        ];
        for (page, icon, label, count, highlight) in nav {
            if nav_item(ui, icon, label, count.as_deref(), highlight, current == page).clicked() {
                actions.push(Action::Navigate(page));
            }
        }
    }
}

fn nav_item(
    ui: &mut Ui,
    icon: &str,
    label: &str,
    count: Option<&str>,
    highlight: bool,
    selected: bool,
) -> egui::Response {
    let p = Palette::of(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let hovered = resp.hovered();
    if selected || hovered {
        let fill = if selected { p.surface_hover } else { p.tint(p.text_faint) };
        ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS_SM + 2), fill);
    }
    if selected {
        let bar = egui::Rect::from_min_size(pos2(rect.left(), rect.top() + 9.0), vec2(3.0, rect.height() - 18.0));
        ui.painter().rect_filled(bar, CornerRadius::same(2), p.accent);
    }
    let fg = if selected { p.text } else { p.text_muted };
    let icon_color = if selected { p.accent } else { fg };
    ui.painter().text(
        pos2(rect.left() + 14.0, rect.center().y),
        Align2::LEFT_CENTER,
        icon,
        theme::regular(18.0),
        icon_color,
    );
    ui.painter().text(
        pos2(rect.left() + 42.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        if selected { theme::semibold(14.0) } else { theme::regular(14.0) },
        fg,
    );
    if let Some(c) = count {
        let galley = ui.painter().layout_no_wrap(
            c.to_owned(),
            theme::semibold(11.5),
            if highlight { p.on_accent } else { p.text_muted },
        );
        let w = (galley.size().x + 12.0).max(22.0);
        let pill = egui::Rect::from_center_size(pos2(rect.right() - 10.0 - w / 2.0, rect.center().y), vec2(w, 20.0));
        let fill = if highlight { p.accent } else { p.tint(p.text_faint) };
        ui.painter().rect_filled(pill, CornerRadius::same(255), fill);
        ui.painter().galley(pill.center() - galley.size() / 2.0, galley, p.text);
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label));
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Status dot with a soft halo when live. Deliberately static: an idle
/// background app should cost zero CPU.
fn pulse_dot(ui: &mut Ui, color: egui::Color32, live: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
    if live {
        ui.painter().circle_filled(rect.center(), 6.0, color.gamma_multiply(0.25));
    }
    ui.painter().circle_filled(rect.center(), 3.5, color);
}
