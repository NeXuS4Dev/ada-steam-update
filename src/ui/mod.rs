//! The egui application: window lifecycle, navigation and shared UI plumbing.

mod activity;
mod add_dialog;
mod detail;
mod library;
mod notes;
mod settings;
mod sidebar;
pub mod theme;
pub mod tray;
pub mod widgets;

use std::collections::{HashMap, HashSet};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, RichText, Stroke, TextureHandle, TextureOptions, ThemePreference,
    ViewportCommand, vec2,
};
use egui_phosphor::regular as ph;

use crate::model::{AppId, Settings, ThemeChoice};
use crate::platform;
use crate::worker::{self, Command, Event, ImageKey, Shared, SharedState, WorkerHandle};
use theme::{PAGE_PAD, Palette};
use tray::TrayAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Library,
    Activity,
    Settings,
}

/// Everything a view can ask the app to do. Views only *read* shared state and
/// return actions; mutations happen in one place after rendering.
#[derive(Clone, Debug)]
pub enum Action {
    Navigate(Page),
    OpenGame(AppId, Option<u64>),
    SelectUpdate(u64),
    Back,
    CheckAll,
    Check(AppId),
    AskRemove(AppId),
    Remove(AppId),
    ToggleMute(AppId),
    MarkSeen(Option<AppId>),
    OpenUrl(String),
    ShowAdd(Option<String>),
    Add(AppId),
    UpdateSettings(Settings),
    TestNotification,
    OpenDataFolder,
    Copy(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToastKind {
    Success,
    Info,
    Error,
}

struct Toast {
    kind: ToastKind,
    title: String,
    body: Option<String>,
    created: Instant,
}

const TOAST_TTL: Duration = Duration::from_millis(4500);

pub struct AdaApp {
    state: SharedState,
    worker: WorkerHandle,
    events: Receiver<Event>,

    tray: Option<tray::Tray>,
    tray_rx: Receiver<TrayAction>,
    hidden: bool,
    quitting: bool,
    told_about_tray: bool,

    page: Page,
    selected_game: Option<AppId>,
    selected_update: Option<u64>,
    confirm_remove: Option<AppId>,
    filter: String,
    add: Option<add_dialog::AddDialog>,
    adding: HashSet<AppId>,

    textures: HashMap<ImageKey, TextureHandle>,
    /// Images requested from the worker and when; failed ones are retried after a while.
    requested: HashMap<ImageKey, Instant>,
    logo: TextureHandle,
    notes_cache: notes::NotesCache,

    toasts: Vec<Toast>,
    applied_theme: Option<ThemeChoice>,
}

impl AdaApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: SharedState,
        listener: Option<TcpListener>,
        start_hidden: bool,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        theme::install_fonts(&ctx);
        theme::install_style(&ctx);

        let repaint_ctx = ctx.clone();
        let (worker, events) = worker::spawn(state.clone(), move || repaint_ctx.request_repaint());

        let (tray_tx, tray_rx) = mpsc::channel();
        let tray = tray::create(ctx.clone(), tray_tx.clone());

        // Later launches (`ada-steam-update`, `--quit`) talk to us through the same channel as the tray.
        if let Some(l) = listener {
            let ctx = ctx.clone();
            platform::serve_signals(l, move |signal| {
                let action = match signal {
                    platform::Signal::Show => TrayAction::Show,
                    platform::Signal::Quit => TrayAction::Quit,
                };
                let _ = tray_tx.send(action);
                ctx.request_repaint();
            });
        }

        let logo = {
            let size = 64;
            let img =
                egui::ColorImage::from_rgba_unmultiplied([size, size], &crate::icon_art::render_icon(size as u32));
            ctx.load_texture("ada-logo", img, TextureOptions::LINEAR)
        };

        // Without a tray there is nowhere to hide: make sure the window exists and start minimized.
        if start_hidden && tray.is_none() {
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        }

        Self {
            state,
            worker,
            events,
            hidden: start_hidden && tray.is_some(),
            tray,
            tray_rx,
            quitting: false,
            told_about_tray: false,
            page: Page::Library,
            selected_game: None,
            selected_update: None,
            confirm_remove: None,
            filter: String::new(),
            add: None,
            adding: HashSet::new(),
            textures: HashMap::new(),
            requested: HashMap::new(),
            logo,
            notes_cache: notes::NotesCache::default(),
            toasts: Vec::new(),
            applied_theme: None,
        }
    }

