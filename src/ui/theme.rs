//! Design tokens and egui style setup.
//!
//! Every color used by the UI comes from a [`Palette`]; views never hard-code
//! colors, so both themes stay consistent.

use egui::epaint::Shadow;
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, TextStyle, Theme, Visuals,
};

use crate::model::UpdateKind;

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub bg: Color32,
    pub sidebar: Color32,
    pub surface: Color32,
    pub surface_hover: Color32,
    pub elevated: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub major: Color32,
    pub regular: Color32,
    pub small: Color32,
    pub silent: Color32,
    pub shadow: Color32,
    pub dark: bool,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const DARK: Palette = Palette {
    bg: hex(0x0B0D12),
    sidebar: hex(0x0F1218),
    surface: hex(0x141821),
    surface_hover: hex(0x1A1F2B),
    elevated: hex(0x171B26),
    border: hex(0x222836),
    border_strong: hex(0x2F3647),
    text: hex(0xE8EBF2),
    text_muted: hex(0xA0A8BA),
    text_faint: hex(0x6B7487),
    accent: hex(0x5B8CFF),
    accent_hover: hex(0x7AA2FF),
    on_accent: hex(0xFFFFFF),
    success: hex(0x3DD68C),
    warning: hex(0xF5B544),
    danger: hex(0xF26B6B),
    major: hex(0xB794FF),
    regular: hex(0x5B8CFF),
    small: hex(0x2EC4B6),
    silent: hex(0xF5B544),
    shadow: Color32::from_black_alpha(110),
    dark: true,
};

pub const LIGHT: Palette = Palette {
    bg: hex(0xF5F6F8),
    sidebar: hex(0xFBFBFC),
    surface: hex(0xFFFFFF),
    surface_hover: hex(0xF3F5F9),
    elevated: hex(0xFFFFFF),
    border: hex(0xE3E6EC),
    border_strong: hex(0xCDD2DB),
    text: hex(0x111827),
    text_muted: hex(0x4B5565),
    text_faint: hex(0x8C95A6),
    accent: hex(0x3E6FF5),
    accent_hover: hex(0x2C5BE0),
    on_accent: hex(0xFFFFFF),
    success: hex(0x0F9F5A),
    warning: hex(0xB86E00),
    danger: hex(0xD43B2F),
    major: hex(0x7646F0),
    regular: hex(0x3E6FF5),
    small: hex(0x0B8A7D),
    silent: hex(0xB86E00),
    shadow: Color32::from_black_alpha(28),
    dark: false,
};

impl Palette {
    pub fn of(ctx: &egui::Context) -> &'static Palette {
        if ctx.theme() == Theme::Dark { &DARK } else { &LIGHT }
    }

    pub fn kind(&self, kind: UpdateKind) -> Color32 {
        match kind {
            UpdateKind::Major => self.major,
            UpdateKind::Regular => self.regular,
            UpdateKind::Small => self.small,
            UpdateKind::Silent => self.silent,
        }
    }

    /// A translucent tint of `c` suitable for badge / banner backgrounds.
    pub fn tint(&self, c: Color32) -> Color32 {
        let a = if self.dark { 38 } else { 26 };
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
    }

    pub fn tint_strong(&self, c: Color32) -> Color32 {
        let a = if self.dark { 70 } else { 50 };
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
    }
}

// ---------------------------------------------------------------------------
// Spacing / radius scale (4-pt grid)
// ---------------------------------------------------------------------------

pub const RADIUS_SM: u8 = 6;
pub const RADIUS: u8 = 10;
pub const RADIUS_LG: u8 = 14;
pub const PAGE_PAD: i8 = 28;
pub const SIDEBAR_WIDTH: f32 = 232.0;

// ---------------------------------------------------------------------------
// Typography
// ---------------------------------------------------------------------------

