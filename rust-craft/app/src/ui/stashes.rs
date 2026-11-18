//! Stashes page and the floating tool windows (blame, file history, compare).

use super::diff_view::{DiffAction, DiffMode};
use super::histories::subject_job;
use super::popups::{Job, Popup};
use super::repo_view::{load_history_diff, Action, RepoView};
use super::widgets;
use crate::consts::*;
use crate::git::{ops, ChangeState};
use crate::prefs::Preferences;
use crate::theme::Palette;
use eframe::egui::{self, FontId, RichText};

impl RepoView {
    pub fn stashes(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences) {
        let stashes = self.repo.data.stashes.clone();
        egui::SidePanel::left("stash-left")
            .default_width(WORKING_COPY_LEFT_WIDTH)
            .resizable(true)
            .frame(egui::Frame::new().fill(pal.window).inner_margin(4))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::section_label(ui, pal, &format!("STASHES ({})", stashes.len()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::small_button(ui, "🗑", "Clear all stashes").clicked() && !stashes.is_empty() {
                            self.popup = Some(Popup::Confirm {
                                title: "Clear Stashes".into(),
                                message: format!("Drop all {} stashes? This cannot be undone.", stashes.len()),
                                danger: true,
                                job: Some(Job::git("Clear stashes", ops::stash_clear)),
                            });
                        }
                    });
                });
                ui.add(egui::TextEdit::singleline(&mut self.stash.filter).hint_text("🔍 Search stashes…").desired_width(f32::INFINITY));
                let f = self.stash.filter.to_lowercase();
                let half = ui.available_height() / 2.0;
                egui::ScrollArea::vertical().max_height(half).min_scrolled_height(half).auto_shrink([false, false]).id_salt("stash-list").show(ui, |ui| {
                    for (i, s) in stashes.iter().enumerate() {
                        if !f.is_empty() && !s.message.to_lowercase().contains(&f) {
                            continue;
                        }
                        let resp = widgets::row(ui, self.stash.selected == Some(i), 40.0, pal, |ui| {
                            ui.vertical(|ui| {
                                ui.add_space(3.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&s.name).monospace().color(pal.fg2));
                                    ui.label(RichText::new(widgets::rel_time(s.time, prefs.relative_time)).size(11.0).color(pal.fg2));
                                });
                                ui.label(s.subject());
                            });
                        });
                        if resp.clicked() {
                            self.stash_select(i, prefs);
                        }
                        let sc = s.clone();
                        resp.context_menu(|ui| {
                            if ui.button("Apply…").clicked() {
                                self.popup = Some(Popup::StashApply { name: sc.name.clone(), restore_index: false, drop_after: false });
                                ui.close_menu();
                            }
                            if ui.button("Pop").clicked() {
                                let n = sc.name.clone();
                                self.actions.push(Action::Job(Job::git("Pop stash", move |p| ops::stash_pop(p, &n))));
                                ui.close_menu();
                            }
                            if ui.button("Create branch from stash…").clicked() {
                                self.popup = Some(Popup::StashBranch { stash: sc.name.clone(), branch: String::new() });
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Drop…").clicked() {
                                let n = sc.name.clone();
                                self.popup = Some(Popup::Confirm {
                                    title: "Drop Stash".into(),
                                    message: format!("Drop {}: {}?", sc.name, sc.subject()),
                                    danger: true,
                                    job: Some(Job::git("Drop stash", move |p| ops::stash_drop(p, &n))),
                                });
                                ui.close_menu();
                            }
                            if ui.button("Copy message").clicked() {
                                widgets::copy(ui, &sc.message);
                                ui.close_menu();
                            }
                        });
                    }
                });
                ui.separator();
                widgets::section_label(ui, pal, "CHANGES");
                let changes = self.stash.changes.clone();
                egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("stash-changes").show(ui, |ui| {
                    for (k, c) in changes.iter().enumerate() {
                        let r = widgets::row(ui, self.stash.selected_change == Some(k), 22.0, pal, |ui| {
                            widgets::change_icon(ui, c.index);
                            ui.label(&c.path);
                        });
                        if r.clicked() {
                            self.stash.selected_change = Some(k);
                            if let Some(s) = self.stash.selected.and_then(|i| self.repo.data.stashes.get(i)).cloned() {
                                let target = if c.index == ChangeState::Untracked {
                                    ops::DiffTarget::Revisions { from: String::new(), to: s.parents.get(2).cloned().unwrap_or_default(), path: c.path.clone(), original: String::new() }
                                } else {
                                    ops::DiffTarget::Revisions { from: s.parents.first().cloned().unwrap_or_default(), to: s.sha.clone(), path: c.path.clone(), original: c.original_path.clone() }
                                };
                                self.stash.diff.load(&self.repo.path, target, prefs);
                            }
                        }
                    }
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.contents)).show_inside(ui, |ui| {
            if self.stash.selected.is_none() {
                ui.centered_and_justified(|ui| ui.label(RichText::new(if stashes.is_empty() { "No stashes" } else { "Select a stash" }).color(pal.fg2)));
                return;
            }
            if let Some(DiffAction::Reload) = self.stash.diff.show(ui, pal, prefs, DiffMode::ReadOnly) {
                self.stash.diff.reload(&self.repo.path, prefs);
            }
        });
    }

    pub fn windows(&mut self, ctx: &egui::Context, pal: &Palette, prefs: &mut Preferences) {
        self.blame_window(ctx, pal, prefs);
        self.history_window(ctx, pal, prefs);
        self.compare_window(ctx, pal, prefs);
    }

    fn blame_window(&mut self, ctx: &egui::Context, pal: &Palette, prefs: &Preferences) {
        let Some(b) = &self.blame else { return };
        let mut open = true;
        let mut goto = None;
        let title = format!("Blame — {} @ {}", b.path, &b.rev[..b.rev.len().min(SHORT_SHA_LENGTH)]);
        egui::Window::new(title).id(egui::Id::new("blame-win")).open(&mut open).default_size([900.0, 600.0]).resizable(true).show(ctx, |ui| match &b.lines {
            Err(e) => {
                ui.label(RichText::new(e).color(egui::Color32::LIGHT_RED));
            }
            Ok(lines) => {
                let font = FontId::monospace(12.0);
                let row_h = 18.0;
                egui::ScrollArea::both().auto_shrink([false, false]).show_rows(ui, row_h, lines.len(), |ui, range| {
                    for i in range {
                        let l = &lines[i];
                        let first_of_block = i == 0 || lines[i - 1].sha != l.sha;
                        ui.horizontal(|ui| {
                            ui.set_height(row_h);
                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(330.0, row_h), egui::Sense::click());
                            if first_of_block {
                                let p = ui.painter();
                                p.line_segment([rect.left_top(), rect.right_top() + egui::vec2(2000.0, 0.0)], egui::Stroke::new(1.0_f32, pal.border2));
                                p.text(rect.left_center() + egui::vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, &l.sha[..l.sha.len().min(SHORT_SHA_LENGTH)], font.clone(), pal.link);
                                p.text(rect.left_center() + egui::vec2(90.0, 0.0), egui::Align2::LEFT_CENTER, widgets::truncate_middle(&l.author, 16), font.clone(), pal.fg1);
                                p.text(rect.left_center() + egui::vec2(230.0, 0.0), egui::Align2::LEFT_CENTER, widgets::rel_time(l.time, false).get(..10).unwrap_or(""), font.clone(), pal.fg2);
                            }
                            if resp.clicked() {
                                goto = Some(l.sha.clone());
                            }
                            let resp = resp.on_hover_text(format!("{}\n{}\n{}\n{}", l.sha, l.author, widgets::full_time(l.time), l.file));
                            let _ = resp;
                            ui.label(RichText::new(format!("{:>5}", i + 1)).font(font.clone()).color(pal.fg2));
                            ui.label(RichText::new(l.content.replace('\t', "    ")).font(font.clone()));
                        });
                    }
                });
            }
        });
        let _ = prefs;
        if let Some(s) = goto {
            self.actions.push(Action::SelectCommit(s));
        }
        if !open {
            self.blame = None;
        }
    }

    fn history_window(&mut self, ctx: &egui::Context, pal: &Palette, prefs: &mut Preferences) {
        let repo = self.repo.path.clone();
        let Some(w) = &mut self.file_history else { return };
        let mut open = true;
        let mut goto = None;
        let mut blame = None;
        egui::Window::new(format!("File History — {}", w.path)).id(egui::Id::new("history-win")).open(&mut open).default_size([1000.0, 640.0]).resizable(true).show(ctx, |ui| {
            egui::SidePanel::left("fh-list").default_width(340.0).resizable(true).show_inside(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 40.0, w.commits.len(), |ui, range| {
                    for i in range {
                        let c = w.commits[i].clone();
                        let r = widgets::row(ui, w.selected == Some(i), 40.0, pal, |ui| {
                            ui.vertical(|ui| {
                                ui.add_space(3.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(c.short_sha()).monospace().color(pal.link));
                                    ui.label(RichText::new(&c.author.name).color(pal.fg2));
                                    ui.label(RichText::new(widgets::rel_time(c.committer_time, prefs.relative_time)).size(11.0).color(pal.fg2));
                                });
                                ui.label(subject_job(&c.subject, pal, FontId::proportional(13.0), false));
                            });
                        });
                        if r.clicked() {
                            w.selected = Some(i);
                            load_history_diff(&repo, w, prefs);
                        }
                        r.context_menu(|ui| {
                            if ui.button("Go to commit in history").clicked() {
                                goto = Some(c.sha.clone());
                                ui.close_menu();
                            }
                            if ui.button("Blame at this revision").clicked() {
                                blame = Some(c.sha.clone());
                                ui.close_menu();
                            }
                            if ui.button("Copy SHA").clicked() {
                                widgets::copy(ui, &c.sha);
                                ui.close_menu();
                            }
                        });
                    }
                });
            });
            egui::CentralPanel::default().show_inside(ui, |ui| {
                if let Some(DiffAction::Reload) = w.diff.show(ui, pal, prefs, DiffMode::ReadOnly) {
                    w.diff.reload(&repo, prefs);
                }
            });
        });
        let path = w.path.clone();
        if let Some(s) = goto {
            self.actions.push(Action::SelectCommit(s));
        }
        if let Some(rev) = blame {
            self.actions.push(Action::OpenBlame { path, rev });
        }
        if !open {
            self.file_history = None;
        }
    }

    fn compare_window(&mut self, ctx: &egui::Context, pal: &Palette, prefs: &mut Preferences) {
        let repo = self.repo.path.clone();
        let Some(w) = &mut self.compare else { return };
        let mut open = true;
        let mut swap = false;
        egui::Window::new(format!("Compare {} ↔ {}", w.from_label, w.to_label)).id(egui::Id::new("compare-win")).open(&mut open).default_size([1000.0, 640.0]).resizable(true).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&w.from_label).monospace().strong());
                if ui.small_button("🔃").on_hover_text("Swap").clicked() {
                    swap = true;
                }
                ui.label(RichText::new(&w.to_label).monospace().strong());
                ui.label(RichText::new(format!("{} file(s) changed", w.changes.len())).color(pal.fg2));
            });
            ui.separator();
            egui::SidePanel::left("cmp-list").default_width(320.0).resizable(true).show_inside(ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut w.filter).hint_text("🔍 Filter…").desired_width(f32::INFINITY));
                let f = w.filter.to_lowercase();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (k, c) in w.changes.clone().iter().enumerate() {
                        if !f.is_empty() && !c.path.to_lowercase().contains(&f) {
                            continue;
                        }
                        let r = widgets::row(ui, w.selected == Some(k), 22.0, pal, |ui| {
                            widgets::change_icon(ui, c.index);
                            ui.label(&c.path);
                        });
                        if r.clicked() {
                            w.selected = Some(k);
                            let target = ops::DiffTarget::Revisions { from: w.from.clone(), to: w.to.clone(), path: c.path.clone(), original: c.original_path.clone() };
                            w.diff.load(&repo, target, prefs);
                        }
                    }
                });
            });
            egui::CentralPanel::default().show_inside(ui, |ui| {
                if let Some(DiffAction::Reload) = w.diff.show(ui, pal, prefs, DiffMode::ReadOnly) {
                    w.diff.reload(&repo, prefs);
                }
            });
        });
        if swap && !w.to.is_empty() {
            std::mem::swap(&mut w.from, &mut w.to);
            std::mem::swap(&mut w.from_label, &mut w.to_label);
            w.changes = crate::git::parse::compare_revisions(&repo, &w.from, &w.to);
            w.selected = None;
            w.diff.clear();
        }
        if !open {
            self.compare = None;
        }
    }
}
