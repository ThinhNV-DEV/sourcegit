//! Repository sidebar: local branches, remotes, tags, submodules, worktrees.

use super::popups::{Job, Popup};
use super::repo_view::{Action, RepoView};
use super::widgets;
use crate::git::{ops, Branch, SubmoduleStatus};
use crate::prefs::Preferences;
use crate::repo::HistoryScope;
use crate::theme::{Palette, GRAPH_COLORS};
use eframe::egui::{self, RichText};
use std::collections::BTreeMap;

const ROW_H: f32 = 24.0;

#[derive(Default)]
struct Folder<'a> {
    folders: BTreeMap<String, Folder<'a>>,
    branches: Vec<&'a Branch>,
}

fn build_tree<'a>(branches: &[&'a Branch]) -> Folder<'a> {
    let mut root = Folder::default();
    for b in branches {
        let parts: Vec<&str> = b.name.split('/').collect();
        let mut node = &mut root;
        for p in &parts[..parts.len() - 1] {
            node = node.folders.entry(p.to_string()).or_default();
        }
        node.branches.push(b);
    }
    root
}

impl RepoView {
    pub fn sidebar(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences) {
        ui.add(egui::TextEdit::singleline(&mut self.sidebar_filter).hint_text("🔍 Filter Branches/Tags/Submodules").desired_width(f32::INFINITY));
        ui.add_space(4.0);
        let filter = self.sidebar_filter.to_lowercase();
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("sidebar").show(ui, |ui| {
            self.section_local(ui, pal, &filter);
            self.section_remotes(ui, pal, &filter);
            self.section_tags(ui, pal, &filter);
            self.section_submodules(ui, pal, &filter);
            self.section_worktrees(ui, pal, &filter);
        });
        let _ = prefs;
    }

