//! Text diff viewer (unified + side-by-side) with hunk / line staging hooks.

use crate::git::ops::{self, DiffOptions, DiffTarget};
use crate::git::{DiffContent, DiffLine, LineKind, TextDiff};
use crate::prefs::Preferences;
use crate::theme::Palette;
use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiffMode {
    /// Work tree changes: can stage / discard.
    Unstaged,
    /// Index changes: can unstage.
    Staged,
    ReadOnly,
}

#[derive(Debug, Clone)]
pub enum DiffAction {
    StageHunk(usize),
    UnstageHunk(usize),
    DiscardHunk(usize),
    StageLines,
    UnstageLines,
    DiscardLines,
    Reload,
    OpenBlame,
    OpenHistory,
}

#[derive(Default)]
pub struct DiffViewer {
    pub target: Option<DiffTarget>,
    pub content: Option<DiffContent>,
    pub selected: BTreeSet<usize>,
    anchor: Option<usize>,
}

impl DiffViewer {
    pub fn clear(&mut self) {
        self.target = None;
        self.content = None;
        self.selected.clear();
        self.anchor = None;
    }

    pub fn load(&mut self, repo: &Path, target: DiffTarget, prefs: &Preferences) {
        let opt = DiffOptions { context: prefs.diff_context, ignore_whitespace: prefs.ignore_whitespace };
        self.content = Some(ops::diff(repo, &target, opt));
        self.target = Some(target);
        self.selected.clear();
        self.anchor = None;
    }

    pub fn reload(&mut self, repo: &Path, prefs: &Preferences) {
        if let Some(t) = self.target.clone() {
            self.load(repo, t, prefs);
        }
    }

