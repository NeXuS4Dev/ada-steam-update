//! System tray integration (Windows & macOS). On other platforms the app simply
//! has no tray and closing the window quits.

use std::sync::mpsc::Sender;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    Show,
    CheckNow,
    Quit,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod imp {
    use super::*;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    pub struct Tray {
        icon: TrayIcon,
        normal: tray_icon::Icon,
        badged: tray_icon::Icon,
        badge_on: bool,
    }

    impl Tray {
        /// Show a red dot on the tray icon while there are unseen updates.
        pub fn set_badge(&mut self, on: bool, tooltip: &str) {
            if on != self.badge_on {
                self.badge_on = on;
                let icon = if on { self.badged.clone() } else { self.normal.clone() };
                let _ = self.icon.set_icon(Some(icon));
            }
            let _ = self.icon.set_tooltip(Some(tooltip));
        }
    }

    fn icon(badge: bool) -> Option<tray_icon::Icon> {
        let size = 32;
        let rgba = crate::icon_art::render_icon_with(size, badge);
        tray_icon::Icon::from_rgba(rgba, size, size).ok()
    }

    pub fn create(ctx: egui::Context, tx: Sender<TrayAction>) -> Option<Tray> {
        let open = MenuItem::new("Open Ada Steam Update", true, None);
        let check = MenuItem::new("Check for updates now", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &check, &PredefinedMenuItem::separator(), &quit]).ok()?;

        let (normal, badged) = (icon(false)?, icon(true)?);
        let tray = TrayIconBuilder::new()
            .with_tooltip(crate::platform::APP_NAME)
            .with_icon(normal.clone())
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| log::warn!("tray icon unavailable: {e}"))
            .ok()?;

        let (open_id, check_id, quit_id) = (open.id().clone(), check.id().clone(), quit.id().clone());
        {
            let (tx, ctx) = (tx.clone(), ctx.clone());
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                let action = if e.id == open_id {
                    TrayAction::Show
                } else if e.id == check_id {
                    TrayAction::CheckNow
                } else if e.id == quit_id {
                    TrayAction::Quit
                } else {
                    return;
                };
                let _ = tx.send(action);
                ctx.request_repaint();
            }));
        }
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            let show = matches!(
                e,
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                    | TrayIconEvent::DoubleClick { .. }
            );
            if show {
                let _ = tx.send(TrayAction::Show);
                ctx.request_repaint();
            }
        }));

        Some(Tray { icon: tray, normal, badged, badge_on: false })
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod imp {
    use super::*;

    pub struct Tray;

    impl Tray {
        pub fn set_badge(&mut self, _on: bool, _tooltip: &str) {}
    }

    pub fn create(_ctx: egui::Context, _tx: Sender<TrayAction>) -> Option<Tray> {
        None
    }
}

pub use imp::{Tray, create};
