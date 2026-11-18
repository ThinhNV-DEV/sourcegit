//! History page: commit graph table + commit detail (Information / Changes / Files).

use super::diff_view::{self, DiffAction, DiffMode};
use super::popups::{Job, Popup};
use super::repo_view::{Action, DetailTab, RepoView};
use super::widgets;
use crate::consts::*;
use crate::git::{ops, parse, Commit, DecoratorKind};
use crate::graph::DotKind;
use crate::prefs::Preferences;
use crate::repo::HistoryScope;
use crate::theme::{Palette, GRAPH_COLORS};
use eframe::egui::{self, text::LayoutJob, Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, TextFormat, Vec2};

const AUTHOR_W: f32 = 150.0;
const SHA_W: f32 = 100.0;
const TIME_W: f32 = 140.0;
/// src/Views/CommitGraph.cs:187-196
const DOT_RADIUS_LARGE: f32 = 6.0;
const DOT_RADIUS_SMALL: f32 = 3.0;

/// Builds a rich subject: bold conventional-commit prefix, `inline code`, #issue links.
pub fn subject_job(subject: &str, pal: &Palette, font: FontId, dim: bool) -> LayoutJob {
    let fg = if dim { pal.fg2 } else { pal.fg1 };
    let mut job = LayoutJob::default();
    let normal = TextFormat { font_id: font.clone(), color: fg, ..Default::default() };
    let mut rest = subject;
    // Conventional commit prefix: type(scope)!:
    if let Some(colon) = subject.find(": ") {
        let head = &subject[..colon];
        let ty = head.split('(').next().unwrap_or("").trim_end_matches('!');
        if !ty.is_empty() && ty.len() <= 12 && ty.chars().all(|c| c.is_ascii_alphabetic()) && (head.len() == ty.len() || head.ends_with(')') || head.ends_with('!')) {
            let strong = TextFormat { font_id: font.clone(), color: fg, ..Default::default() };
            job.append(&subject[..=colon], 0.0, strong);
            rest = &subject[colon + 1..];
            if let Some(sec) = job.sections.last_mut() {
                let strong = if pal.contents.r() < 128 { Color32::WHITE } else { Color32::BLACK };
                sec.format.color = if dim { pal.fg2 } else { strong };
            }
        }
    }
    let code = TextFormat { font_id: FontId::monospace(font.size - 1.0), color: fg, background: pal.inline_code, ..Default::default() };
    let link = TextFormat { font_id: font.clone(), color: pal.link, ..Default::default() };
    let mut buf = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '`' {
            let mut inner = String::new();
            let mut closed = false;
            for d in chars.by_ref() {
                if d == '`' {
                    closed = true;
                    break;
                }
                inner.push(d);
            }
            if closed && !inner.is_empty() {
                job.append(&std::mem::take(&mut buf), 0.0, normal.clone());
                job.append(&format!(" {inner} "), 0.0, code.clone());
            } else {
                buf.push('`');
                buf.push_str(&inner);
            }
        } else if c == '#' && chars.peek().map(|d| d.is_ascii_digit()).unwrap_or(false) {
            job.append(&std::mem::take(&mut buf), 0.0, normal.clone());
            let mut num = String::from("#");
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    num.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            job.append(&num, 0.0, link.clone());
        } else {
            buf.push(c);
        }
    }
    job.append(&buf, 0.0, normal);
    job
}