    // -----------------------------------------------------------------------
    // Plumbing
    // -----------------------------------------------------------------------

    fn toast(&mut self, kind: ToastKind, title: impl Into<String>, body: Option<String>) {
        self.toasts.push(Toast { kind, title: title.into(), body, created: Instant::now() });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    /// Texture for an image, requesting it from the worker on first use.
    pub(crate) fn texture(&mut self, key: ImageKey) -> Option<TextureHandle> {
        if let Some(t) = self.textures.get(&key) {
            return Some(t.clone());
        }
        let retry = Duration::from_secs(worker::IMAGE_RETRY_SECS as u64 + 1);
        let due = self.requested.get(&key).is_none_or(|t| t.elapsed() > retry);
        if due {
            self.requested.insert(key.clone(), Instant::now());
            self.worker.send(Command::LoadImage(key));
        }
        None
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.events.try_recv() {
            match ev {
                Event::Image { key, image } => {
                    let name = format!("{key:?}");
                    let tex = ctx.load_texture(name, image, TextureOptions::LINEAR);
                    self.textures.insert(key, tex);
                }
                Event::Added { app_id, name } => {
                    self.adding.remove(&app_id);
                    if let Some(d) = &mut self.add {
                        d.added.insert(app_id);
                    }
                    self.toast(
                        ToastKind::Success,
                        format!("Now tracking {name}"),
                        Some("You'll be notified when it updates.".into()),
                    );
                }
                Event::AddFailed { app_id, error } => {
                    self.adding.remove(&app_id);
                    self.toast(ToastKind::Error, "Couldn't add game", Some(error));
                }
                Event::SearchResults { request, hits } => {
                    if let Some(d) = &mut self.add {
                        d.on_results(request, Ok(hits));
                    }
                }
                Event::SearchFailed { request, error } => {
                    if let Some(d) = &mut self.add {
                        d.on_results(request, Err(error));
                    }
                }
                Event::UpdatesFound(found) => {
                    let title = match found.as_slice() {
                        [one] => format!("{} was updated", one.name),
                        many => format!("{} games were updated", many.len()),
                    };
                    let body = found.first().map(|f| format!("{} · {}", f.kind.label(), f.title));
                    self.toast(ToastKind::Info, title, body);
                }
                Event::CheckFailed { app_id, error } => {
                    let name = self.state.lock().library.game(app_id).map(|g| g.name.clone()).unwrap_or_default();
                    self.toast(ToastKind::Error, format!("Couldn't check {name}"), Some(error));
                }
            }
        }
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Navigate(p) => {
                self.page = p;
                self.selected_game = None;
                self.selected_update = None;
            }
            Action::OpenGame(id, update) => {
                self.page = Page::Library;
                self.selected_game = Some(id);
                self.selected_update = update;
            }
            Action::SelectUpdate(b) => self.selected_update = Some(b),
            Action::Back => {
                self.selected_game = None;
                self.selected_update = None;
            }
            Action::CheckAll => self.worker.send(Command::CheckAll),
            Action::Check(id) => self.worker.send(Command::Check(id)),
            Action::AskRemove(id) => self.confirm_remove = Some(id),
            Action::Remove(id) => {
                let name = {
                    let mut s = self.state.lock();
                    let name = s.library.game(id).map(|g| g.name.clone()).unwrap_or_default();
                    s.library.remove(id);
                    self.state.save_library(&s);
                    name
                };
                self.confirm_remove = None;
                if self.selected_game == Some(id) {
                    self.selected_game = None;
                }
                self.toast(ToastKind::Info, format!("Stopped tracking {name}"), None);
            }
            Action::ToggleMute(id) => {
                let mut s = self.state.lock();
                if let Some(g) = s.library.game_mut(id) {
                    g.muted = !g.muted;
                }
                self.state.save_library(&s);
            }
            Action::MarkSeen(id) => {
                let mut s = self.state.lock();
                s.library.mark_seen(id);
                self.state.save_library(&s);
            }
            Action::OpenUrl(url) => {
                if let Err(e) = open::that_detached(&url) {
                    self.toast(ToastKind::Error, "Couldn't open link", Some(e.to_string()));
                }
            }
            Action::ShowAdd(prefill) => {
                let mut d = add_dialog::AddDialog::default();
                if let Some(p) = prefill {
                    d.query = p;
                }
                self.add = Some(d);
            }
            Action::Add(id) => {
                if self.adding.insert(id) {
                    self.worker.send(Command::Add(id));
                }
            }
            Action::UpdateSettings(new) => self.update_settings(new),
            Action::TestNotification => platform::notify(
                "Notifications are working",
                "You'll see a message like this whenever a tracked game updates.",
            ),
            Action::OpenDataFolder => {
                let dir = self.state.paths().data.clone();
                let _ = std::fs::create_dir_all(&dir);
                let _ = open::that_detached(dir);
            }
            Action::Copy(text) => {
                ctx.copy_text(text);
                self.toast(ToastKind::Success, "Copied to clipboard", None);
            }
        }
    }

