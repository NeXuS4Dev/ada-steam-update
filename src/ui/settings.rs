use egui::{RichText, Ui};
use egui_phosphor::regular as ph;

use super::theme::{self, Palette};
use super::widgets::{self, Btn};
use super::{Action, AdaApp};
use crate::model::{CHECK_INTERVALS_MIN, ThemeChoice};
use crate::worker::Shared;

impl AdaApp {
    pub(super) fn settings(&mut self, ui: &mut Ui, shared: &Shared, actions: &mut Vec<Action>) {
        let p = Palette::of(ui.ctx());
        let original = shared.settings.clone();
        let mut s = original.clone();
        let has_tray = self.tray.is_some();

        Self::page_header(ui, "Settings", "Preferences are saved automatically.", |_| {});
        ui.add_space(20.0);

        let max_w = 760.0_f32.min(ui.available_width());
        ui.allocate_ui(egui::vec2(max_w, 0.0), |ui| {
            section(ui, "General", |ui| {
                row(ui, "Launch at startup", "Start Ada automatically when you sign in to your computer.", |ui| {
                    widgets::toggle(ui, &mut s.launch_at_startup);
                });
                divider(ui);
                row(
                    ui,
                    "Start in the background",
                    if has_tray {
                        "When launched at startup, stay in the system tray instead of opening the window."
                    } else {
                        "When launched at startup, start minimized."
                    },
                    |ui| {
                        ui.add_enabled_ui(s.launch_at_startup, |ui| widgets::toggle(ui, &mut s.start_minimized));
                    },
                );
                if has_tray {
                    divider(ui);
                    row(ui, "Keep running when closed", "Closing the window keeps Ada watching from the system tray.", |ui| {
                        widgets::toggle(ui, &mut s.close_to_tray);
                    });
                }
            });

            section(ui, "Notifications", |ui| {
                row(ui, "Desktop notifications", "Show a system notification when a tracked game updates.", |ui| {
                    widgets::toggle(ui, &mut s.notifications);
                });
                divider(ui);
                row(ui, "Send a test notification", "Make sure notifications are allowed by your system.", |ui| {
                    if Btn::new("Send test").icon(ph::BELL).small().enabled(s.notifications).show(ui).clicked() {
                        actions.push(Action::TestNotification);
                    }
                });
            });

            section(ui, "Update checks", |ui| {
                row(
                    ui,
                    "Check every",
                    "How often Ada asks Steam for new builds. Updates that ship while Ada is closed are reported on the next launch.",
                    |ui| {
                        let label = |m: u32| if m < 60 { format!("{m} minutes") } else if m == 60 { "1 hour".into() } else { format!("{} hours", m / 60) };
                        egui::ComboBox::from_id_salt("interval")
                            .selected_text(label(s.check_interval_min))
                            .width(130.0)
                            .show_ui(ui, |ui| {
                                for m in CHECK_INTERVALS_MIN {
                                    ui.selectable_value(&mut s.check_interval_min, m, label(m));
                                }
                            });
                    },
                );
            });

            section(ui, "Appearance", |ui| {
                row(ui, "Theme", "Follow your system, or pick a fixed look.", |ui| {
                    widgets::segmented(
                        ui,
                        &mut s.theme,
                        &[
                            (ThemeChoice::System, ph::MONITOR, "System"),
                            (ThemeChoice::Dark, ph::MOON, "Dark"),
                            (ThemeChoice::Light, ph::SUN, "Light"),
                        ],
                    );
                });
            });

            section(ui, "Data", |ui| {
                row(
                    ui,
                    "Library & history",
                    &format!("Stored locally in {}", shared_path(self)),
                    |ui| {
                        if Btn::new("Open folder").icon(ph::FOLDER_OPEN).small().show(ui).clicked() {
                            actions.push(Action::OpenDataFolder);
                        }
                    },
                );
            });

            section(ui, "About", |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.logo).fit_to_exact_size(egui::vec2(40.0, 40.0)));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new(format!("Ada Steam Update {}", env!("CARGO_PKG_VERSION"))).font(theme::semibold(14.0)).color(p.text));
                        widgets::muted(ui, "Free and open source. Build data from api.steamcmd.net, patch notes from Steam.");
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if Btn::new("Source code").icon(ph::GITHUB_LOGO).small().show(ui).clicked() {
                            actions.push(Action::OpenUrl(env!("CARGO_PKG_REPOSITORY").into()));
                        }
                    });
                });
                ui.add_space(6.0);
                widgets::muted(ui, "Not affiliated with Valve Corporation. Steam is a trademark of Valve Corporation.");
            });
        });

        if s != original {
            actions.push(Action::UpdateSettings(s));
        }
    }
}

fn shared_path(app: &AdaApp) -> String {
    app.state.paths().data.display().to_string()
}

fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui.ctx());
    ui.label(RichText::new(title).font(theme::semibold(13.0)).color(p.text_muted));
    ui.add_space(2.0);
    widgets::card(p).inner_margin(egui::Margin::symmetric(20, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
    ui.add_space(22.0);
}

fn divider(ui: &mut Ui) {
    let p = Palette::of(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.border);
}

fn row(ui: &mut Ui, title: &str, description: &str, control: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui.ctx());
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let text_w = (ui.available_width() - 220.0).max(200.0);
        ui.allocate_ui_with_layout(egui::vec2(text_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(title).font(theme::semibold(14.0)).color(p.text));
            ui.add(egui::Label::new(RichText::new(description).size(13.0).color(p.text_muted)).wrap());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
    ui.add_space(10.0);
}
