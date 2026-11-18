//! Palettes ported from src/Resources/Themes.axaml.
use eframe::egui::{self, Color32};

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

/// Full palette from Themes.axaml; a few entries are kept for completeness.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub window: Color32,
    pub title_bar: Color32,
    pub tool_bar: Color32,
    pub popup: Color32,
    pub contents: Color32,
    pub badge: Color32,
    pub badge_fg: Color32,
    pub conflict: Color32,
    pub conflict_fg: Color32,
    pub border0: Color32,
    pub border1: Color32,
    pub border2: Color32,
    pub flat_button: Color32,
    pub flat_button_hover: Color32,
    pub fg1: Color32,
    pub fg2: Color32,
    pub diff_empty: Color32,
    pub diff_added: Color32,
    pub diff_deleted: Color32,
    pub diff_added_hl: Color32,
    pub diff_deleted_hl: Color32,
    pub link: Color32,
    pub inline_code: Color32,
    pub sub_panel: Color32,
    pub accent: Color32,
    pub selection: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}
const fn ahex(argb: u32) -> Color32 {
    let a = (argb >> 24) & 0xFF;
    let r = (((argb >> 16) & 0xFF) * a / 255) as u8;
    let g = (((argb >> 8) & 0xFF) * a / 255) as u8;
    let b = ((argb & 0xFF) * a / 255) as u8;
    Color32::from_rgba_premultiplied(r, g, b, a as u8)
}

pub fn palette(mode: ThemeMode) -> Palette {
    match mode {
        ThemeMode::Light => Palette {
            window: hex(0xF4F4F4),
            title_bar: hex(0xF2F0F0),
            tool_bar: hex(0xF0F3F5),
            popup: hex(0xFAFAFA),
            contents: hex(0xFFFFFF),
            badge: hex(0xB0CEE8),
            badge_fg: hex(0x1F1F1F),
            conflict: hex(0x836C2E),
            conflict_fg: hex(0xFFFFFF),
            border0: hex(0xCFCFCF),
            border1: hex(0x898989),
            border2: hex(0xCFCFCF),
            flat_button: hex(0xF8F8F8),
            flat_button_hover: hex(0xFFFFFF),
            fg1: hex(0x1F1F1F),
            fg2: hex(0x6F6F6F),
            diff_empty: ahex(0x10000000),
            diff_added: ahex(0x80BFE6C1),
            diff_deleted: ahex(0x80FF9797),
            diff_added_hl: hex(0xA7E1A7),
            diff_deleted_hl: hex(0xF19B9D),
            link: hex(0x0000EE),
            inline_code: hex(0xE4E4E4),
            sub_panel: hex(0xFBFBFB),
            accent: hex(0x1E88E5),
            selection: ahex(0x401E88E5),
        },
        ThemeMode::Dark => Palette {
            window: hex(0x252525),
            title_bar: hex(0x1F1F1F),
            tool_bar: hex(0x2F2F2F),
            popup: hex(0x2B2B2B),
            contents: hex(0x1C1C1C),
            badge: hex(0x8F8F8F),
            badge_fg: hex(0xE0E0E0),
            conflict: hex(0xFAFAD2),
            conflict_fg: hex(0x252525),
            border0: hex(0x181818),
            border1: hex(0x7C7C7C),
            border2: hex(0x404040),
            flat_button: hex(0x303030),
            flat_button_hover: hex(0x333333),
            fg1: hex(0xDFDFDF),
            fg2: hex(0x9F9F9F),
            diff_empty: ahex(0x3C000000),
            diff_added: ahex(0xC03A5C3F),
            diff_deleted: ahex(0xC0633F3E),
            diff_added_hl: ahex(0xA0308D3C),
            diff_deleted_hl: ahex(0xA09F4247),
            link: hex(0x4DAAFC),
            inline_code: hex(0x383838),
            sub_panel: hex(0x272727),
            accent: hex(0x4DAAFC),
            selection: ahex(0x504DAAFC),
        },
    }
}

/// Graph pens: Orange, ForestGreen, Turquoise, Olive, Magenta, Red, Khaki, Lime, RoyalBlue, Teal.
pub const GRAPH_COLORS: [Color32; crate::consts::GRAPH_PEN_COUNT] = [
    hex(0xFFA500),
    hex(0x228B22),
    hex(0x40E0D0),
    hex(0x808000),
    hex(0xFF00FF),
    hex(0xFF0000),
    hex(0xF0E68C),
    hex(0x00FF00),
    hex(0x4169E1),
    hex(0x008080),
];

pub fn apply(ctx: &egui::Context, mode: ThemeMode) {
    let p = palette(mode);
    let mut v = match mode {
        ThemeMode::Dark => egui::Visuals::dark(),
        ThemeMode::Light => egui::Visuals::light(),
    };
    v.panel_fill = p.window;
    v.window_fill = p.popup;
    v.extreme_bg_color = p.contents;
    v.faint_bg_color = p.sub_panel;
    v.override_text_color = Some(p.fg1);
    v.hyperlink_color = p.link;
    v.selection.bg_fill = p.selection;
    v.selection.stroke = egui::Stroke::new(1.0_f32, p.accent);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, p.border2);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, p.fg1);
    v.widgets.inactive.bg_fill = p.flat_button;
    v.widgets.inactive.weak_bg_fill = p.flat_button;
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, p.fg1);
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, p.border2);
    v.widgets.hovered.bg_fill = p.flat_button_hover;
    v.widgets.hovered.weak_bg_fill = p.flat_button_hover;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, p.border1);
    v.window_stroke = egui::Stroke::new(1.0_f32, p.border2);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
    });
}