    pub fn text(&self) -> Option<&TextDiff> {
        match &self.content {
            Some(DiffContent::Text(t)) => Some(t),
            _ => None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences, mode: DiffMode) -> Option<DiffAction> {
        let mut action = None;
        let Some(target) = &self.target else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Select a file to view changes").color(pal.fg2));
            });
            return None;
        };
        let path = target.path().to_string();

        // Toolbar
        egui::Frame::new().fill(pal.tool_bar).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("📄").size(13.0));
                ui.label(RichText::new(&path).strong());
                if let Some(DiffContent::Text(td)) = &self.content {
                    ui.label(RichText::new(format!("+{}", td.added)).color(Color32::from_rgb(0x3F, 0xA5, 0x4A)));
                    ui.label(RichText::new(format!("-{}", td.deleted)).color(Color32::from_rgb(0xD9, 0x4F, 0x4F)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if mode != DiffMode::Staged && !matches!(target, DiffTarget::Unstaged { untracked: true, .. }) {
                        if ui.small_button("History").on_hover_text("File history").clicked() {
                            action = Some(DiffAction::OpenHistory);
                        }
                        if ui.small_button("Blame").clicked() {
                            action = Some(DiffAction::OpenBlame);
                        }
                    }
                    if ui.selectable_label(prefs.diff_side_by_side, "⊞").on_hover_text("Side-by-side").clicked() {
                        prefs.diff_side_by_side = !prefs.diff_side_by_side;
                    }
                    if ui.selectable_label(prefs.ignore_whitespace, "WS").on_hover_text("Ignore whitespace changes").clicked() {
                        prefs.ignore_whitespace = !prefs.ignore_whitespace;
                        action = Some(DiffAction::Reload);
                    }
                    if ui.small_button("+").on_hover_text("Increase context lines").clicked() {
                        prefs.diff_context += 1;
                        action = Some(DiffAction::Reload);
                    }
                    ui.label(RichText::new(format!("{}", prefs.diff_context)).color(pal.fg2));
                    if ui.small_button("−").on_hover_text("Decrease context lines").clicked() {
                        prefs.diff_context = (prefs.diff_context.saturating_sub(1)).max(crate::consts::DIFF_MIN_CONTEXT_LINES);
                        action = Some(DiffAction::Reload);
                    }
                    if mode != DiffMode::ReadOnly && !self.selected.is_empty() {
                        ui.separator();
                        match mode {
                            DiffMode::Unstaged => {
                                if ui.button("Discard selected").clicked() {
                                    action = Some(DiffAction::DiscardLines);
                                }
                                if ui.button("Stage selected").clicked() {
                                    action = Some(DiffAction::StageLines);
                                }
                            }
                            DiffMode::Staged => {
                                if ui.button("Unstage selected").clicked() {
                                    action = Some(DiffAction::UnstageLines);
                                }
                            }
                            DiffMode::ReadOnly => {}
                        }
                        ui.label(RichText::new(format!("{} lines", self.selected.len())).color(pal.fg2));
                    }
                });
            });
        });

        let content = self.content.clone();
        egui::Frame::new().fill(pal.contents).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            match content {
                None | Some(DiffContent::Empty) => {
                    ui.centered_and_justified(|ui| ui.label(RichText::new("No changes or only EOL changes").color(pal.fg2)));
                }
                Some(DiffContent::Binary) => {
                    ui.centered_and_justified(|ui| ui.label(RichText::new("Binary file — no text diff").color(pal.fg2)));
                }
                Some(DiffContent::ModeOnly(m)) => {
                    ui.centered_and_justified(|ui| ui.label(RichText::new(format!("File mode changed\n{m}")).color(pal.fg2)));
                }
                Some(DiffContent::Submodule { old, new }) => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(30.0);
                        ui.label(RichText::new("Submodule").strong());
                        ui.monospace(format!("- {}", if old.is_empty() { "(none)" } else { &old }));
                        ui.monospace(format!("+ {}", if new.is_empty() { "(none)" } else { &new }));
                    });
                }
                Some(DiffContent::Text(td)) => {
                    let a = if prefs.diff_side_by_side { self.side_by_side(ui, pal, &td) } else { self.unified(ui, pal, &td, mode) };
                    if a.is_some() {
                        action = a;
                    }
                }
            }
        });
        action
    }

    fn line_bg(pal: &Palette, kind: LineKind) -> Option<Color32> {
        match kind {
            LineKind::Added => Some(pal.diff_added),
            LineKind::Deleted => Some(pal.diff_deleted),
            LineKind::Indicator => Some(pal.inline_code),
            LineKind::Normal => None,
        }
    }

    fn unified(&mut self, ui: &mut egui::Ui, pal: &Palette, td: &TextDiff, mode: DiffMode) -> Option<DiffAction> {
        let mut action = None;
        let font = FontId::monospace(12.0);
        let row_h = 18.0;
        let max_no = td.lines.iter().map(|l| l.old_no.max(l.new_no)).max().unwrap_or(0);
        let digits = max_no.to_string().len().max(3) as f32;
        let char_w = ui.fonts(|f| f.glyph_width(&font, '0'));
        let gutter = digits * char_w + 10.0;
        let max_len = td.lines.iter().map(|l| l.content.chars().count()).max().unwrap_or(0) as f32;
        let total_w = (gutter * 2.0 + 20.0 + max_len * char_w + 20.0).max(ui.available_width());
        let tab = "    ";

        egui::ScrollArea::both().auto_shrink([false, false]).id_salt("diff-unified").show_rows(ui, row_h, td.lines.len(), |ui, range| {
            for i in range {
                let l = &td.lines[i];
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(total_w, row_h), Sense::click());
                let p = ui.painter();
                if let Some(bg) = Self::line_bg(pal, l.kind) {
                    p.rect_filled(rect, 0.0, bg);
                }
                if self.selected.contains(&i) {
                    p.rect_filled(rect, 0.0, pal.selection);
                    p.line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(3.0_f32, pal.accent));
                }
                let y = rect.center().y;
                let fg2 = pal.fg2;
                if l.kind == LineKind::Indicator {
                    p.text(Pos2::new(rect.left() + 6.0, y), egui::Align2::LEFT_CENTER, &l.content, font.clone(), fg2);
                    // Hunk buttons
                    if mode != DiffMode::ReadOnly {
                        let bx = ui.clip_rect().right() - 8.0;
                        let labels: &[(&str, DiffAction)] = match mode {
                            DiffMode::Unstaged => &[("Discard hunk", DiffAction::DiscardHunk(l.hunk)), ("Stage hunk", DiffAction::StageHunk(l.hunk))],
                            DiffMode::Staged => &[("Unstage hunk", DiffAction::UnstageHunk(l.hunk))],
                            DiffMode::ReadOnly => &[],
                        };
                        let mut x = bx;
                        for (label, act) in labels {
                            let g = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(11.0), pal.fg1);
                            let r = Rect::from_min_size(Pos2::new(x - g.size().x - 10.0, rect.top() + 1.0), Vec2::new(g.size().x + 10.0, row_h - 2.0));
                            let br = ui.interact(r, ui.id().with(("hunkbtn", i, *label)), Sense::click());
                            let fill = if br.hovered() { pal.flat_button_hover } else { pal.flat_button };
                            ui.painter().rect(r, 3.0, fill, Stroke::new(1.0_f32, pal.border2), egui::StrokeKind::Inside);
                            ui.painter().galley(Pos2::new(r.left() + 5.0, r.center().y - g.size().y / 2.0), g, pal.fg1);
                            if br.clicked() {
                                action = Some(act.clone());
                            }
                            x = r.left() - 6.0;
                        }
                    }
                    continue;
                }
                let no = |n: u32| if n == 0 { String::new() } else { n.to_string() };
                p.text(Pos2::new(rect.left() + gutter - 4.0, y), egui::Align2::RIGHT_CENTER, no(l.old_no), font.clone(), fg2);
                p.text(Pos2::new(rect.left() + gutter * 2.0 - 4.0, y), egui::Align2::RIGHT_CENTER, no(l.new_no), font.clone(), fg2);
                p.line_segment(
                    [Pos2::new(rect.left() + gutter * 2.0 + 2.0, rect.top()), Pos2::new(rect.left() + gutter * 2.0 + 2.0, rect.bottom())],
                    Stroke::new(1.0_f32, pal.border2),
                );
                let sign = match l.kind {
                    LineKind::Added => "+",
                    LineKind::Deleted => "-",
                    _ => " ",
                };
                p.text(Pos2::new(rect.left() + gutter * 2.0 + 6.0, y), egui::Align2::LEFT_CENTER, sign, font.clone(), fg2);
                p.text(Pos2::new(rect.left() + gutter * 2.0 + 18.0, y), egui::Align2::LEFT_CENTER, l.content.replace('\t', tab), font.clone(), pal.fg1);

                if mode != DiffMode::ReadOnly && matches!(l.kind, LineKind::Added | LineKind::Deleted) && resp.clicked() {
                    let shift = ui.input(|i| i.modifiers.shift);
                    let cmd = ui.input(|i| i.modifiers.command);
                    if shift {
                        if let Some(a) = self.anchor {
                            let (s, e) = if a < i { (a, i) } else { (i, a) };
                            for j in s..=e {
                                if matches!(td.lines[j].kind, LineKind::Added | LineKind::Deleted) {
                                    self.selected.insert(j);
                                }
                            }
                        }
                    } else if cmd {
                        if !self.selected.remove(&i) {
                            self.selected.insert(i);
                        }
                        self.anchor = Some(i);
                    } else {
                        if self.selected.len() == 1 && self.selected.contains(&i) {
                            self.selected.clear();
                        } else {
                            self.selected.clear();
                            self.selected.insert(i);
                        }
                        self.anchor = Some(i);
                    }
                }
                resp.context_menu(|ui| {
                    if ui.button("Copy line").clicked() {
                        ui.ctx().copy_text(l.content.clone());
                        ui.close_menu();
                    }
                    if ui.button("Copy all").clicked() {
                        let s: String = td.lines.iter().filter(|l| l.kind != LineKind::Indicator).map(|l| format!("{}\n", l.content)).collect();
                        ui.ctx().copy_text(s);
                        ui.close_menu();
                    }
                });
            }
        });
        action
    }

    fn side_by_side(&mut self, ui: &mut egui::Ui, pal: &Palette, td: &TextDiff) -> Option<DiffAction> {
        // Pair deleted/added blocks into rows.
        let mut rows: Vec<(Option<&DiffLine>, Option<&DiffLine>)> = Vec::new();
        let mut i = 0;
        while i < td.lines.len() {
            let l = &td.lines[i];
            match l.kind {
                LineKind::Deleted | LineKind::Added => {
                    let mut dels = Vec::new();
                    let mut adds = Vec::new();
                    while i < td.lines.len() && td.lines[i].kind == LineKind::Deleted {
                        dels.push(&td.lines[i]);
                        i += 1;
                    }
                    while i < td.lines.len() && td.lines[i].kind == LineKind::Added {
                        adds.push(&td.lines[i]);
                        i += 1;
                    }
                    let n = dels.len().max(adds.len());
                    for k in 0..n {
                        rows.push((dels.get(k).copied(), adds.get(k).copied()));
                    }
                }
                _ => {
                    rows.push((Some(l), Some(l)));
                    i += 1;
                }
            }
        }
        let font = FontId::monospace(12.0);
        let row_h = 18.0;
        let char_w = ui.fonts(|f| f.glyph_width(&font, '0'));
        let max_no = td.lines.iter().map(|l| l.old_no.max(l.new_no)).max().unwrap_or(0);
        let gutter = (max_no.to_string().len().max(3) as f32) * char_w + 10.0;
        let width = ui.available_width();
        let half = width / 2.0;
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("diff-sbs").show_rows(ui, row_h, rows.len(), |ui, range| {
            for r in range {
                let (left, right) = rows[r];
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::hover());
                let p = ui.painter().clone();
                for (side, line) in [(0, left), (1, right)] {
                    let x0 = rect.left() + half * side as f32;
                    let cell = Rect::from_min_size(Pos2::new(x0, rect.top()), Vec2::new(half, row_h));
                    let pc = p.with_clip_rect(cell.intersect(ui.clip_rect()));
                    match line {
                        None => {
                            pc.rect_filled(cell, 0.0, pal.diff_empty);
                        }
                        Some(l) => {
                            if let Some(bg) = Self::line_bg(pal, l.kind) {
                                pc.rect_filled(cell, 0.0, bg);
                            }
                            let y = cell.center().y;
                            if l.kind == LineKind::Indicator {
                                pc.text(Pos2::new(x0 + 6.0, y), egui::Align2::LEFT_CENTER, &l.content, font.clone(), pal.fg2);
                                continue;
                            }
                            let no = if side == 0 { l.old_no } else { l.new_no };
                            if no > 0 {
                                pc.text(Pos2::new(x0 + gutter - 4.0, y), egui::Align2::RIGHT_CENTER, no.to_string(), font.clone(), pal.fg2);
                            }
                            pc.text(Pos2::new(x0 + gutter + 6.0, y), egui::Align2::LEFT_CENTER, l.content.replace('\t', "    "), font.clone(), pal.fg1);
                        }
                    }
                }
                p.line_segment([Pos2::new(rect.left() + half, rect.top()), Pos2::new(rect.left() + half, rect.bottom())], Stroke::new(1.0_f32, pal.border2));
            }
        });
        None
    }
}

/// Plain text/file viewer with line numbers.
pub fn show_text(ui: &mut egui::Ui, pal: &Palette, text: &str, salt: &str) {
    let lines: Vec<&str> = text.lines().collect();
    let font = FontId::monospace(12.0);
    let row_h = 18.0;
    let char_w = ui.fonts(|f| f.glyph_width(&font, '0'));
    let gutter = (lines.len().to_string().len().max(3) as f32) * char_w + 12.0;
    let max_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
    let total_w = (gutter + 12.0 + max_len * char_w + 20.0).max(ui.available_width());
    egui::Frame::new().fill(pal.contents).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        egui::ScrollArea::both().auto_shrink([false, false]).id_salt(salt).show_rows(ui, row_h, lines.len(), |ui, range| {
            for i in range {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, row_h), Sense::hover());
                let p = ui.painter();
                let y = rect.center().y;
                p.text(Pos2::new(rect.left() + gutter - 6.0, y), egui::Align2::RIGHT_CENTER, (i + 1).to_string(), font.clone(), pal.fg2);
                p.text(Pos2::new(rect.left() + gutter + 6.0, y), egui::Align2::LEFT_CENTER, lines[i].replace('\t', "    "), font.clone(), pal.fg1);
            }
        });
    });
}
