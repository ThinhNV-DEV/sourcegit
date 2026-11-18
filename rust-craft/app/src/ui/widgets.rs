use crate::git::ChangeState;
use crate::theme::Palette;
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, Vec2};

pub fn rel_time(ts: i64, relative: bool) -> String {
    use chrono::TimeZone;
    let Some(dt) = chrono::Local.timestamp_opt(ts, 0).single() else { return String::new() };
    if !relative {
        return dt.format("%Y-%m-%d %H:%M:%S").to_string();
    }
    let now = chrono::Local::now();
    let secs = (now - dt).num_seconds();
    if secs < 60 {
        return "Just now".into();
    }
    let mins = secs / 60;
    if mins < 60 {
        return plural(mins, "minute");
    }
    let hours = mins / 60;
    if hours < 24 {
        return plural(hours, "hour");
    }
    let days = hours / 24;
    if days < 30 {
        return if days == 1 { "Yesterday".into() } else { plural(days, "day") };
    }
    let months = days / 30;
    if months < 12 {
        return plural(months, "month");
    }
    plural(days / 365, "year")
}

fn plural(n: i64, unit: &str) -> String {
    if n == 1 { format!("1 {unit} ago") } else { format!("{n} {unit}s ago") }
}

pub fn full_time(ts: i64) -> String {
    rel_time(ts, false)
}

pub fn change_color(state: ChangeState) -> Color32 {
    match state {
        ChangeState::Modified | ChangeState::TypeChanged => Color32::from_rgb(0xE0, 0x9B, 0x00),
        ChangeState::Added => Color32::from_rgb(0x3F, 0xA5, 0x4A),
        ChangeState::Deleted => Color32::from_rgb(0xD9, 0x4F, 0x4F),
        ChangeState::Renamed | ChangeState::Copied => Color32::from_rgb(0x9A, 0x6D, 0xD7),
        ChangeState::Untracked => Color32::from_rgb(0x6A, 0x9F, 0x6A),
        ChangeState::Conflicted => Color32::from_rgb(0xFF, 0x57, 0x22),
        ChangeState::None => Color32::GRAY,
    }
}

/// Small colored square with the change letter (like SourceGit's ChangeStatusIcon).
pub fn change_icon(ui: &mut Ui, state: ChangeState) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(2), change_color(state));
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, state.letter(), FontId::monospace(10.0), Color32::WHITE);
    resp
}

pub fn avatar(ui: &mut Ui, name: &str, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_avatar(ui.painter(), rect, name);
    resp
}

pub fn paint_avatar(p: &egui::Painter, rect: Rect, name: &str) {
    let mut h: u32 = 2166136261;
    for b in name.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    let hue = (h % 360) as f32 / 360.0;
    let c: Color32 = egui::ecolor::Hsva::new(hue, 0.45, 0.65, 1.0).into();
    p.rect_filled(rect, CornerRadius::same((rect.width() * 0.2) as u8), c);
    let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    p.text(rect.center(), egui::Align2::CENTER_CENTER, initial, FontId::proportional(rect.height() * 0.6), Color32::WHITE);
}

/// Paints a rounded badge and returns its width.
pub fn paint_badge(p: &egui::Painter, pos: Pos2, h: f32, text: &str, bg: Color32, fg: Color32, border: Color32, bold: bool) -> f32 {
    let font = FontId::proportional(11.5);
    let galley = p.layout_no_wrap(text.to_string(), font, fg);
    let w = galley.size().x + 10.0;
    let rect = Rect::from_min_size(Pos2::new(pos.x, pos.y + (h - 16.0) / 2.0), Vec2::new(w, 16.0));
    p.rect(rect, CornerRadius::same(3), bg, Stroke::new(1.0_f32, border), egui::StrokeKind::Inside);
    let tp = Pos2::new(rect.left() + 5.0, rect.center().y - galley.size().y / 2.0);
    p.galley(tp, galley.clone(), fg);
    if bold {
        p.galley(tp + Vec2::new(0.5, 0.0), galley, fg);
    }
    w
}

pub fn badge(ui: &mut Ui, text: &str, bg: Color32, fg: Color32) -> Response {
    let font = FontId::proportional(11.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, fg);
    let size = Vec2::new(galley.size().x + 10.0, 18.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    ui.painter().rect_filled(rect, CornerRadius::same(3), bg);
    ui.painter().galley(Pos2::new(rect.left() + 5.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    resp
}

pub fn section_label(ui: &mut Ui, pal: &Palette, text: &str) {
    ui.label(RichText::new(text).strong().size(12.0).color(pal.fg2));
}

pub fn tool_button(ui: &mut Ui, glyph: &str, tip: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(glyph).size(16.0)).frame(false).min_size(Vec2::new(28.0, 26.0))).on_hover_text(tip)
}

pub fn small_button(ui: &mut Ui, glyph: &str, tip: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(glyph).size(13.0)).frame(false).min_size(Vec2::new(20.0, 18.0))).on_hover_text(tip)
}

/// A full-width selectable row; returns the response.
pub fn row(ui: &mut Ui, selected: bool, height: f32, pal: &Palette, add: impl FnOnce(&mut Ui)) -> Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, pal.selection);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, pal.flat_button_hover.gamma_multiply(0.8));
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(4.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    add(&mut child);
    resp
}

pub fn truncate_middle(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1) / 2;
    let start: String = s.chars().take(keep).collect();
    let end: String = s.chars().skip(n - keep).collect();
    format!("{start}…{end}")
}

pub fn copy(ui: &Ui, text: &str) {
    ui.ctx().copy_text(text.to_string());
}

pub fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let cmd = "xdg-open";
    let _ = std::process::Command::new(cmd).arg(path).spawn();
}

pub fn open_terminal(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").args(["-a", "Terminal"]).arg(path).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd").args(["/c", "start", "cmd"]).current_dir(path).spawn();
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let term = std::env::var("TERMINAL").unwrap_or_else(|_| "x-terminal-emulator".into());
        let _ = std::process::Command::new(term).current_dir(path).spawn();
    }
}

pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let cmd = "xdg-open";
    let _ = std::process::Command::new(cmd).arg(url).spawn();
}
