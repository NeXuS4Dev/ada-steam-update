//! Reusable, themed UI components.

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, Frame, Margin, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
    WidgetInfo, WidgetType, pos2, vec2,
};
use egui_phosphor::regular as ph;

use super::theme::{self, Palette, RADIUS, RADIUS_SM};
use crate::model::{Platform, Timestamp, UpdateKind};

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

pub struct Btn<'a> {
    icon: Option<&'a str>,
    text: &'a str,
    variant: Variant,
    enabled: bool,
    small: bool,
    full_width: bool,
}

impl<'a> Btn<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { icon: None, text, variant: Variant::Secondary, enabled: true, small: false, full_width: false }
    }
    pub fn primary(text: &'a str) -> Self {
        Self::new(text).variant(Variant::Primary)
    }
    pub fn ghost(text: &'a str) -> Self {
        Self::new(text).variant(Variant::Ghost)
    }
    pub fn danger(text: &'a str) -> Self {
        Self::new(text).variant(Variant::Danger)
    }
    pub fn icon(mut self, icon: &'a str) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn variant(mut self, v: Variant) -> Self {
        self.variant = v;
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let p = Palette::of(ui.ctx());
        let font = if self.small { theme::semibold(13.0) } else { theme::semibold(14.0) };
        let height = if self.small { 30.0 } else { 36.0 };
        let pad_x = if self.small { 10.0 } else { 14.0 };
        let gap = 7.0;

        let text_color = Color32::PLACEHOLDER;
        let icon_galley = self.icon.map(|i| {
            ui.painter().layout_no_wrap(i.to_owned(), theme::regular(if self.small { 15.0 } else { 16.0 }), text_color)
        });
        let text_galley =
            (!self.text.is_empty()).then(|| ui.painter().layout_no_wrap(self.text.to_owned(), font, text_color));

        let content_w = icon_galley.as_ref().map_or(0.0, |g| g.size().x)
            + text_galley.as_ref().map_or(0.0, |g| g.size().x)
            + if icon_galley.is_some() && text_galley.is_some() { gap } else { 0.0 };
        let mut width = content_w + pad_x * 2.0;
        if text_galley.is_none() {
            width = height; // square icon button
        }
        if self.full_width {
            width = width.max(ui.available_width());
        }

        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, resp) = ui.allocate_exact_size(vec2(width, height), sense);
        resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, self.enabled, self.text));

        if ui.is_rect_visible(rect) {
            let hovered = self.enabled && resp.hovered();
            let pressed = self.enabled && resp.is_pointer_button_down_on();
            let (fill, stroke, fg) = match self.variant {
                Variant::Primary => (
                    if pressed {
                        p.accent
                    } else if hovered {
                        p.accent_hover
                    } else {
                        p.accent
                    },
                    Stroke::NONE,
                    p.on_accent,
                ),
                Variant::Secondary => (
                    if hovered { p.surface_hover } else { p.surface },
                    Stroke::new(1.0, if hovered { p.border_strong } else { p.border }),
                    p.text,
                ),
                Variant::Ghost => (
                    if hovered { p.surface_hover } else { Color32::TRANSPARENT },
                    Stroke::NONE,
                    if hovered { p.text } else { p.text_muted },
                ),
                Variant::Danger => {
                    (if hovered { p.tint_strong(p.danger) } else { p.tint(p.danger) }, Stroke::NONE, p.danger)
                }
            };
            let alpha = if self.enabled { 1.0 } else { 0.45 };
            let painter = ui.painter();
            let rect = if pressed { rect.shrink(0.5) } else { rect };
            painter.rect(
                rect,
                CornerRadius::same(RADIUS_SM + 2),
                fill.gamma_multiply(alpha),
                stroke,
                StrokeKind::Inside,
            );
            let fg = fg.gamma_multiply(alpha);
            let mut x = rect.center().x - content_w / 2.0;
            if let Some(g) = icon_galley {
                let pos = pos2(x, rect.center().y - g.size().y / 2.0);
                x += g.size().x + gap;
                painter.galley(pos, g, fg);
            }
            if let Some(g) = text_galley {
                let pos = pos2(x, rect.center().y - g.size().y / 2.0);
                painter.galley(pos, g, fg);
            }
            if resp.has_focus() {
                painter.rect_stroke(
                    rect.expand(2.0),
                    CornerRadius::same(RADIUS_SM + 4),
                    Stroke::new(2.0, p.tint_strong(p.accent)),
                    StrokeKind::Outside,
                );
            }
        }
        if self.enabled { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp }
    }
}

/// Square icon-only ghost button with tooltip.
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str) -> Response {
    Btn::ghost("").icon(icon).show(ui).on_hover_text(tooltip)
}