    fn update_settings(&mut self, new: Settings) {
        let old = self.state.lock().settings.clone();
        let mut new = new;
        if new.launch_at_startup != old.launch_at_startup
            && let Err(e) = platform::set_launch_at_startup(new.launch_at_startup)
        {
            new.launch_at_startup = old.launch_at_startup;
            self.toast(ToastKind::Error, "Couldn't change startup setting", Some(format!("{e:#}")));
        }
        {
            let mut s = self.state.lock();
            s.settings = new.clone();
            self.state.save_settings(&s);
        }
        if new.check_interval_min != old.check_interval_min {
            self.worker.send(Command::Reschedule);
        }
    }

    fn sync_theme(&mut self, ctx: &egui::Context, choice: ThemeChoice) {
        if self.applied_theme != Some(choice) {
            self.applied_theme = Some(choice);
            ctx.set_theme(match choice {
                ThemeChoice::System => ThemePreference::System,
                ThemeChoice::Dark => ThemePreference::Dark,
                ThemeChoice::Light => ThemePreference::Light,
            });
        }
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str, right: impl FnOnce(&mut egui::Ui)) {
        let p = Palette::of(ui.ctx());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(title).font(theme::semibold(24.0)).color(p.text));
                ui.label(RichText::new(subtitle).size(14.0).color(p.text_muted));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
        });
    }

    fn render_toasts(&mut self, ctx: &egui::Context) {
        let p = Palette::of(ctx);
        self.toasts.retain(|t| t.created.elapsed() < TOAST_TTL);
        if self.toasts.is_empty() {
            return;
        }
        let mut dismiss = None;
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-20.0, -20.0))
            .order(egui::Order::Tooltip)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_width(340.0);
                for (i, t) in self.toasts.iter().enumerate() {
                    let age = t.created.elapsed().as_secs_f32();
                    let remaining = TOAST_TTL.as_secs_f32() - age;
                    let opacity = (age / 0.15).min(1.0).min(remaining / 0.3).clamp(0.0, 1.0);
                    let (icon, color) = match t.kind {
                        ToastKind::Success => (ph::CHECK_CIRCLE, p.success),
                        ToastKind::Info => (ph::BELL, p.accent),
                        ToastKind::Error => (ph::WARNING_CIRCLE, p.danger),
                    };
                    let resp = Frame::new()
                        .fill(p.elevated)
                        .stroke(Stroke::new(1.0, p.border_strong))
                        .corner_radius(CornerRadius::same(theme::RADIUS))
                        .inner_margin(Margin::symmetric(14, 12))
                        .shadow(ui.visuals().popup_shadow)
                        .multiply_with_opacity(opacity)
                        .show(ui, |ui| {
                            ui.set_width(312.0);
                            ui.horizontal_top(|ui| {
                                ui.label(RichText::new(icon).size(18.0).color(color.gamma_multiply(opacity)));
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    ui.label(
                                        RichText::new(&t.title)
                                            .font(theme::semibold(14.0))
                                            .color(p.text.gamma_multiply(opacity)),
                                    );
                                    if let Some(b) = &t.body {
                                        ui.label(
                                            RichText::new(b).size(13.0).color(p.text_muted.gamma_multiply(opacity)),
                                        );
                                    }
                                });
                            });
                        })
                        .response
                        .interact(egui::Sense::click());
                    if resp.clicked() {
                        dismiss = Some(i);
                    }
                    ui.add_space(4.0);
                }
            });
        if let Some(i) = dismiss {
            self.toasts.remove(i);
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    fn render_remove_confirm(&mut self, ctx: &egui::Context, shared: &Shared, actions: &mut Vec<Action>) {
        let Some(id) = self.confirm_remove else { return };
        let p = Palette::of(ctx);
        let name = shared.library.game(id).map(|g| g.name.clone()).unwrap_or_default();
        let modal = egui::Modal::new(egui::Id::new("confirm-remove"))
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
                ui.set_width(380.0);
                ui.label(RichText::new(format!("Stop tracking {name}?")).font(theme::semibold(17.0)).color(p.text));
                ui.add_space(4.0);
                widgets::muted(ui, "Its update history will be removed. You can add it again at any time.");
                ui.add_space(16.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::Btn::danger("Remove").icon(ph::TRASH).show(ui).clicked() {
                        actions.push(Action::Remove(id));
                    }
                    if widgets::Btn::new("Cancel").show(ui).clicked() {
                        self.confirm_remove = None;
                    }
                });
            });
        if modal.should_close() {
            self.confirm_remove = None;
        }
    }
}