    fn section_header(&mut self, ui: &mut egui::Ui, pal: &Palette, key: &str, glyph: &str, title: &str, count: usize, buttons: impl FnOnce(&mut Self, &mut egui::Ui)) -> bool {
        let open = self.expanded.contains(key) || !self.sidebar_filter.is_empty();
        let resp = widgets::row(ui, false, ROW_H + 2.0, pal, |ui| {
            ui.label(RichText::new(if open { "⏷" } else { "⏵" }).color(pal.fg2).size(11.0));
            ui.label(RichText::new(glyph).color(pal.fg2));
            ui.label(RichText::new(format!("{title} ({count})")).strong().color(pal.fg2));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| buttons(self, ui));
        });
        if resp.clicked() {
            if !self.expanded.remove(key) {
                self.expanded.insert(key.to_string());
            }
        }
        open
    }

    fn section_local(&mut self, ui: &mut egui::Ui, pal: &Palette, filter: &str) {
        let branches: Vec<Branch> = self.repo.local_branches().cloned().collect();
        let open = self.section_header(ui, pal, "local", "🖥", "LOCAL BRANCHES", branches.len(), |s, ui| {
            if widgets::small_button(ui, "➕", "Create branch").clicked() {
                let base = s.repo.current_branch().map(|b| b.name.clone()).unwrap_or_else(|| "HEAD".into());
                s.actions.push(Action::Popup(Popup::CreateBranch { base: base.clone(), base_label: base, name: String::new(), checkout: true }));
            }
        });
        if !open {
            return;
        }
        let refs: Vec<&Branch> = branches.iter().filter(|b| filter.is_empty() || b.name.to_lowercase().contains(filter)).collect();
        let tree = build_tree(&refs);
        self.branch_tree(ui, pal, &tree, "local", 1);
    }

    fn branch_tree(&mut self, ui: &mut egui::Ui, pal: &Palette, node: &Folder, prefix: &str, depth: usize) {
        for (name, sub) in &node.folders {
            let key = format!("{prefix}/{name}");
            let open = self.expanded.contains(&key) || !self.sidebar_filter.is_empty();
            let r = widgets::row(ui, false, ROW_H, pal, |ui| {
                ui.add_space(depth as f32 * 14.0);
                ui.label(RichText::new(if open { "⏷" } else { "⏵" }).color(pal.fg2).size(11.0));
                ui.label(RichText::new("📁").color(pal.fg2));
                ui.label(name);
            });
            if r.clicked() && !self.expanded.remove(&key) {
                self.expanded.insert(key.clone());
            }
            if open {
                self.branch_tree(ui, pal, sub, &key, depth + 1);
            }
        }
        for b in &node.branches {
            self.branch_row(ui, pal, b, depth);
        }
    }

    fn branch_row(&mut self, ui: &mut egui::Ui, pal: &Palette, b: &Branch, depth: usize) {
        let short = b.name.rsplit('/').next().unwrap_or(&b.name).to_string();
        let filtered = matches!(&self.repo.scope, HistoryScope::Refs(r) if r.contains(&b.full_name));
        let resp = widgets::row(ui, filtered, ROW_H, pal, |ui| {
            ui.add_space(depth as f32 * 14.0 + 14.0);
            if b.is_current {
                ui.label(RichText::new("✔").color(GRAPH_COLORS[1]));
                ui.label(RichText::new(&short).strong());
            } else {
                ui.label(RichText::new(if b.is_local { "🔀" } else { "🔀" }).color(pal.fg2));
                ui.label(&short);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let t = b.track_text();
                if !t.is_empty() {
                    widgets::badge(ui, &t, pal.badge, pal.badge_fg);
                }
                if !b.worktree_path.is_empty() && !b.is_current {
                    ui.label(RichText::new("🌲").color(pal.fg2)).on_hover_text(format!("Checked out in worktree {}", b.worktree_path));
                }
            });
        });
        let mut tip = format!("{}\nLast commit: {}", b.full_name, widgets::full_time(b.committer_date));
        if b.is_local && !b.upstream.is_empty() {
            tip.push_str(&format!("\nTracking: {}", b.upstream_short()));
        }
        let resp = resp.on_hover_text(tip);
        if resp.clicked() {
            self.actions.push(Action::SelectCommit(b.head.clone()));
        }
        if resp.double_clicked() && b.is_local && !b.is_current {
            let n = b.name.clone();
            self.actions.push(Action::Job(Job::git(format!("Checkout {n}"), move |p| ops::checkout(p, &n))));
        } else if resp.double_clicked() && !b.is_local {
            self.checkout_remote(b);
        }
        let bc = b.clone();
        resp.context_menu(|ui| self.branch_menu(ui, &bc));
    }

    pub fn checkout_remote(&mut self, b: &Branch) {
        // If a local branch already tracks it, just check that out.
        if let Some(local) = self.repo.local_branches().find(|l| l.upstream == b.full_name) {
            let n = local.name.clone();
            if !local.is_current {
                self.actions.push(Action::Job(Job::git(format!("Checkout {n}"), move |p| ops::checkout(p, &n))));
            }
            return;
        }
        self.actions.push(Action::Popup(Popup::CheckoutRemote { remote_branch: b.friendly_name(), local: b.name.clone() }));
    }

    pub fn branch_menu(&mut self, ui: &mut egui::Ui, b: &Branch) {
        let cur = self.repo.current_branch().cloned();
        let cur_name = cur.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "HEAD".into());
        let fname = b.friendly_name();
        if b.is_local {
            if b.is_current {
                if ui.button("Fetch…").clicked() {
                    self.open_fetch();
                    ui.close_menu();
                }
                if !b.upstream.is_empty() && ui.button(format!("Pull from {}…", b.upstream_short())).clicked() {
                    self.open_pull();
                    ui.close_menu();
                }
                if ui.button(format!("Push {}…", b.name)).clicked() {
                    self.open_push(Some(&b.name));
                    ui.close_menu();
                }
            } else {
                if ui.button(format!("Checkout {}", b.name)).clicked() {
                    let n = b.name.clone();
                    self.actions.push(Action::Job(Job::git(format!("Checkout {n}"), move |p| ops::checkout(p, &n))));
                    ui.close_menu();
                }
                if !b.upstream.is_empty() && !b.behind.is_empty() && b.ahead.is_empty() && ui.button(format!("Fast-forward to {}", b.upstream_short())).clicked() {
                    let (n, u) = (b.name.clone(), b.upstream_short());
                    self.actions.push(Action::Job(Job::git(format!("Fast-forward {n}"), move |p| ops::fast_forward(p, &n, &u))));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(format!("Merge {} into {}…", b.name, cur_name)).clicked() {
                    self.actions.push(Action::Popup(Popup::Merge { source: b.name.clone(), into: cur_name.clone(), mode: ops::MergeMode::Default, message: String::new() }));
                    ui.close_menu();
                }
                if ui.button(format!("Rebase {} on {}…", cur_name, b.name)).clicked() {
                    self.actions.push(Action::Popup(Popup::Rebase { onto: b.name.clone(), label: b.name.clone(), current: cur_name.clone(), autostash: true }));
                    ui.close_menu();
                }
                if ui.button(format!("Push {}…", b.name)).clicked() {
                    self.open_push(Some(&b.name));
                    ui.close_menu();
                }
            }
            ui.separator();
            if ui.button(format!("Compare with {cur_name}")).clicked() {
                self.actions.push(Action::OpenCompare { from: cur_name.clone(), to: b.name.clone(), from_label: cur_name.clone(), to_label: b.name.clone() });
                ui.close_menu();
            }
            if ui.button("Rename…").clicked() {
                self.actions.push(Action::Popup(Popup::RenameBranch { old: b.name.clone(), new: b.name.clone() }));
                ui.close_menu();
            }
            if !b.is_current && ui.button("Delete…").clicked() {
                self.actions.push(Action::Popup(Popup::DeleteBranch { branch: b.clone(), force: false, delete_upstream: false }));
                ui.close_menu();
            }
            if ui.button("Set tracking branch…").clicked() {
                let mut options = vec!["(none — unset upstream)".to_string()];
                options.extend(self.repo.data.branches.iter().filter(|x| !x.is_local).map(|x| x.friendly_name()));
                let selected = options.iter().position(|o| *o == b.upstream_short()).unwrap_or(0);
                self.actions.push(Action::Popup(Popup::SetUpstream { branch: b.name.clone(), options, selected }));
                ui.close_menu();
            }
        } else {
            if ui.button(format!("Checkout {fname}…")).clicked() {
                self.checkout_remote(b);
                ui.close_menu();
            }
            ui.separator();
            if ui.button(format!("Merge {fname} into {cur_name}…")).clicked() {
                self.actions.push(Action::Popup(Popup::Merge { source: fname.clone(), into: cur_name.clone(), mode: ops::MergeMode::Default, message: String::new() }));
                ui.close_menu();
            }
            if ui.button(format!("Rebase {cur_name} on {fname}…")).clicked() {
                self.actions.push(Action::Popup(Popup::Rebase { onto: fname.clone(), label: fname.clone(), current: cur_name.clone(), autostash: true }));
                ui.close_menu();
            }
            if ui.button(format!("Compare with {cur_name}")).clicked() {
                self.actions.push(Action::OpenCompare { from: cur_name.clone(), to: fname.clone(), from_label: cur_name.clone(), to_label: fname.clone() });
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Delete…").clicked() {
                self.actions.push(Action::Popup(Popup::DeleteBranch { branch: b.clone(), force: false, delete_upstream: false }));
                ui.close_menu();
            }
        }
        ui.separator();
        if ui.button("Create new branch from here…").clicked() {
            self.actions.push(Action::Popup(Popup::CreateBranch { base: fname.clone(), base_label: fname.clone(), name: String::new(), checkout: true }));
            ui.close_menu();
        }
        if ui.button("Create tag here…").clicked() {
            let remotes: Vec<String> = self.repo.data.remotes.iter().map(|r| r.name.clone()).collect();
            self.actions.push(Action::Popup(Popup::CreateTag { rev: b.head.clone(), label: fname.clone(), name: String::new(), annotated: true, message: String::new(), push: false, remotes, remote: 0 }));
            ui.close_menu();
        }
        if ui.button("Archive…").clicked() {
            self.actions.push(Action::Popup(Popup::Archive { rev: fname.clone(), label: fname.clone(), file: String::new() }));
            ui.close_menu();
        }
        let filtered = matches!(&self.repo.scope, HistoryScope::Refs(r) if r.contains(&b.full_name));
        if ui.button(if filtered { "Show all branches in history" } else { "Show only this branch in history" }).clicked() {
            self.actions.push(Action::SetScope(if filtered { HistoryScope::All } else { HistoryScope::Refs(vec![b.full_name.clone()]) }));
            ui.close_menu();
        }
        if ui.button("Copy branch name").clicked() {
            widgets::copy(ui, &fname);
            ui.close_menu();
        }
    }

    fn section_remotes(&mut self, ui: &mut egui::Ui, pal: &Palette, filter: &str) {
        let remotes = self.repo.data.remotes.clone();
        let open = self.section_header(ui, pal, "remotes", "🌐", "REMOTES", remotes.len(), |s, ui| {
            if widgets::small_button(ui, "➕", "Add remote").clicked() {
                s.actions.push(Action::Popup(Popup::AddRemote { name: String::new(), url: String::new() }));
            }
        });
        if !open {
            return;
        }
        for r in remotes {
            let key = format!("remote/{}", r.name);
            let ropen = self.expanded.contains(&key) || !self.sidebar_filter.is_empty();
            let resp = widgets::row(ui, false, ROW_H, pal, |ui| {
                ui.add_space(14.0);
                ui.label(RichText::new(if ropen { "⏷" } else { "⏵" }).color(pal.fg2).size(11.0));
                ui.label(RichText::new("☁").color(pal.fg2));
                ui.label(&r.name);
            });
            let resp = resp.on_hover_text(&r.url);
            if resp.clicked() && !self.expanded.remove(&key) {
                self.expanded.insert(key.clone());
            }
            let rc = r.clone();
            resp.context_menu(|ui| {
                if ui.button("Fetch").clicked() {
                    let n = rc.name.clone();
                    self.actions.push(Action::Job(Job::git(format!("Fetch {n}"), move |p| ops::fetch(p, Some(&n), true, false))));
                    ui.close_menu();
                }
                if ui.button("Prune").clicked() {
                    let n = rc.name.clone();
                    self.actions.push(Action::Job(Job::git(format!("Prune {n}"), move |p| ops::prune_remote(p, &n))));
                    ui.close_menu();
                }
                if let Some(url) = rc.web_url() {
                    if ui.button("Open in browser").clicked() {
                        widgets::open_url(&url);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Edit…").clicked() {
                    self.actions.push(Action::Popup(Popup::EditRemote { old_name: rc.name.clone(), name: rc.name.clone(), url: rc.url.clone(), push_url: if rc.push_url == rc.url { String::new() } else { rc.push_url.clone() } }));
                    ui.close_menu();
                }
                if ui.button("Delete…").clicked() {
                    let n = rc.name.clone();
                    self.actions.push(Action::Popup(Popup::Confirm {
                        title: "Delete Remote".into(),
                        message: format!("Remove remote '{n}' and its remote-tracking branches?"),
                        danger: true,
                        job: Some(Job::git(format!("Delete remote {n}"), move |p| ops::delete_remote(p, &n))),
                    }));
                    ui.close_menu();
                }
                if ui.button("Copy URL").clicked() {
                    widgets::copy(ui, &rc.url);
                    ui.close_menu();
                }
            });
            if ropen {
                let branches: Vec<Branch> = self
                    .repo
                    .remote_branches(&r.name)
                    .into_iter()
                    .filter(|b| filter.is_empty() || b.friendly_name().to_lowercase().contains(filter))
                    .cloned()
                    .collect();
                let refs: Vec<&Branch> = branches.iter().collect();
                let tree = build_tree(&refs);
                self.branch_tree(ui, pal, &tree, &key, 2);
            }
        }
    }

    fn section_tags(&mut self, ui: &mut egui::Ui, pal: &Palette, filter: &str) {
        let tags = self.repo.data.tags.clone();
        let open = self.section_header(ui, pal, "tags", "🏷", "TAGS", tags.len(), |s, ui| {
            if widgets::small_button(ui, "➕", "Create tag at HEAD").clicked() {
                let remotes: Vec<String> = s.repo.data.remotes.iter().map(|r| r.name.clone()).collect();
                s.actions.push(Action::Popup(Popup::CreateTag { rev: "HEAD".into(), label: "HEAD".into(), name: String::new(), annotated: true, message: String::new(), push: false, remotes, remote: 0 }));
            }
        });
        if !open {
            return;
        }
        for t in tags.iter().filter(|t| filter.is_empty() || t.name.to_lowercase().contains(filter)) {
            let resp = widgets::row(ui, false, ROW_H, pal, |ui| {
                ui.add_space(28.0);
                ui.label(RichText::new("🏷").color(pal.fg2));
                ui.label(&t.name);
            });
            let tip = if t.is_annotated {
                format!("{}\n{} — {}\n\n{}", t.name, t.tagger.name, widgets::full_time(t.date), t.message)
            } else {
                format!("{} (lightweight)\n{}", t.name, widgets::full_time(t.date))
            };
            let resp = resp.on_hover_text(tip);
            if resp.clicked() {
                self.actions.push(Action::SelectCommit(t.sha.clone()));
            }
            let tc = t.clone();
            resp.context_menu(|ui| {
                let remotes: Vec<String> = self.repo.data.remotes.iter().map(|r| r.name.clone()).collect();
                if !remotes.is_empty() && ui.button("Push…").clicked() {
                    self.actions.push(Action::Popup(Popup::PushTag { name: tc.name.clone(), remotes: remotes.clone(), remote: 0 }));
                    ui.close_menu();
                }
                if ui.button("Delete…").clicked() {
                    self.actions.push(Action::Popup(Popup::DeleteTag { name: tc.name.clone(), remotes: remotes.clone(), remote: 0, delete_remote: false }));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Create new branch from here…").clicked() {
                    self.actions.push(Action::Popup(Popup::CreateBranch { base: tc.name.clone(), base_label: tc.name.clone(), name: String::new(), checkout: true }));
                    ui.close_menu();
                }
                let cur = self.repo.current_branch().map(|b| b.name.clone()).unwrap_or_else(|| "HEAD".into());
                if ui.button(format!("Merge {} into {}…", tc.name, cur)).clicked() {
                    self.actions.push(Action::Popup(Popup::Merge { source: tc.name.clone(), into: cur.clone(), mode: ops::MergeMode::Default, message: String::new() }));
                    ui.close_menu();
                }
                if ui.button("Archive…").clicked() {
                    self.actions.push(Action::Popup(Popup::Archive { rev: tc.name.clone(), label: tc.name.clone(), file: String::new() }));
                    ui.close_menu();
                }
                if ui.button("Copy tag name").clicked() {
                    widgets::copy(ui, &tc.name);
                    ui.close_menu();
                }
            });
        }
    }

    fn section_submodules(&mut self, ui: &mut egui::Ui, pal: &Palette, filter: &str) {
        let subs = self.repo.data.submodules.clone();
        let open = self.section_header(ui, pal, "submodules", "📦", "SUBMODULES", subs.len(), |s, ui| {
            if widgets::small_button(ui, "➕", "Add submodule").clicked() {
                s.actions.push(Action::Popup(Popup::AddSubmodule { url: String::new(), path: String::new(), recursive: true }));
            }
            if widgets::small_button(ui, "⟳", "Update submodules").clicked() {
                s.actions.push(Action::Popup(Popup::UpdateSubmodules { paths: vec![], init: true, recursive: true, remote: false }));
            }
        });
        if !open {
            return;
        }
        for s in subs.iter().filter(|s| filter.is_empty() || s.path.to_lowercase().contains(filter)) {
            let resp = widgets::row(ui, false, ROW_H, pal, |ui| {
                ui.add_space(28.0);
                ui.label(RichText::new("📦").color(pal.fg2));
                ui.label(&s.path);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (t, c) = match s.status {
                        SubmoduleStatus::Normal => ("", pal.fg2),
                        SubmoduleStatus::NotInitialized => ("not init", pal.fg2),
                        SubmoduleStatus::RevisionChanged => ("rev changed", egui::Color32::from_rgb(0xE0, 0x9B, 0)),
                        SubmoduleStatus::Unmerged => ("unmerged", egui::Color32::LIGHT_RED),
                        SubmoduleStatus::Modified => ("modified", egui::Color32::from_rgb(0xE0, 0x9B, 0)),
                    };
                    if !t.is_empty() {
                        ui.label(RichText::new(t).size(11.0).color(c));
                    }
                });
            });
            let resp = resp.on_hover_text(format!("{}\n{}\n{}", s.path, s.url, s.sha));
            if resp.double_clicked() && s.status != SubmoduleStatus::NotInitialized {
                self.actions.push(Action::OpenRepo(self.repo.path.join(&s.path)));
            }
            let sc = s.clone();
            resp.context_menu(|ui| {
                if sc.status != SubmoduleStatus::NotInitialized && ui.button("Open submodule repository").clicked() {
                    self.actions.push(Action::OpenRepo(self.repo.path.join(&sc.path)));
                    ui.close_menu();
                }
                if ui.button("Update…").clicked() {
                    self.actions.push(Action::Popup(Popup::UpdateSubmodules { paths: vec![sc.path.clone()], init: true, recursive: true, remote: false }));
                    ui.close_menu();
                }
                if ui.button("Deinit").clicked() {
                    let p2 = sc.path.clone();
                    self.actions.push(Action::Job(Job::git("Deinit submodule", move |p| ops::submodule_deinit(p, &p2))));
                    ui.close_menu();
                }
                if ui.button("Delete…").clicked() {
                    let p2 = sc.path.clone();
                    self.actions.push(Action::Popup(Popup::Confirm {
                        title: "Delete Submodule".into(),
                        message: format!("Delete submodule '{}'?", sc.path),
                        danger: true,
                        job: Some(Job::git("Delete submodule", move |p| ops::submodule_delete(p, &p2))),
                    }));
                    ui.close_menu();
                }
                if ui.button("Copy path").clicked() {
                    widgets::copy(ui, &sc.path);
                    ui.close_menu();
                }
            });
        }
    }

    fn section_worktrees(&mut self, ui: &mut egui::Ui, pal: &Palette, filter: &str) {
        let wts = self.repo.data.worktrees.clone();
        let open = self.section_header(ui, pal, "worktrees", "🌲", "WORKTREES", wts.len(), |s, ui| {
            if widgets::small_button(ui, "➕", "Add worktree").clicked() {
                s.actions.push(Action::Popup(Popup::AddWorktree { path: String::new(), branch: String::new(), new_branch: true, tracking: String::new() }));
            }
            if widgets::small_button(ui, "✂", "Prune worktrees").clicked() {
                s.actions.push(Action::Job(Job::git("Prune worktrees", ops::worktree_prune)));
            }
        });
        if !open {
            return;
        }
        for w in wts.iter().filter(|w| filter.is_empty() || w.path.to_lowercase().contains(filter)) {
            let resp = widgets::row(ui, false, ROW_H, pal, |ui| {
                ui.add_space(28.0);
                ui.label(RichText::new(if w.is_locked { "🔒" } else { "🌲" }).color(pal.fg2));
                ui.label(widgets::truncate_middle(&w.path, 36));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let b = if w.is_detached { format!("detached {}", &w.head[..w.head.len().min(10)]) } else { w.branch_short() };
                    ui.label(RichText::new(b).size(11.0).color(pal.fg2));
                });
            });
            let resp = resp.on_hover_text(&w.path);
            if resp.double_clicked() {
                self.actions.push(Action::OpenRepo(w.path.clone().into()));
            }
            let wc = w.clone();
            resp.context_menu(|ui| {
                if ui.button("Open worktree").clicked() {
                    self.actions.push(Action::OpenRepo(wc.path.clone().into()));
                    ui.close_menu();
                }
                let lock = !wc.is_locked;
                if ui.button(if lock { "Lock" } else { "Unlock" }).clicked() {
                    let p2 = wc.path.clone();
                    self.actions.push(Action::Job(Job::git("Lock worktree", move |p| ops::worktree_lock(p, &p2, lock))));
                    ui.close_menu();
                }
                if ui.button("Remove…").clicked() {
                    let p2 = wc.path.clone();
                    self.actions.push(Action::Popup(Popup::Confirm {
                        title: "Remove Worktree".into(),
                        message: format!("Remove worktree '{}'? (forces removal of local changes)", wc.path),
                        danger: true,
                        job: Some(Job::git("Remove worktree", move |p| ops::worktree_remove(p, &p2, true))),
                    }));
                    ui.close_menu();
                }
                if ui.button("Copy path").clicked() {
                    widgets::copy(ui, &wc.path);
                    ui.close_menu();
                }
            });
        }
    }
}