// ---------------------------------------------------------------------------
// Toggle switch & segmented control
// ---------------------------------------------------------------------------

pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let p = Palette::of(ui.ctx());
    let size = vec2(38.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, *on, ""));
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(resp.id, *on);
        let off_bg = if p.dark { p.border_strong } else { Color32::from_rgb(0xD5, 0xDA, 0xE3) };
        let bg = lerp_color(off_bg, p.accent, t);
        let radius = rect.height() / 2.0;
        ui.painter().rect_filled(rect, radius, bg);
        let cx = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        let knob = pos2(cx, rect.center().y);
        ui.painter().circle_filled(knob + vec2(0.0, 1.0), radius - 3.0, Color32::from_black_alpha(40));
        ui.painter().circle_filled(knob, radius - 3.0, Color32::WHITE);
        if resp.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                radius + 2.0,
                Stroke::new(2.0, p.tint_strong(p.accent)),
                StrokeKind::Outside,
            );
        }
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str, &str)]) -> bool {
    let p = Palette::of(ui.ctx());
    let mut changed = false;
    Frame::new()
        .fill(if p.dark { p.bg } else { p.surface_hover })
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(RADIUS_SM + 2))
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (opt, icon, label) in options {
                    let selected = *value == *opt;
                    let text = format!("{icon}  {label}");
                    let galley = ui.painter().layout_no_wrap(text, theme::semibold(13.0), Color32::PLACEHOLDER);
                    let size = vec2(galley.size().x + 22.0, 28.0);
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    if resp.clicked() && !selected {
                        *value = *opt;
                        changed = true;
                    }
                    let fill = if selected {
                        if p.dark { p.surface_hover } else { p.surface }
                    } else if resp.hovered() {
                        p.tint(p.text_faint)
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, CornerRadius::same(RADIUS_SM), fill);
                    if selected {
                        ui.painter().rect_stroke(
                            rect,
                            CornerRadius::same(RADIUS_SM),
                            Stroke::new(1.0, p.border_strong),
                            StrokeKind::Inside,
                        );
                    }
                    let fg = if selected { p.text } else { p.text_muted };
                    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
                    resp.on_hover_cursor(CursorIcon::PointingHand);
                }
            });
        });
    changed
}

// ---------------------------------------------------------------------------
// Badges & small pieces
// ---------------------------------------------------------------------------

pub fn pill(ui: &mut Ui, icon: Option<&str>, text: &str, color: Color32) -> Response {
    let p = Palette::of(ui.ctx());
    let label = match icon {
        Some(i) => format!("{i} {text}"),
        None => text.to_owned(),
    };
    let galley = ui.painter().layout_no_wrap(label, theme::semibold(12.0), color);
    let size = galley.size() + vec2(16.0, 6.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(size.x, size.y.max(22.0)), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(255), p.tint(color));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

pub fn kind_icon(kind: UpdateKind) -> &'static str {
    match kind {
        UpdateKind::Major => ph::ROCKET_LAUNCH,
        UpdateKind::Regular => ph::SPARKLE,
        UpdateKind::Small => ph::WRENCH,
        UpdateKind::Silent => ph::EYE_SLASH,
    }
}

pub fn kind_badge(ui: &mut Ui, kind: UpdateKind) -> Response {
    let p = Palette::of(ui.ctx());
    pill(ui, Some(kind_icon(kind)), kind.label(), p.kind(kind)).on_hover_text(kind.description())
}

pub fn platform_icon(pl: Platform) -> &'static str {
    match pl {
        Platform::Windows => ph::WINDOWS_LOGO,
        Platform::MacOs => ph::APPLE_LOGO,
        Platform::Linux => ph::LINUX_LOGO,
    }
}

pub fn platforms_text(platforms: &[Platform]) -> String {
    if platforms.is_empty() {
        return format!("{}  All platforms", ph::MONITOR);
    }
    platforms.iter().map(|p| format!("{}  {}", platform_icon(*p), p.label())).collect::<Vec<_>>().join("   ")
}

/// Small unread dot.
pub fn dot(ui: &mut Ui, color: Color32, radius: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(radius * 2.0 + 2.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), radius, color);
    resp
}

pub fn card(p: &Palette) -> Frame {
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::same(20))
}

pub fn section_title(ui: &mut Ui, title: &str, subtitle: Option<&str>) {
    let p = Palette::of(ui.ctx());
    ui.label(RichText::new(title).font(theme::semibold(15.0)).color(p.text));
    if let Some(s) = subtitle {
        ui.add_space(-4.0);
        ui.label(RichText::new(s).size(13.0).color(p.text_muted));
    }
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = Palette::of(ui.ctx());
    ui.label(RichText::new(text.into()).size(13.0).color(p.text_muted))
}

