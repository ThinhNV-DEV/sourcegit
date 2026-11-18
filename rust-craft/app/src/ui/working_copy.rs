//! Local changes page: unstaged / staged lists, diff with hunk & line staging, commit box.

use super::diff_view::{DiffAction, DiffMode};
use super::popups::{Job, Popup};
use super::repo_view::{Action, RepoView};
use super::widgets;
use crate::consts::*;
use crate::git::{ops, Change, ChangeState};
use crate::prefs::Preferences;
use crate::theme::Palette;
use eframe::egui::{self, RichText};

impl RepoView {
    pub fn working_copy(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences) {
        egui::SidePanel::left("wc-left")
            .default_width(WORKING_COPY_LEFT_WIDTH)
            .width_range(200.0..=700.0)
            .resizable(true)
            .frame(egui::Frame::new().fill(pal.window).inner_margin(4))
            .show_inside(ui, |ui| self.wc_lists(ui, pal, prefs));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.contents)).show_inside(ui, |ui| {
            egui::TopBottomPanel::bottom("wc-commit")
                .resizable(true)
                .default_height(170.0)
                .height_range(130.0..=400.0)
                .frame(egui::Frame::new().fill(pal.window).inner_margin(8))
                .show_inside(ui, |ui| self.commit_box(ui, pal, prefs));
            egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.contents)).show_inside(ui, |ui| {
                if let Some(c) = self.selected_conflict() {
                    self.conflict_panel(ui, pal, &c);
                    return;
                }
                let mode = if self.wc.focus_staged { DiffMode::Staged } else { DiffMode::Unstaged };
                let a = self.wc.diff.show(ui, pal, prefs, mode);
                if let Some(a) = a {
                    self.handle_wc_diff_action(a, prefs);
                }
            });
        });
    }

    fn selected_conflict(&self) -> Option<Change> {
        if self.wc.focus_staged {
            return None;
        }
        let p = self.wc.last_selected.as_ref()?;
        self.repo.data.changes.iter().find(|c| &c.path == p && c.is_conflicted()).cloned()
    }

    fn conflict_panel(&mut self, ui: &mut egui::Ui, pal: &Palette, c: &Change) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(RichText::new("⚠").size(36.0).color(pal.conflict));
            ui.label(RichText::new(format!("{} — {}", c.path, c.conflict_desc())).strong().size(15.0));
            ui.add_space(4.0);
            ui.label(RichText::new("Resolve it with one side, edit the file manually and mark it resolved, or open your merge tool.").color(pal.fg2));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 420.0).max(0.0) / 2.0);
                let p1 = c.path.clone();
                if ui.button("Use Mine (ours)").clicked() {
                    self.actions.push(Action::Job(Job::git("Use mine", move |p| ops::use_side(p, &p1, false))));
                }
                let p2 = c.path.clone();
                if ui.button("Use Theirs").clicked() {
                    self.actions.push(Action::Job(Job::git("Use theirs", move |p| ops::use_side(p, &p2, true))));
                }
                let p3 = c.path.clone();
                if ui.button("Mark as Resolved").clicked() {
                    self.actions.push(Action::Job(Job::git("Mark resolved", move |p| ops::mark_resolved(p, &p3))));
                }
                let p4 = c.path.clone();
                if ui.button("Open Merge Tool").clicked() {
                    self.actions.push(Action::Job(Job::git("Merge tool", move |p| crate::git::exec(p, &["mergetool", "--no-prompt", "--", &p4]))));
                }
            });
        });
        // Show the file with conflict markers.
        let path = self.repo.path.join(&c.path);
        if let Ok(text) = std::fs::read_to_string(path) {
            ui.add_space(12.0);
            super::diff_view::show_text(ui, pal, &text, "conflict-text");
        }
    }

    fn handle_wc_diff_action(&mut self, a: DiffAction, prefs: &mut Preferences) {
        let repo = self.repo.path.clone();
        let result: Option<Result<(), String>> = match a {
            DiffAction::Reload => {
                self.wc.diff.reload(&repo, prefs);
                None
            }
            DiffAction::OpenBlame => {
                if let Some(t) = &self.wc.diff.target {
                    self.actions.push(Action::OpenBlame { path: t.path().to_string(), rev: "HEAD".into() });
                }
                None
            }
            DiffAction::OpenHistory => {
                if let Some(t) = &self.wc.diff.target {
                    self.actions.push(Action::OpenFileHistory(t.path().to_string()));
                }
                None
            }
            DiffAction::StageHunk(h) | DiffAction::UnstageHunk(h) | DiffAction::DiscardHunk(h) => {
                let td = self.wc.diff.text().cloned();
                td.and_then(|td| ops::hunk_patch(&td, h)).map(|patch| {
                    let (cached, reverse) = match a {
                        DiffAction::StageHunk(_) => (true, false),
                        DiffAction::UnstageHunk(_) => (true, true),
                        _ => (false, true),
                    };
                    ops::apply_patch_text(&repo, &patch, cached, reverse).into_result().map(|_| ())
                })
            }
            DiffAction::StageLines | DiffAction::UnstageLines | DiffAction::DiscardLines => {
                let td = self.wc.diff.text().cloned();
                let sel = self.wc.diff.selected.clone();
                let (cached, reverse) = match a {
                    DiffAction::StageLines => (true, false),
                    DiffAction::UnstageLines => (true, true),
                    _ => (false, true),
                };
                td.and_then(|td| ops::lines_patch(&td, &sel, reverse))
                    .map(|patch| ops::apply_patch_text(&repo, &patch, cached, reverse).into_result().map(|_| ()))
            }
        };
        if let Some(r) = result {
            if let Err(e) = r {
                self.actions.push(Action::Error(e));
            }
            self.refresh_status(prefs);
            self.wc.diff.reload(&repo, prefs);
        }
    }

    fn select_wc_change(&mut self, c: &Change, staged: bool, prefs: &Preferences) {
        self.wc.focus_staged = staged;
        self.wc.last_selected = Some(c.path.clone());
        if c.is_conflicted() && !staged {
            self.wc.diff.clear();
            return;
        }
        let target = if staged {
            ops::DiffTarget::Staged { path: c.path.clone(), original: c.original_path.clone() }
        } else {
            ops::DiffTarget::Unstaged { path: c.path.clone(), untracked: c.worktree == ChangeState::Untracked || (c.index == ChangeState::Added && false) }
        };
        self.wc.diff.load(&self.repo.path, target, prefs);
    }

    fn wc_lists(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences) {
        let unstaged = self.repo.unstaged();
        let staged = self.repo.staged();
        let busy = self.task.is_some();
        ui.add(egui::TextEdit::singleline(&mut self.wc.filter).hint_text("🔍 Filter changes…").desired_width(f32::INFINITY));
        let filter = self.wc.filter.to_lowercase();
        let avail = ui.available_height();

        // UNSTAGED
        ui.horizontal(|ui| {
            widgets::section_label(ui, pal, &format!("UNSTAGED ({})", unstaged.len()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    if widgets::small_button(ui, "⏬", "Stage all").clicked() && !unstaged.is_empty() {
                        let r = ops::stage(&self.repo.path, &[]);
                        self.after_local_op(r, prefs);
                    }
                    let sel: Vec<String> = self.wc.sel_unstaged.iter().cloned().collect();
                    if widgets::small_button(ui, "⬇", "Stage selected").clicked() && !sel.is_empty() {
                        let r = ops::stage(&self.repo.path, &sel);
                        self.after_local_op(r, prefs);
                    }
                    if widgets::small_button(ui, "↺", "Discard all…").clicked() && !unstaged.is_empty() {
                        self.popup = Some(Popup::Discard { changes: vec![], all: true, include_untracked: true, include_ignored: false });
                    }
                    if ui.selectable_label(prefs.show_untracked, RichText::new("?").size(12.0)).on_hover_text("Show untracked files").clicked() {
                        prefs.show_untracked = !prefs.show_untracked;
                        self.refresh_status(prefs);
                    }
                });
            });
        });
        let list_h = (avail - 80.0) / 2.0;
        egui::ScrollArea::vertical().max_height(list_h).min_scrolled_height(list_h).auto_shrink([false, false]).id_salt("wc-unstaged").show(ui, |ui| {
            for c in unstaged.iter().filter(|c| filter.is_empty() || c.path.to_lowercase().contains(&filter)) {
                self.change_row(ui, pal, prefs, c, false, &unstaged);
            }
        });
        ui.separator();
        // STAGED
        ui.horizontal(|ui| {
            widgets::section_label(ui, pal, &format!("STAGED ({})", staged.len()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    if widgets::small_button(ui, "⏫", "Unstage all").clicked() && !staged.is_empty() {
                        let r = ops::unstage(&self.repo.path, &[], self.repo.data.has_head);
                        self.after_local_op(r, prefs);
                    }
                    let sel: Vec<String> = self.wc.sel_staged.iter().cloned().collect();
                    if widgets::small_button(ui, "⬆", "Unstage selected").clicked() && !sel.is_empty() {
                        let r = ops::unstage(&self.repo.path, &sel, self.repo.data.has_head);
                        self.after_local_op(r, prefs);
                    }
                });
            });
        });
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("wc-staged").show(ui, |ui| {
            for c in staged.iter().filter(|c| filter.is_empty() || c.path.to_lowercase().contains(&filter)) {
                self.change_row(ui, pal, prefs, c, true, &staged);
            }
        });
    }

    fn after_local_op(&mut self, r: crate::git::Output, prefs: &Preferences) {
        if !r.ok {
            self.actions.push(Action::Error(r.stderr.trim().to_string()));
        }
        self.refresh_status(prefs);
        self.wc.sel_unstaged.clear();
        self.wc.sel_staged.clear();
        // Keep the diff of the last selected file in sync with where it is now.
        if let Some(p) = self.wc.last_selected.clone() {
            let now_staged = self.repo.data.changes.iter().find(|c| c.path == p).map(|c| (c.has_staged(), c.has_unstaged()));
            match now_staged {
                Some((s, u)) => {
                    let c = self.repo.data.changes.iter().find(|c| c.path == p).cloned().unwrap();
                    let staged = if self.wc.focus_staged { s } else { !u };
                    self.select_wc_change(&c, staged, prefs);
                }
                None => self.wc.diff.clear(),
            }
        }
    }

    fn change_row(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences, c: &Change, staged: bool, list: &[Change]) {
        let sel_set = if staged { &self.wc.sel_staged } else { &self.wc.sel_unstaged };
        let selected = sel_set.contains(&c.path);
        let state = if staged { c.index } else if c.is_conflicted() { ChangeState::Conflicted } else { c.worktree };
        let resp = widgets::row(ui, selected, 22.0, pal, |ui| {
            widgets::change_icon(ui, state);
            let name = std::path::Path::new(&c.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            ui.label(name);
            let dir = std::path::Path::new(&c.path).parent().map(|p| p.display().to_string()).unwrap_or_default();
            if !dir.is_empty() {
                ui.label(RichText::new(dir).color(pal.fg2).size(11.5));
            }
        });
        let resp = resp.on_hover_text(if c.original_path.is_empty() { c.path.clone() } else { format!("{} → {}", c.original_path, c.path) });
        if resp.clicked() {
            let m = ui.input(|i| i.modifiers);
            let set = if staged { &mut self.wc.sel_staged } else { &mut self.wc.sel_unstaged };
            if m.command {
                if !set.remove(&c.path) {
                    set.insert(c.path.clone());
                }
            } else if m.shift {
                if let Some(last) = &self.wc.last_selected {
                    let a = list.iter().position(|x| &x.path == last).unwrap_or(0);
                    let b = list.iter().position(|x| x.path == c.path).unwrap_or(0);
                    let (s, e) = if a < b { (a, b) } else { (b, a) };
                    for x in &list[s..=e] {
                        set.insert(x.path.clone());
                    }
                }
            } else {
                set.clear();
                set.insert(c.path.clone());
            }
            if staged {
                self.wc.sel_unstaged.clear();
            } else {
                self.wc.sel_staged.clear();
            }
            self.select_wc_change(c, staged, prefs);
        }
        if resp.double_clicked() {
            let r = if staged { ops::unstage(&self.repo.path, &[c.path.clone()], self.repo.data.has_head) } else { ops::stage(&self.repo.path, &[c.path.clone()]) };
            self.after_local_op(r, prefs);
        }
        let cc = c.clone();
        resp.context_menu(|ui| {
            let sel: Vec<Change> = {
                let set = if staged { &self.wc.sel_staged } else { &self.wc.sel_unstaged };
                if set.contains(&cc.path) && set.len() > 1 {
                    list.iter().filter(|x| set.contains(&x.path)).cloned().collect()
                } else {
                    vec![cc.clone()]
                }
            };
            let paths: Vec<String> = sel.iter().map(|x| x.path.clone()).collect();
            let label = if sel.len() > 1 { format!("{} files", sel.len()) } else { String::new() };
            if !staged {
                if ui.button(format!("Stage {label}")).clicked() {
                    let r = ops::stage(&self.repo.path, &paths);
                    self.after_local_op(r, prefs);
                    ui.close_menu();
                }
                if ui.button(format!("Discard {label}…")).clicked() {
                    self.popup = Some(Popup::Discard { changes: sel.clone(), all: false, include_untracked: false, include_ignored: false });
                    ui.close_menu();
                }
                if ui.button(format!("Stash {label}…")).clicked() {
                    self.popup = Some(Popup::StashPush { message: String::new(), include_untracked: true, keep_index: false, only_staged: false, paths: paths.clone() });
                    ui.close_menu();
                }
                if sel.len() == 1 && cc.worktree == ChangeState::Untracked {
                    ui.menu_button("Add to .gitignore", |ui| {
                        let name = std::path::Path::new(&cc.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let ext = std::path::Path::new(&cc.path).extension().map(|e| format!("*.{}", e.to_string_lossy()));
                        let dir = std::path::Path::new(&cc.path).parent().map(|p| format!("{}/", p.display())).filter(|d| d != "/");
                        let mut options = vec![cc.path.clone(), name];
                        if let Some(e) = ext {
                            options.push(e);
                        }
                        if let Some(d) = dir {
                            options.push(d);
                        }
                        for o in options {
                            if ui.button(&o).clicked() {
                                if let Err(e) = ops::add_to_gitignore(&self.repo.path, &o) {
                                    self.actions.push(Action::Error(e));
                                }
                                self.refresh_status(prefs);
                                ui.close_menu();
                            }
                        }
                    });
                }
                if cc.is_conflicted() && sel.len() == 1 {
                    ui.separator();
                    let p1 = cc.path.clone();
                    if ui.button("Use Mine").clicked() {
                        self.actions.push(Action::Job(Job::git("Use mine", move |p| ops::use_side(p, &p1, false))));
                        ui.close_menu();
                    }
                    let p2 = cc.path.clone();
                    if ui.button("Use Theirs").clicked() {
                        self.actions.push(Action::Job(Job::git("Use theirs", move |p| ops::use_side(p, &p2, true))));
                        ui.close_menu();
                    }
                }
            } else if ui.button(format!("Unstage {label}")).clicked() {
                let r = ops::unstage(&self.repo.path, &paths, self.repo.data.has_head);
                self.after_local_op(r, prefs);
                ui.close_menu();
            }
            ui.separator();
            if sel.len() == 1 {
                if ui.button("Open file").clicked() {
                    widgets::open_path(&self.repo.path.join(&cc.path));
                    ui.close_menu();
                }
                if ui.button("Reveal in file manager").clicked() {
                    let p = self.repo.path.join(&cc.path);
                    widgets::open_path(p.parent().unwrap_or(&self.repo.path));
                    ui.close_menu();
                }
                if cc.worktree != ChangeState::Untracked && ui.button("File history").clicked() {
                    self.actions.push(Action::OpenFileHistory(cc.path.clone()));
                    ui.close_menu();
                }
            }
            if ui.button("Save as patch…").clicked() {
                if let Some(f) = rfd::FileDialog::new().set_file_name("changes.patch").save_file() {
                    match ops::save_changes_as_patch(&self.repo.path, staged, &f) {
                        Ok(_) => self.actions.push(Action::Info(format!("Saved {}", f.display()))),
                        Err(e) => self.actions.push(Action::Error(e)),
                    }
                }
                ui.close_menu();
            }
            if ui.button("Copy path").clicked() {
                widgets::copy(ui, &paths.join("\n"));
                ui.close_menu();
            }
        });
    }

    fn commit_box(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences) {
        let staged_count = self.repo.staged().len();
        let conflicts = self.repo.conflicts();
        let busy = self.task.is_some();
        let subject_len = self.wc.message.lines().next().map(|l| l.chars().count()).unwrap_or(0);
        ui.horizontal(|ui| {
            widgets::section_label(ui, pal, "COMMIT MESSAGE");
            let color = if subject_len > SUBJECT_GUIDE_LENGTH { egui::Color32::from_rgb(0xE0, 0x9B, 0) } else { pal.fg2 };
            ui.label(RichText::new(format!("{subject_len}/{SUBJECT_GUIDE_LENGTH}")).size(11.0).color(color));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("Template", |ui| {
                    for ty in ["feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert"] {
                        if ui.button(format!("{ty}: …")).clicked() {
                            let rest = self.wc.message.splitn(2, ": ").nth(1).unwrap_or(&self.wc.message).to_string();
                            self.wc.message = format!("{ty}: {rest}");
                            ui.close_menu();
                        }
                    }
                });
            });
        });
        let h = (ui.available_height() - 40.0).max(50.0);
        egui::ScrollArea::vertical().max_height(h).id_salt("commit-msg").show(ui, |ui| {
            ui.add_sized(
                [ui.available_width(), h],
                egui::TextEdit::multiline(&mut self.wc.message).hint_text("Enter commit message. Use an empty line to separate subject and description!").font(egui::TextStyle::Monospace),
            );
        });
        ui.horizontal(|ui| {
            let amend_before = self.wc.amend;
            ui.checkbox(&mut self.wc.amend, "Amend");
            if self.wc.amend && !amend_before && self.repo.data.has_head && self.wc.message.trim().is_empty() {
                self.wc.message = crate::git::parse::query_full_message(&self.repo.path, "HEAD");
            }
            ui.checkbox(&mut self.wc.sign_off, "Sign-off");
            ui.checkbox(&mut self.wc.no_verify, "No verify");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can = !busy && conflicts == 0 && !self.wc.message.trim().is_empty() && (staged_count > 0 || self.wc.amend);
                let commit_btn = ui.add_enabled(can, egui::Button::new(RichText::new("COMMIT").strong()).min_size(egui::vec2(90.0, 26.0)));
                let ctrl_enter = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) && can;
                let unstaged = self.repo.unstaged().len();
                let can_all = !busy && conflicts == 0 && !self.wc.message.trim().is_empty() && (staged_count + unstaged > 0);
                let commit_all = ui.add_enabled(can_all, egui::Button::new("STAGE ALL & COMMIT")).on_hover_text("Stage all changes then commit");
                if commit_all.clicked() {
                    let r = ops::stage(&self.repo.path, &[]);
                    if !r.ok {
                        self.actions.push(Action::Error(r.stderr));
                    } else {
                        self.do_commit(prefs);
                    }
                } else if commit_btn.clicked() || ctrl_enter {
                    self.do_commit(prefs);
                }
                if staged_count == 0 && !self.wc.amend {
                    ui.label(RichText::new("No staged changes").size(11.0).color(pal.fg2));
                }
                prefs.sign_off = self.wc.sign_off;
            });
        });
    }

    pub fn do_commit(&mut self, prefs: &Preferences) {
        let msg = self.wc.message.clone();
        let (amend, sign_off, no_verify) = (self.wc.amend, self.wc.sign_off, self.wc.no_verify);
        let r = ops::commit(&self.repo.path, &msg, amend, sign_off, amend && self.repo.staged().is_empty(), no_verify);
        if r.ok {
            self.wc.message.clear();
            self.wc.amend = false;
            self.wc.diff.clear();
            self.actions.push(Action::Info("Committed".into()));
            self.refresh(prefs);
        } else {
            let e = if r.stderr.trim().is_empty() { r.stdout } else { r.stderr };
            self.actions.push(Action::Error(format!("Commit failed:\n{}", e.trim())));
        }
    }
}