impl RepoView {
    pub fn histories(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences) {
        if self.search.open {
            self.search_bar(ui, pal);
        }
        let total_h = ui.available_height();
        egui::TopBottomPanel::bottom("commit-detail")
            .resizable(true)
            .default_height(total_h * 0.45)
            .height_range(80.0..=total_h - 80.0)
            .frame(egui::Frame::new().fill(pal.window))
            .show_inside(ui, |ui| self.commit_detail(ui, pal, prefs));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.contents)).show_inside(ui, |ui| {
            if self.search.open && self.search.searched {
                self.search_results(ui, pal, prefs);
            } else {
                self.commit_table(ui, pal, prefs);
            }
        });
    }

    fn search_bar(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        egui::Frame::new().fill(pal.tool_bar).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("search-method").selected_text(self.search.method.label()).show_ui(ui, |ui| {
                    for m in parse::SearchMethod::ALL {
                        ui.selectable_value(&mut self.search.method, m, m.label());
                    }
                });
                let r = ui.add(egui::TextEdit::singleline(&mut self.search.text).hint_text("Search commits…").desired_width(320.0));
                ui.checkbox(&mut self.search.only_current, "Current branch only");
                let go = ui.button("Search").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if go && !self.search.text.trim().is_empty() {
                    self.search.results = parse::search_commits(&self.repo.path, self.search.method, self.search.text.trim(), self.search.only_current);
                    self.search.searched = true;
                }
                if self.search.searched {
                    ui.label(RichText::new(format!("{} result(s)", self.search.results.len())).color(pal.fg2));
                    if ui.small_button("Clear").clicked() {
                        self.search.searched = false;
                        self.search.results.clear();
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✖").clicked() {
                        self.search.open = false;
                        self.search.searched = false;
                    }
                });
            });
        });
    }

    fn search_results(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &Preferences) {
        let results = self.search.results.clone();
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("search-results").show_rows(ui, HISTORY_ROW_HEIGHT, results.len(), |ui, range| {
            for i in range {
                let c = &results[i];
                let sel = self.detail.as_ref().map(|d| d.commit.sha == c.sha).unwrap_or(false);
                let resp = widgets::row(ui, sel, HISTORY_ROW_HEIGHT, pal, |ui| {
                    ui.label(RichText::new(c.short_sha()).monospace().color(pal.fg2));
                    ui.label(subject_job(&c.subject, pal, FontId::proportional(13.0), false));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(widgets::rel_time(c.committer_time, prefs.relative_time)).color(pal.fg2));
                        ui.label(RichText::new(&c.author.name).color(pal.fg2));
                    });
                });
                if resp.clicked() {
                    self.actions.push(Action::SelectCommit(c.sha.clone()));
                    self.search.searched = false;
                }
            }
        });
    }

    fn commit_table(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences) {
        // Header
        let (hrect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), HISTORY_HEADER_HEIGHT), Sense::hover());
        {
            let p = ui.painter();
            p.rect_filled(hrect, 0.0, pal.window);
            p.line_segment([hrect.left_bottom(), hrect.right_bottom()], Stroke::new(1.0_f32, pal.border2));
            let f = FontId::proportional(12.0);
            let c = pal.fg2;
            let right = hrect.right() - 14.0;
            p.text(Pos2::new(hrect.left() + (right - TIME_W - SHA_W - AUTHOR_W - hrect.left()) / 2.0, hrect.center().y), egui::Align2::CENTER_CENTER, "GRAPH & SUBJECT", f.clone(), c);
            p.text(Pos2::new(right - TIME_W - SHA_W - AUTHOR_W / 2.0, hrect.center().y), egui::Align2::CENTER_CENTER, "AUTHOR", f.clone(), c);
            p.text(Pos2::new(right - TIME_W - SHA_W / 2.0, hrect.center().y), egui::Align2::CENTER_CENTER, "SHA", f.clone(), c);
            p.text(Pos2::new(right - TIME_W / 2.0, hrect.center().y), egui::Align2::CENTER_CENTER, "COMMIT TIME", f, c);
        }
        // Scope toggle in header-left
        {
            let r = Rect::from_min_size(hrect.left_top() + Vec2::new(4.0, 2.0), Vec2::new(20.0, 20.0));
            let resp = ui.interact(r, ui.id().with("scope-btn"), Sense::click()).on_hover_text("History filter");
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "☰", FontId::proportional(13.0), if resp.hovered() { pal.fg1 } else { pal.fg2 });
            let popup_id = ui.id().with("scope-popup");
            if resp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup_id));
            }
            egui::popup::popup_below_widget(ui, popup_id, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(200.0);
                let all = self.repo.scope == HistoryScope::All;
                if ui.radio(all, "All branches").clicked() {
                    self.actions.push(Action::SetScope(HistoryScope::All));
                    ui.close_menu();
                }
                if ui.radio(self.repo.scope == HistoryScope::CurrentBranch, "Current branch only").clicked() {
                    self.actions.push(Action::SetScope(HistoryScope::CurrentBranch));
                    ui.close_menu();
                }
                ui.separator();
                if ui.checkbox(&mut prefs.first_parent_only, "First-parent only").changed() {
                    self.actions.push(Action::SetScope(self.repo.scope.clone()));
                }
                if ui.checkbox(&mut prefs.topo_order, "Topological order").changed() {
                    self.actions.push(Action::SetScope(self.repo.scope.clone()));
                }
                ui.checkbox(&mut prefs.relative_time, "Relative time");
            });
        }

        let commits_len = self.repo.data.commits.len();
        if commits_len == 0 {
            ui.centered_and_justified(|ui| ui.label(RichText::new("No commits yet").color(pal.fg2)));
            return;
        }

        let row_h = HISTORY_ROW_HEIGHT;
        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("history-list");
        if let Some(i) = self.scroll_to.take() {
            // Center the requested row.
            let y = (i as f32 * row_h - ui.available_height() / 2.0).max(0.0);
            scroll = scroll.vertical_scroll_offset(y);
        }
        let mut clicked: Option<(usize, egui::Modifiers)> = None;
        let mut double: Option<usize> = None;
        let mut menu_for: Option<usize> = None;
        let keyboard_nav = ui.input(|i| (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp)));

        scroll.show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            let (full, _) = ui.allocate_exact_size(Vec2::new(width, row_h * commits_len as f32), Sense::hover());
            let top = full.top();
            let first = ((viewport.min.y / row_h).floor().max(0.0)) as usize;
            let last = (((viewport.max.y / row_h).ceil()) as usize).min(commits_len);
            let painter = ui.painter_at(Rect::from_min_max(Pos2::new(full.left(), top + viewport.min.y), Pos2::new(full.right(), top + viewport.max.y)));
            let right = full.right() - 14.0;
            let subject_right = right - TIME_W - SHA_W - AUTHOR_W - 8.0;

            // Row backgrounds + interaction
            for i in first..last {
                let rect = Rect::from_min_size(Pos2::new(full.left(), top + i as f32 * row_h), Vec2::new(width, row_h));
                let resp = ui.interact(rect, ui.id().with(("commit", i)), Sense::click());
                let selected = self.selected.contains(&i);
                if selected {
                    painter.rect_filled(rect, 0.0, pal.selection);
                } else if resp.hovered() {
                    painter.rect_filled(rect, 0.0, pal.flat_button_hover.gamma_multiply(0.6));
                }
                if resp.clicked() {
                    clicked = Some((i, ui.input(|x| x.modifiers)));
                }
                if resp.double_clicked() {
                    double = Some(i);
                }
                if resp.secondary_clicked() {
                    if !self.selected.contains(&i) {
                        clicked = Some((i, egui::Modifiers::NONE));
                    }
                    menu_for = Some(i);
                }
            }

            // Graph (clipped to subject column)
            let gp = painter.with_clip_rect(painter.clip_rect().intersect(Rect::from_min_max(Pos2::new(full.left(), f32::MIN), Pos2::new(subject_right, f32::MAX))));
            let g = &self.repo.data.graph;
            let y0 = first as f32 - 1.0;
            let y1 = last as f32 + 1.0;
            let ox = full.left();
            let to_px = |x: f32, y: f32| Pos2::new(ox + x, top + y * row_h);
            for path in &g.paths {
                if path.max_y < y0 || path.min_y > y1 {
                    continue;
                }
                let stroke = Stroke::new(GRAPH_PEN_THICKNESS, GRAPH_COLORS[path.color % GRAPH_PEN_COUNT]);
                let n = path.points.len();
                for k in 1..n {
                    let a = path.points[k - 1];
                    let b = path.points[k];
                    if b.y < y0 || a.y > y1 {
                        continue;
                    }
                    let pa = to_px(a.x, a.y);
                    let pb = to_px(b.x, b.y);
                    if (a.x - b.x).abs() < 0.01 {
                        gp.line_segment([pa, pb], stroke);
                    } else if b.x > a.x {
                        gp.add(Shape::QuadraticBezier(egui::epaint::QuadraticBezierShape::from_points_stroke([pa, Pos2::new(pb.x, pa.y), pb], false, Color32::TRANSPARENT, stroke)));
                    } else if k < n - 1 {
                        let mid = (pa.y + pb.y) / 2.0;
                        gp.add(Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(
                            [pa, Pos2::new(pa.x, mid + 4.0), Pos2::new(pb.x, mid - 4.0), pb],
                            false,
                            Color32::TRANSPARENT,
                            stroke,
                        )));
                    } else {
                        gp.add(Shape::QuadraticBezier(egui::epaint::QuadraticBezierShape::from_points_stroke([pa, Pos2::new(pa.x, pb.y), pb], false, Color32::TRANSPARENT, stroke)));
                    }
                }
            }
            for l in &g.links {
                if l.end.y < y0 || l.start.y > y1 {
                    continue;
                }
                let stroke = Stroke::new(GRAPH_PEN_THICKNESS, GRAPH_COLORS[l.color % GRAPH_PEN_COUNT]);
                gp.add(Shape::QuadraticBezier(egui::epaint::QuadraticBezierShape::from_points_stroke(
                    [to_px(l.start.x, l.start.y), to_px(l.control.x, l.control.y), to_px(l.end.x, l.end.y)],
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                )));
            }
            for i in first..last {
                let Some(d) = g.dots.get(i) else { continue };
                let c = to_px(d.center.x, d.center.y);
                let color = GRAPH_COLORS[d.color % GRAPH_PEN_COUNT];
                match d.kind {
                    DotKind::Head => {
                        gp.circle(c, DOT_RADIUS_LARGE, pal.contents, Stroke::new(GRAPH_PEN_THICKNESS, color));
                        gp.circle_filled(c, DOT_RADIUS_SMALL, color);
                    }
                    DotKind::Merge => {
                        gp.circle_filled(c, DOT_RADIUS_LARGE, color);
                        gp.line_segment([c - Vec2::new(0.0, 3.0), c + Vec2::new(0.0, 3.0)], Stroke::new(2.0_f32, pal.contents));
                        gp.line_segment([c - Vec2::new(3.0, 0.0), c + Vec2::new(3.0, 0.0)], Stroke::new(2.0_f32, pal.contents));
                    }
                    DotKind::Default => {
                        gp.circle(c, DOT_RADIUS_SMALL, pal.contents, Stroke::new(GRAPH_PEN_THICKNESS, color));
                    }
                }
            }

            // Text columns
            let font = FontId::proportional(13.0);
            for i in first..last {
                let c = &self.repo.data.commits[i];
                let y = top + i as f32 * row_h;
                let cy = y + row_h / 2.0;
                let dim = false;
                let mut x = full.left() + c.left_margin;
                // Decorators
                let sp = painter.with_clip_rect(painter.clip_rect().intersect(Rect::from_min_max(Pos2::new(full.left(), y), Pos2::new(subject_right, y + row_h))));
                for d in &c.decorators {
                    if d.kind == DecoratorKind::Tag && !prefs.show_tags_in_graph {
                        continue;
                    }
                    let lane = GRAPH_COLORS[c.color % GRAPH_PEN_COUNT];
                    let (label, border, bold) = match d.kind {
                        DecoratorKind::CurrentBranchHead => (format!("✔ {}", d.name), lane, true),
                        DecoratorKind::LocalBranchHead => (format!("🔀 {}", d.name), lane, false),
                        DecoratorKind::CurrentCommitHead => ("HEAD".to_string(), lane, true),
                        DecoratorKind::RemoteBranchHead => (format!("☁ {}", d.name), lane, false),
                        DecoratorKind::Tag => (format!("🏷 {}", d.name), pal.fg2, false),
                    };
                    let w = widgets::paint_badge(&sp, Pos2::new(x, y), row_h, &label, pal.sub_panel, pal.fg1, border, bold);
                    x += w + 4.0;
                }
                let galley = ui.fonts(|f| f.layout_job(subject_job(&c.subject, pal, font.clone(), dim)));
                sp.galley(Pos2::new(x + 2.0, cy - galley.size().y / 2.0), galley, pal.fg1);

                // Author
                let ax = right - TIME_W - SHA_W - AUTHOR_W;
                let ap = painter.with_clip_rect(painter.clip_rect().intersect(Rect::from_min_max(Pos2::new(ax, y), Pos2::new(ax + AUTHOR_W - 6.0, y + row_h))));
                widgets::paint_avatar(&ap, Rect::from_center_size(Pos2::new(ax + 10.0, cy), Vec2::splat(16.0)), &c.author.name);
                ap.text(Pos2::new(ax + 24.0, cy), egui::Align2::LEFT_CENTER, &c.author.name, font.clone(), pal.fg1);
                painter.text(Pos2::new(right - TIME_W - SHA_W + 6.0, cy), egui::Align2::LEFT_CENTER, c.short_sha(), FontId::monospace(12.5), pal.fg1);
                painter.text(Pos2::new(right - 4.0, cy), egui::Align2::RIGHT_CENTER, widgets::rel_time(c.committer_time, prefs.relative_time), font.clone(), pal.fg1);
            }
        });

        // Keyboard navigation
        if ui.memory(|m| m.focused().is_none()) {
            if let Some(&cur) = self.selected.last() {
                if keyboard_nav.0 && cur + 1 < commits_len {
                    clicked = Some((cur + 1, egui::Modifiers::NONE));
                    self.scroll_to = None;
                } else if keyboard_nav.1 && cur > 0 {
                    clicked = Some((cur - 1, egui::Modifiers::NONE));
                }
            }
        }

        if let Some((i, m)) = clicked {
            if m.command {
                if let Some(pos) = self.selected.iter().position(|&x| x == i) {
                    self.selected.remove(pos);
                } else {
                    self.selected.push(i);
                }
                self.anchor = Some(i);
            } else if m.shift {
                let a = self.anchor.unwrap_or(i);
                let (s, e) = if a < i { (a, i) } else { (i, a) };
                self.selected = (s..=e).collect();
            } else {
                self.select_commit(i, prefs);
            }
            if self.selected.len() == 1 && self.detail.as_ref().map(|d| d.commit.sha != self.repo.data.commits[self.selected[0]].sha).unwrap_or(true) {
                let idx = self.selected[0];
                self.load_detail(idx, prefs);
            }
        }
        if let Some(i) = double {
            self.double_click_commit(i);
        }
        if let Some(i) = menu_for {
            ui.memory_mut(|m| m.data.insert_temp(egui::Id::new("commit-menu"), i));
        }
        // Context menu (rendered as a popup anchored at pointer)
        let menu_id = egui::Id::new("commit-menu");
        if let Some(i) = ui.memory(|m| m.data.get_temp::<usize>(menu_id)) {
            let pos = ui.memory(|m| m.data.get_temp::<Pos2>(menu_id.with("pos")));
            let pos = match pos {
                Some(p) if menu_for.is_none() => p,
                _ => {
                    let p = ui.input(|x| x.pointer.interact_pos()).unwrap_or_default();
                    ui.memory_mut(|m| m.data.insert_temp(menu_id.with("pos"), p));
                    p
                }
            };
            let mut close = false;
            let area = egui::Area::new(menu_id.with("area")).order(egui::Order::Foreground).fixed_pos(pos).show(ui.ctx(), |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(260.0);
                    close = self.commit_menu(ui, i);
                });
            });
            let clicked_outside = ui.input(|x| x.pointer.any_pressed()) && menu_for.is_none() && !area.response.contains_pointer();
            if close || clicked_outside || ui.input(|x| x.key_pressed(egui::Key::Escape)) {
                ui.memory_mut(|m| m.data.remove::<usize>(menu_id));
            }
        }
    }

    fn double_click_commit(&mut self, i: usize) {
        let c = &self.repo.data.commits[i];
        if let Some(d) = c.decorators.iter().find(|d| d.kind == DecoratorKind::LocalBranchHead) {
            let n = d.name.clone();
            self.actions.push(Action::Job(Job::git(format!("Checkout {n}"), move |p| ops::checkout(p, &n))));
        } else if let Some(d) = c.decorators.iter().find(|d| d.kind == DecoratorKind::RemoteBranchHead) {
            if let Some(b) = self.repo.data.branches.iter().find(|b| !b.is_local && b.friendly_name() == d.name).cloned() {
                self.checkout_remote(&b);
            }
        }
    }

    /// Returns true when the menu should close.
    fn commit_menu(&mut self, ui: &mut egui::Ui, i: usize) -> bool {
        let mut close = false;
        let Some(c) = self.repo.data.commits.get(i).cloned() else { return true };
        let cur = self.repo.current_branch().cloned();
        let cur_name = cur.as_ref().map(|b| b.name.clone()).unwrap_or_else(|| "HEAD".into());
        let remotes: Vec<String> = self.repo.data.remotes.iter().map(|r| r.name.clone()).collect();

        if self.selected.len() > 1 {
            let mut sel: Vec<usize> = self.selected.clone();
            sel.sort();
            let commits: Vec<Commit> = sel.iter().rev().filter_map(|&k| self.repo.data.commits.get(k).cloned()).collect();
            if sel.len() == 2 {
                let (a, b) = (&self.repo.data.commits[sel[1]], &self.repo.data.commits[sel[0]]);
                if ui.button("Compare selected commits").clicked() {
                    self.actions.push(Action::OpenCompare { from: a.sha.clone(), to: b.sha.clone(), from_label: a.short_sha().into(), to_label: b.short_sha().into() });
                    close = true;
                }
            }
            if ui.button(format!("Cherry-pick {} commits…", commits.len())).clicked() {
                self.actions.push(Action::Popup(Popup::CherryPick {
                    shas: commits.iter().map(|c| c.sha.clone()).collect(),
                    labels: commits.iter().map(|c| format!("{}  {}", c.short_sha(), c.subject)).collect(),
                    no_commit: false,
                    append_source: false,
                    is_merge: false,
                    mainline: 1,
                }));
                close = true;
            }
            if ui.button("Copy SHAs").clicked() {
                widgets::copy(ui, &commits.iter().map(|c| c.sha.clone()).collect::<Vec<_>>().join("\n"));
                close = true;
            }
            return close;
        }

        // Branch shortcuts for decorators on this commit
        for d in c.decorators.iter() {
            match d.kind {
                DecoratorKind::LocalBranchHead => {
                    if let Some(b) = self.repo.data.branches.iter().find(|b| b.is_local && b.name == d.name).cloned() {
                        ui.menu_button(format!("🔀 {}", b.name), |ui| self.branch_menu(ui, &b));
                    }
                }
                DecoratorKind::CurrentBranchHead => {
                    if let Some(b) = cur.clone() {
                        ui.menu_button(format!("✔ {}", b.name), |ui| self.branch_menu(ui, &b));
                    }
                }
                DecoratorKind::RemoteBranchHead => {
                    if let Some(b) = self.repo.data.branches.iter().find(|b| !b.is_local && b.friendly_name() == d.name).cloned() {
                        ui.menu_button(format!("☁ {}", d.name), |ui| self.branch_menu(ui, &b));
                    }
                }
                _ => {}
            }
        }
        if !c.decorators.is_empty() {
            ui.separator();
        }

        if cur.is_some() && ui.button(format!("Reset {cur_name} to here…")).clicked() {
            self.actions.push(Action::Popup(Popup::Reset { sha: c.sha.clone(), subject: format!("{}  {}", c.short_sha(), c.subject), branch: cur_name.clone(), mode: ops::ResetMode::Mixed }));
            close = true;
        }
        let others: Vec<String> = self.repo.local_branches().filter(|b| !b.is_current && b.worktree_path.is_empty()).map(|b| b.name.clone()).collect();
        if !others.is_empty() {
            ui.menu_button("Reset other branch to here", |ui| {
                for b in &others {
                    if ui.button(b).clicked() {
                        self.actions.push(Action::Popup(Popup::ResetBranch { branch: b.clone(), sha: c.sha.clone(), subject: format!("{}  {}", c.short_sha(), c.subject) }));
                        close = true;
                    }
                }
            });
        }
        if !c.is_merged {
            if ui.button(format!("Rebase {cur_name} to here…")).clicked() {
                self.actions.push(Action::Popup(Popup::Rebase { onto: c.sha.clone(), label: format!("{}  {}", c.short_sha(), c.subject), current: cur_name.clone(), autostash: true }));
                close = true;
            }
            if ui.button(format!("Merge into {cur_name}…")).clicked() {
                self.actions.push(Action::Popup(Popup::Merge { source: c.sha.clone(), into: cur_name.clone(), mode: ops::MergeMode::Default, message: String::new() }));
                close = true;
            }
            if ui.button("Cherry-pick…").clicked() {
                self.actions.push(Action::Popup(Popup::CherryPick {
                    shas: vec![c.sha.clone()],
                    labels: vec![format!("{}  {}", c.short_sha(), c.subject)],
                    no_commit: false,
                    append_source: false,
                    is_merge: c.parents.len() > 1,
                    mainline: 1,
                }));
                close = true;
            }
        } else if ui.button("Revert commit…").clicked() {
            self.actions.push(Action::Popup(Popup::Revert { sha: c.sha.clone(), subject: format!("{}  {}", c.short_sha(), c.subject), auto_commit: true, is_merge: c.parents.len() > 1 }));
            close = true;
        }
        if ui.button("Checkout commit (detached HEAD)").clicked() {
            let s = c.sha.clone();
            self.actions.push(Action::Popup(Popup::Confirm {
                title: "Checkout Commit".into(),
                message: format!("Checkout {} in detached HEAD state?", c.short_sha()),
                danger: false,
                job: Some(Job::git("Checkout commit", move |p| ops::checkout_detached(p, &s))),
            }));
            close = true;
        }
        ui.separator();
        if ui.button("Create branch here…").clicked() {
            self.actions.push(Action::Popup(Popup::CreateBranch { base: c.sha.clone(), base_label: format!("{}  {}", c.short_sha(), c.subject), name: String::new(), checkout: true }));
            close = true;
        }
        if ui.button("Create tag here…").clicked() {
            self.actions.push(Action::Popup(Popup::CreateTag { rev: c.sha.clone(), label: format!("{}  {}", c.short_sha(), c.subject), name: String::new(), annotated: true, message: String::new(), push: false, remotes, remote: 0 }));
            close = true;
        }
        ui.separator();
        if self.repo.data.has_head && ui.button("Compare with HEAD").clicked() {
            self.actions.push(Action::OpenCompare { from: c.sha.clone(), to: "HEAD".into(), from_label: c.short_sha().into(), to_label: "HEAD".into() });
            close = true;
        }
        if ui.button("Compare with working tree").clicked() {
            self.actions.push(Action::OpenCompare { from: c.sha.clone(), to: String::new(), from_label: c.short_sha().into(), to_label: "Worktree".into() });
            close = true;
        }
        if c.is_merged && self.repo.data.has_head && ui.button("Bisect: start (HEAD bad, this good)").clicked() {
            let s = c.sha.clone();
            self.actions.push(Action::Job(Job::new("Bisect start", move |p| {
                ops::bisect(p, "start").into_result()?;
                ops::bisect(p, "bad").into_result()?;
                crate::git::exec(p, &["bisect", "good", &s]).into_result()
            })));
            close = true;
        }
        ui.separator();
        if ui.button("Save as patch…").clicked() {
            if let Some(f) = rfd::FileDialog::new().set_file_name(format!("{}.patch", c.short_sha())).save_file() {
                if let Err(e) = ops::format_patch(&self.repo.path, &c.sha, &f) {
                    self.actions.push(Action::Error(e));
                } else {
                    self.actions.push(Action::Info(format!("Saved {}", f.display())));
                }
            }
            close = true;
        }
        if ui.button("Archive…").clicked() {
            self.actions.push(Action::Popup(Popup::Archive { rev: c.sha.clone(), label: format!("{}  {}", c.short_sha(), c.subject), file: String::new() }));
            close = true;
        }
        ui.separator();
        if ui.button("Copy SHA").clicked() {
            widgets::copy(ui, &c.sha);
            close = true;
        }
        if ui.button("Copy subject").clicked() {
            widgets::copy(ui, &c.subject);
            close = true;
        }
        if ui.button("Copy info").clicked() {
            widgets::copy(ui, &format!("{}\n{}\n{} <{}>\n{}", c.sha, c.subject, c.author.name, c.author.email, widgets::full_time(c.author_time)));
            close = true;
        }
        close
    }

    fn commit_detail(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences) {
        // Tab strip
        egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (tab, label) in [(DetailTab::Info, "INFORMATION"), (DetailTab::Changes, "CHANGES"), (DetailTab::Files, "FILES")] {
                    let sel = self.detail_tab == tab;
                    let t = RichText::new(label).strong().size(14.0).color(if sel { pal.accent } else { pal.fg2 });
                    let r = ui.add(egui::Label::new(t).sense(Sense::click()));
                    if sel {
                        ui.painter().line_segment([r.rect.left_bottom() + Vec2::new(0.0, 2.0), r.rect.right_bottom() + Vec2::new(0.0, 2.0)], Stroke::new(2.0_f32, pal.accent));
                    }
                    if r.clicked() {
                        self.detail_tab = tab;
                        if tab == DetailTab::Files {
                            self.ensure_files();
                        }
                    }
                    ui.add_space(14.0);
                }
                if self.selected.len() > 1 {
                    ui.label(RichText::new(format!("{} commits selected", self.selected.len())).color(pal.fg2));
                }
            });
        });
        if self.detail.is_none() {
            ui.centered_and_justified(|ui| ui.label(RichText::new("Select a commit").color(pal.fg2)));
            return;
        }
        match self.detail_tab {
            DetailTab::Info => self.detail_info(ui, pal, prefs),
            DetailTab::Changes => self.detail_changes(ui, pal, prefs),
            DetailTab::Files => self.detail_files(ui, pal),
        }
    }

    fn detail_info(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &Preferences) {
        let Some(d) = &self.detail else { return };
        let c = d.commit.clone();
        let message = d.message.clone();
        let changes = d.changes.clone();
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("detail-info").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(30.0);
                widgets::avatar(ui, &c.author.name, 64.0);
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("AUTHOR").strong().color(pal.fg2));
                    ui.horizontal(|ui| {
                        ui.label(&c.author.name);
                        ui.label(RichText::new(&c.author.email).color(pal.fg2));
                    });
                    ui.label(RichText::new(widgets::full_time(c.author_time)).color(pal.fg2));
                });
                if c.committer != c.author || c.committer_time != c.author_time {
                    ui.add_space(30.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("COMMITTER").strong().color(pal.fg2));
                        ui.horizontal(|ui| {
                            ui.label(&c.committer.name);
                            ui.label(RichText::new(&c.committer.email).color(pal.fg2));
                        });
                        ui.label(RichText::new(widgets::full_time(c.committer_time)).color(pal.fg2));
                    });
                }
            });
            ui.add_space(6.0);
            ui.separator();
            egui::Grid::new("commit-info-grid").num_columns(2).spacing([14.0, 8.0]).min_col_width(90.0).show(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new("SHA").strong().color(pal.fg2)));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&c.sha).monospace());
                    if widgets::small_button(ui, "📋", "Copy SHA").clicked() {
                        widgets::copy(ui, &c.sha);
                    }
                });
                ui.end_row();
                if !c.parents.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new("PARENTS").strong().color(pal.fg2)));
                    ui.horizontal(|ui| {
                        for p in &c.parents {
                            let short = &p[..p.len().min(SHORT_SHA_LENGTH)];
                            if ui.link(RichText::new(short).monospace().color(GRAPH_COLORS[0])).clicked() {
                                self.actions.push(Action::SelectCommit(p.clone()));
                            }
                        }
                    });
                    ui.end_row();
                }
                if !c.decorators.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new("REFS").strong().color(pal.fg2)));
                    ui.horizontal_wrapped(|ui| {
                        for d in &c.decorators {
                            let label = match d.kind {
                                DecoratorKind::CurrentBranchHead => format!("✔ {}", d.name),
                                DecoratorKind::LocalBranchHead => format!("🔀 {}", d.name),
                                DecoratorKind::CurrentCommitHead => "HEAD".into(),
                                DecoratorKind::RemoteBranchHead => format!("☁ {}", d.name),
                                DecoratorKind::Tag => format!("🏷 {}", d.name),
                            };
                            widgets::badge(ui, &label, pal.sub_panel, pal.fg1);
                        }
                    });
                    ui.end_row();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| ui.label(RichText::new("MESSAGE").strong().color(pal.fg2)));
                ui.add(egui::Label::new(message.as_str()).selectable(true).wrap());
                ui.end_row();
            });
            ui.separator();
            ui.add_space(4.0);
            for (k, ch) in changes.iter().enumerate() {
                let r = widgets::row(ui, false, 22.0, pal, |ui| {
                    ui.add_space(90.0);
                    widgets::change_icon(ui, ch.index);
                    if ch.original_path.is_empty() {
                        ui.label(&ch.path);
                    } else {
                        ui.label(format!("{} → {}", ch.original_path, ch.path));
                    }
                });
                if r.double_clicked() || r.clicked() && ui.input(|i| i.modifiers.command) {
                    self.detail_tab = DetailTab::Changes;
                    self.actions.push(Action::LoadCommitDiff(k));
                }
            }
            let _ = prefs;
        });
    }

    fn detail_changes(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences) {
        let repo_path = self.repo.path.clone();
        let mut load = None;
        let mut act = None;
        let Some(d) = &mut self.detail else { return };
        egui::SidePanel::left("detail-changes-list")
            .default_width(COMMIT_DETAIL_CHANGES_LEFT_WIDTH)
            .resizable(true)
            .frame(egui::Frame::new().fill(pal.window).inner_margin(4))
            .show_inside(ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut d.change_filter).hint_text("🔍 Search changes…").desired_width(f32::INFINITY));
                let f = d.change_filter.to_lowercase();
                egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("detail-changes").show(ui, |ui| {
                    for (k, ch) in d.changes.iter().enumerate() {
                        if !f.is_empty() && !ch.path.to_lowercase().contains(&f) {
                            continue;
                        }
                        let r = widgets::row(ui, d.selected_change == Some(k), 22.0, pal, |ui| {
                            widgets::change_icon(ui, ch.index);
                            ui.label(&ch.path);
                        });
                        if r.clicked() {
                            load = Some(k);
                        }
                        let path = ch.path.clone();
                        let sha = d.commit.sha.clone();
                        r.context_menu(|ui| {
                            if ui.button("Blame").clicked() {
                                act = Some(Action::OpenBlame { path: path.clone(), rev: sha.clone() });
                                ui.close_menu();
                            }
                            if ui.button("File history").clicked() {
                                act = Some(Action::OpenFileHistory(path.clone()));
                                ui.close_menu();
                            }
                            if ui.button("Copy path").clicked() {
                                widgets::copy(ui, &path);
                                ui.close_menu();
                            }
                        });
                    }
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.contents)).show_inside(ui, |ui| {
            match d.diff.show(ui, pal, prefs, DiffMode::ReadOnly) {
                Some(DiffAction::Reload) => d.diff.reload(&repo_path, prefs),
                Some(DiffAction::OpenBlame) => {
                    if let Some(t) = &d.diff.target {
                        act = Some(Action::OpenBlame { path: t.path().to_string(), rev: d.commit.sha.clone() });
                    }
                }
                Some(DiffAction::OpenHistory) => {
                    if let Some(t) = &d.diff.target {
                        act = Some(Action::OpenFileHistory(t.path().to_string()));
                    }
                }
                _ => {}
            }
        });
        if let Some(k) = load {
            self.actions.push(Action::LoadCommitDiff(k));
        }
        if let Some(a) = act {
            self.actions.push(a);
        }
    }

    fn detail_files(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        let repo_path = self.repo.path.clone();
        let mut act = None;
        let Some(d) = &mut self.detail else { return };
        let files = d.files.clone().unwrap_or_default();
        egui::SidePanel::left("detail-files-list")
            .default_width(COMMIT_DETAIL_CHANGES_LEFT_WIDTH)
            .resizable(true)
            .frame(egui::Frame::new().fill(pal.window).inner_margin(4))
            .show_inside(ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut d.file_filter).hint_text("🔍 Search files…").desired_width(f32::INFINITY));
                let f = d.file_filter.to_lowercase();
                let shown: Vec<&crate::git::RevisionFile> = files.iter().filter(|x| f.is_empty() || x.path.to_lowercase().contains(&f)).collect();
                egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("detail-files").show_rows(ui, 22.0, shown.len(), |ui, range| {
                    for k in range {
                        let file = shown[k];
                        let r = widgets::row(ui, d.selected_file.as_deref() == Some(&file.path), 22.0, pal, |ui| {
                            ui.label(RichText::new("📄").color(pal.fg2));
                            ui.label(&file.path);
                        });
                        if r.clicked() {
                            d.selected_file = Some(file.path.clone());
                            d.file_content = Some(match parse::query_file_content(&repo_path, &d.commit.sha, &file.path) {
                                Ok(bytes) if parse::is_binary(&bytes) => Err(format!("Binary file ({} bytes)", bytes.len())),
                                Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
                                Err(e) => Err(e),
                            });
                        }
                        let path = file.path.clone();
                        let sha = d.commit.sha.clone();
                        r.context_menu(|ui| {
                            if ui.button("Blame").clicked() {
                                act = Some(Action::OpenBlame { path: path.clone(), rev: sha.clone() });
                                ui.close_menu();
                            }
                            if ui.button("File history").clicked() {
                                act = Some(Action::OpenFileHistory(path.clone()));
                                ui.close_menu();
                            }
                            if ui.button("Save as…").clicked() {
                                let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                if let Some(out) = rfd::FileDialog::new().set_file_name(name).save_file() {
                                    if let Ok(bytes) = parse::query_file_content(&repo_path, &sha, &path) {
                                        let _ = std::fs::write(out, bytes);
                                    }
                                }
                                ui.close_menu();
                            }
                            if ui.button("Copy path").clicked() {
                                widgets::copy(ui, &path);
                                ui.close_menu();
                            }
                        });
                    }
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.contents)).show_inside(ui, |ui| match &d.file_content {
            None => {
                ui.centered_and_justified(|ui| ui.label(RichText::new("Select a file to view its content").color(pal.fg2)));
            }
            Some(Err(e)) => {
                ui.centered_and_justified(|ui| ui.label(RichText::new(e.as_str()).color(pal.fg2)));
            }
            Some(Ok(text)) => {
                let t = text.clone();
                diff_view::show_text(ui, pal, &t, "file-content");
            }
        });
        if let Some(a) = act {
            self.actions.push(a);
        }
    }
}