/// Label/value pair used in stat rows.
pub fn stat(ui: &mut Ui, label: &str, value: &str) {
    let p = Palette::of(ui.ctx());
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        ui.label(RichText::new(label.to_uppercase()).font(theme::semibold(11.0)).color(p.text_faint));
        ui.label(RichText::new(value).font(theme::semibold(14.0)).color(p.text));
    });
}

/// Paint a centered empty/zero state with an icon in a soft circle.
pub fn empty_state(ui: &mut Ui, icon: &str, title: &str, body: &str) {
    let p = Palette::of(ui.ctx());
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(72.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 36.0, p.tint(p.accent));
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, theme::regular(32.0), p.accent);
        ui.add_space(14.0);
        ui.label(RichText::new(title).font(theme::semibold(18.0)).color(p.text));
        ui.add_space(2.0);
        ui.label(RichText::new(body).size(14.0).color(p.text_muted));
    });
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

pub fn ago(ts: Timestamp) -> String {
    ago_from(ts, crate::model::now())
}

pub fn ago_from(ts: Timestamp, now: Timestamp) -> String {
    let d = now - ts;
    if d < 0 {
        return "just now".into();
    }
    match d {
        0..45 => "just now".into(),
        45..90 => "1 min ago".into(),
        90..3600 => format!("{} min ago", (d + 30) / 60),
        3600..5400 => "1 hour ago".into(),
        5400..86400 => format!("{} hours ago", (d + 1800) / 3600),
        86400..172800 => "yesterday".into(),
        172800..2_592_000 => format!("{} days ago", d / 86400),
        2_592_000..31_536_000 => {
            let m = d / 2_592_000;
            if m <= 1 { "1 month ago".into() } else { format!("{m} months ago") }
        }
        _ => {
            let y = d / 31_536_000;
            if y <= 1 { "1 year ago".into() } else { format!("{y} years ago") }
        }
    }
}

pub fn until(ts: Timestamp) -> String {
    let d = ts - crate::model::now();
    match d {
        ..=30 => "any moment".into(),
        31..90 => "in 1 min".into(),
        90..3600 => format!("in {} min", (d + 30) / 60),
        _ => format!("in {} h", (d + 1800) / 3600),
    }
}

fn local(ts: Timestamp) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::from_timestamp(ts, 0).map(|d| d.with_timezone(&chrono::Local))
}

pub fn date_time(ts: Timestamp) -> String {
    local(ts).map(|d| d.format("%b %-d, %Y · %H:%M").to_string()).unwrap_or_default()
}

pub fn day_label(ts: Timestamp) -> String {
    let Some(d) = local(ts) else { return String::new() };
    let today = chrono::Local::now().date_naive();
    let day = d.date_naive();
    if day == today {
        "Today".into()
    } else if Some(day) == today.pred_opt() {
        "Yesterday".into()
    } else if (today - day).num_days() < 7 {
        d.format("%A").to_string()
    } else {
        d.format("%B %-d, %Y").to_string()
    }
}

pub fn time_of_day(ts: Timestamp) -> String {
    local(ts).map(|d| d.format("%H:%M").to_string()).unwrap_or_default()
}

pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1000.0 && u < UNITS.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else if v >= 100.0 {
        format!("{v:.0} {}", UNITS[u])
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

pub fn signed_bytes(n: i64) -> String {
    match n.cmp(&0) {
        std::cmp::Ordering::Equal => "No change".into(),
        std::cmp::Ordering::Greater => format!("+{}", bytes(n as u64)),
        std::cmp::Ordering::Less => format!("−{}", bytes(n.unsigned_abs())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_times() {
        assert_eq!(ago_from(100, 110), "just now");
        assert_eq!(ago_from(0, 60), "1 min ago");
        assert_eq!(ago_from(0, 600), "10 min ago");
        assert_eq!(ago_from(0, 3 * 3600), "3 hours ago");
        assert_eq!(ago_from(0, 86400 + 10), "yesterday");
        assert_eq!(ago_from(0, 5 * 86400), "5 days ago");
        assert_eq!(ago_from(0, 400 * 86400), "1 year ago");
        assert_eq!(ago_from(100, 50), "just now");
    }

    #[test]
    fn byte_formatting() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1_500), "1.5 KB");
        assert_eq!(bytes(63_452_679_472), "63.5 GB");
        assert_eq!(bytes(250_000_000), "250 MB");
        assert_eq!(signed_bytes(-2_000_000), "−2.0 MB");
        assert_eq!(signed_bytes(0), "No change");
    }
}