impl eframe::App for AdaApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events(ctx);

        let (theme_choice, close_to_tray, unseen) = {
            let s = self.state.lock();
            (s.settings.theme, s.settings.close_to_tray, s.library.unseen_count())
        };
        self.sync_theme(ctx, theme_choice);

        if let Some(t) = &mut self.tray {
            let tip = match unseen {
                0 => platform::APP_NAME.to_string(),
                1 => format!("{} — 1 new update", platform::APP_NAME),
                n => format!("{} — {n} new updates", platform::APP_NAME),
            };
            t.set_badge(unseen > 0, &tip);
        }

        while let Ok(a) = self.tray_rx.try_recv() {
            match a {
                TrayAction::Show => self.show_window(ctx),
                TrayAction::CheckNow => self.worker.send(Command::CheckAll),
                TrayAction::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                    ctx.request_repaint();
                }
            }
        }

        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && close_to_tray && self.tray.is_some() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            self.hidden = true;
            if !self.told_about_tray {
                self.told_about_tray = true;
                platform::notify(
                    "Ada is still running",
                    "It keeps watching for updates from the system tray. Right-click the tray icon to quit.",
                );
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let p = Palette::of(&ctx);
        let state = self.state.clone();
        let mut actions = Vec::new();

        {
            let shared = state.lock();

            egui::Panel::left("sidebar")
                .exact_size(theme::SIDEBAR_WIDTH)
                .resizable(false)
                .frame(
                    Frame::new()
                        .fill(p.sidebar)
                        .inner_margin(Margin { left: 14, right: 14, top: 18, bottom: 14 })
                        .stroke(Stroke::new(1.0, p.border)),
                )
                .show(ui, |ui| self.sidebar(ui, &shared, &mut actions));

            egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt((self.page as u8, self.selected_game))
                    .show(ui, |ui| {
                        Frame::new()
                            .inner_margin(Margin { left: PAGE_PAD, right: PAGE_PAD, top: 24, bottom: PAGE_PAD })
                            .show(ui, |ui| match (self.page, self.selected_game) {
                                (Page::Library, Some(id)) if shared.library.contains(id) => {
                                    self.detail(ui, &shared, id, &mut actions)
                                }
                                (Page::Library, _) => self.library(ui, &shared, &mut actions),
                                (Page::Activity, _) => self.activity(ui, &shared, &mut actions),
                                (Page::Settings, _) => self.settings(ui, &shared, &mut actions),
                            });
                    });
            });

            if self.add.is_some() {
                self.add_dialog(&ctx, &shared, &mut actions);
            }
            self.render_remove_confirm(&ctx, &shared, &mut actions);
        }

        self.render_toasts(&ctx);

        // Keyboard shortcuts.
        let (new_game, escape, refresh) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::N),
                i.key_pressed(egui::Key::Escape),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::R)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::F5),
            )
        });
        if new_game && self.add.is_none() {
            actions.push(Action::ShowAdd(None));
        }
        if refresh {
            actions.push(Action::CheckAll);
        }
        if escape && self.add.is_none() && self.confirm_remove.is_none() && self.selected_game.is_some() {
            actions.push(Action::Back);
        }
        if ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra1)) && self.selected_game.is_some() {
            actions.push(Action::Back);
        }

        for a in actions {
            self.apply(&ctx, a);
        }

        // Keep relative times ("2 min ago") fresh without burning CPU.
        ctx.request_repaint_after(Duration::from_secs(15));
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }
}