pub const SEMIBOLD: &str = "semibold";
pub const ICONS: &str = "phosphor";

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// Candidate system fonts per platform: (regular, semibold, monospace).
fn system_font_candidates() -> (Vec<String>, Vec<String>, Vec<String>) {
    #[cfg(windows)]
    {
        let dir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into()) + r"\Fonts\";
        (
            vec![dir.clone() + "segoeui.ttf"],
            vec![dir.clone() + "seguisb.ttf", dir.clone() + "segoeuib.ttf"],
            vec![dir.clone() + "CascadiaMono.ttf", dir + "consola.ttf"],
        )
    }
    #[cfg(target_os = "macos")]
    {
        (
            vec!["/System/Library/Fonts/SFNS.ttf".into(), "/System/Library/Fonts/Supplemental/Arial.ttf".into()],
            vec!["/System/Library/Fonts/SFNS.ttf".into(), "/System/Library/Fonts/Supplemental/Arial Bold.ttf".into()],
            vec!["/System/Library/Fonts/SFNSMono.ttf".into(), "/System/Library/Fonts/Menlo.ttc".into()],
        )
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let roots = ["/usr/share/fonts", "/usr/local/share/fonts", "/usr/share/fonts/truetype"];
        let mk = |names: &[&str]| -> Vec<String> {
            let mut v = Vec::new();
            for r in roots {
                for n in names {
                    v.push(format!("{r}/{n}"));
                }
            }
            v
        };
        (
            mk(&[
                "inter/Inter-Regular.ttf",
                "truetype/inter/Inter-Regular.ttf",
                "noto/NotoSans-Regular.ttf",
                "truetype/noto/NotoSans-Regular.ttf",
                "google-noto/NotoSans-Regular.ttf",
                "cantarell/Cantarell-Regular.otf",
                "dejavu/DejaVuSans.ttf",
                "truetype/dejavu/DejaVuSans.ttf",
            ]),
            mk(&[
                "inter/Inter-SemiBold.ttf",
                "truetype/inter/Inter-SemiBold.ttf",
                "noto/NotoSans-SemiBold.ttf",
                "truetype/noto/NotoSans-SemiBold.ttf",
                "google-noto/NotoSans-SemiBold.ttf",
                "noto/NotoSans-Bold.ttf",
                "truetype/noto/NotoSans-Bold.ttf",
                "dejavu/DejaVuSans-Bold.ttf",
                "truetype/dejavu/DejaVuSans-Bold.ttf",
            ]),
            mk(&[
                "jetbrains-mono/JetBrainsMono-Regular.ttf",
                "truetype/noto/NotoSansMono-Regular.ttf",
                "dejavu/DejaVuSansMono.ttf",
                "truetype/dejavu/DejaVuSansMono.ttf",
            ]),
        )
    }
}

fn first_readable(paths: &[String]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let (reg, semi, mono) = system_font_candidates();

    let regular_bytes = first_readable(&reg);
    let semibold_bytes = first_readable(&semi);
    let mono_bytes = first_readable(&mono);

    let defaults_prop = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();

    let mut prop = Vec::new();
    if let Some(b) = regular_bytes {
        fonts.font_data.insert("ui-regular".into(), FontData::from_owned(b).into());
        prop.push("ui-regular".to_string());
    }
    let mut semi_family = Vec::new();
    if let Some(b) = semibold_bytes {
        fonts.font_data.insert("ui-semibold".into(), FontData::from_owned(b).into());
        semi_family.push("ui-semibold".to_string());
    }
    if let Some(b) = mono_bytes {
        fonts.font_data.insert("ui-mono".into(), FontData::from_owned(b).into());
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "ui-mono".into());
    }

    // Icon font: after the text font so it never shadows letters.
    fonts.font_data.insert(ICONS.into(), FontData::from_static(egui_phosphor::Variant::Regular.font_bytes()).into());

    let mut proportional = prop.clone();
    proportional.push(ICONS.into());
    proportional.extend(defaults_prop.iter().cloned());

    let mut semibold = if semi_family.is_empty() { prop } else { semi_family };
    semibold.push(ICONS.into());
    semibold.extend(defaults_prop.iter().cloned());

    fonts.families.insert(FontFamily::Proportional, proportional);
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), semibold);
    fonts.families.entry(FontFamily::Monospace).or_default().push(ICONS.into());

    ctx.set_fonts(fonts);
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

fn visuals_for(p: &Palette) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    let r = CornerRadius::same(RADIUS_SM + 2);

    v.panel_fill = p.bg;
    v.window_fill = p.elevated;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(RADIUS_LG);
    v.window_shadow = Shadow { offset: [0, 12], blur: 40, spread: 0, color: p.shadow };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 20, spread: 0, color: p.shadow };
    v.menu_corner_radius = CornerRadius::same(RADIUS);
    v.extreme_bg_color = if p.dark { hex(0x0E1117) } else { hex(0xFFFFFF) };
    v.text_edit_bg_color = Some(if p.dark { hex(0x10131A) } else { hex(0xFFFFFF) });
    v.faint_bg_color = p.surface;
    v.code_bg_color = p.surface_hover;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.selection.bg_fill = p.tint_strong(p.accent);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.override_text_color = None;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = p.surface_hover;
    w.inactive.weak_bg_fill = p.surface;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = p.surface_hover;
    w.hovered.weak_bg_fill = p.surface_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.surface_hover;
    w.active.weak_bg_fill = p.surface_hover;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open = w.active;
    v
}

pub fn install_style(ctx: &egui::Context) {
    ctx.set_visuals_of(Theme::Dark, visuals_for(&DARK));
    ctx.set_visuals_of(Theme::Light, visuals_for(&LIGHT));
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, semibold(22.0)),
            (TextStyle::Body, regular(14.0)),
            (TextStyle::Button, regular(14.0)),
            (TextStyle::Small, regular(12.0)),
            (TextStyle::Monospace, mono(13.0)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(8.0, 8.0);
        s.button_padding = egui::vec2(12.0, 7.0);
        s.interact_size.y = 32.0;
        s.window_margin = Margin::same(24);
        s.menu_margin = Margin::same(6);
        s.combo_height = 300.0;
        s.scroll = egui::style::ScrollStyle::floating();
        s.scroll.bar_width = 8.0;
        style.interaction.selectable_labels = false;
        style.interaction.tooltip_delay = 0.35;
        style.animation_time = 0.14;
        style.url_in_tooltip = true;
    });
}
