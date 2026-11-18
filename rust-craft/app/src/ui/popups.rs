//! Modal dialogs for repository operations. Each dialog, when confirmed, produces a `Job`
//! that runs off the UI thread.

use crate::git::ops::{self, MergeMode, PushOptions, ResetMode};
use crate::git::{Branch, Change, Output};
use crate::theme::Palette;
use eframe::egui::{self, RichText};
use std::path::Path;

pub type JobFn = Box<dyn FnOnce(&Path) -> Result<String, String> + Send>;

pub struct Job {
    pub label: String,
    pub run: JobFn,
}

impl Job {
    pub fn new(label: impl Into<String>, f: impl FnOnce(&Path) -> Result<String, String> + Send + 'static) -> Job {
        Job { label: label.into(), run: Box::new(f) }
    }
    pub fn git(label: impl Into<String>, f: impl FnOnce(&Path) -> Output + Send + 'static) -> Job {
        Job::new(label, move |p| f(p).into_result())
    }
}

pub enum PopupResult {
    Keep,
    Close,
    Run(Job),
}

pub enum Popup {
    CreateBranch { base: String, base_label: String, name: String, checkout: bool },
    RenameBranch { old: String, new: String },
    DeleteBranch { branch: Branch, force: bool, delete_upstream: bool },
    CheckoutRemote { remote_branch: String, local: String },
    Merge { source: String, into: String, mode: MergeMode, message: String },
    Rebase { onto: String, label: String, current: String, autostash: bool },
    CherryPick { shas: Vec<String>, labels: Vec<String>, no_commit: bool, append_source: bool, is_merge: bool, mainline: u32 },
    Revert { sha: String, subject: String, auto_commit: bool, is_merge: bool },
    Reset { sha: String, subject: String, branch: String, mode: ResetMode },
    ResetBranch { branch: String, sha: String, subject: String },
    Fetch { remotes: Vec<String>, remote: usize, prune: bool, no_tags: bool },
    Pull { remotes: Vec<String>, remote: usize, branches: Vec<String>, branch: usize, into: String, rebase: bool, autostash: bool },
    Push { locals: Vec<String>, local: usize, remotes: Vec<String>, remote: usize, remote_branch: String, set_upstream: bool, force: bool, tags: bool, no_verify: bool },
    AddRemote { name: String, url: String },
    EditRemote { old_name: String, name: String, url: String, push_url: String },
    CreateTag { rev: String, label: String, name: String, annotated: bool, message: String, push: bool, remotes: Vec<String>, remote: usize },
    DeleteTag { name: String, remotes: Vec<String>, remote: usize, delete_remote: bool },
    PushTag { name: String, remotes: Vec<String>, remote: usize },
    StashPush { message: String, include_untracked: bool, keep_index: bool, only_staged: bool, paths: Vec<String> },
    StashApply { name: String, restore_index: bool, drop_after: bool },
    StashBranch { stash: String, branch: String },
    SetUpstream { branch: String, options: Vec<String>, selected: usize },
    AddSubmodule { url: String, path: String, recursive: bool },
    UpdateSubmodules { paths: Vec<String>, init: bool, recursive: bool, remote: bool },
    AddWorktree { path: String, branch: String, new_branch: bool, tracking: String },
    Archive { rev: String, label: String, file: String },
    ApplyPatch { file: String, whitespace: usize, three_way: bool },
    Discard { changes: Vec<Change>, all: bool, include_untracked: bool, include_ignored: bool },
    RepoConfig { user_name: String, user_email: String, gpg_sign: bool },
    Confirm { title: String, message: String, danger: bool, job: Option<Job> },
}

fn text_row(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) -> egui::Response {
    ui.label(label);
    let r = ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(360.0));
    ui.end_row();
    r
}

fn static_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(label);
    ui.label(RichText::new(value).strong());
    ui.end_row();
}

fn combo(ui: &mut egui::Ui, id: &str, label: &str, items: &[String], sel: &mut usize) {
    ui.label(label);
    egui::ComboBox::from_id_salt(id)
        .width(360.0)
        .selected_text(items.get(*sel).cloned().unwrap_or_default())
        .show_ui(ui, |ui| {
            for (i, it) in items.iter().enumerate() {
                ui.selectable_value(sel, i, it);
            }
        });
    ui.end_row();
}

fn check_row(ui: &mut egui::Ui, value: &mut bool, label: &str) {
    ui.label("");
    ui.checkbox(value, label);
    ui.end_row();
}

pub fn valid_ref_name(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty()
        && !s.starts_with('-')
        && !s.ends_with('/')
        && !s.ends_with(".lock")
        && !s.contains("..")
        && !s.contains("@{")
        && !s.chars().any(|c| c.is_whitespace() || "~^:?*[\\".contains(c))
}

impl Popup {
    pub fn title(&self) -> String {
        match self {
            Popup::CreateBranch { .. } => "Create Branch".into(),
            Popup::RenameBranch { .. } => "Rename Branch".into(),
            Popup::DeleteBranch { .. } => "Delete Branch".into(),
            Popup::CheckoutRemote { .. } => "Checkout Remote Branch".into(),
            Popup::Merge { .. } => "Merge".into(),
            Popup::Rebase { .. } => "Rebase Current Branch".into(),
            Popup::CherryPick { .. } => "Cherry-Pick".into(),
            Popup::Revert { .. } => "Revert Commit".into(),
            Popup::Reset { .. } => "Reset Current Branch To Revision".into(),
            Popup::ResetBranch { .. } => "Reset Branch (without checkout)".into(),
            Popup::Fetch { .. } => "Fetch".into(),
            Popup::Pull { .. } => "Pull".into(),
            Popup::Push { .. } => "Push".into(),
            Popup::AddRemote { .. } => "Add Remote".into(),
            Popup::EditRemote { .. } => "Edit Remote".into(),
            Popup::CreateTag { .. } => "Create Tag".into(),
            Popup::DeleteTag { .. } => "Delete Tag".into(),
            Popup::PushTag { .. } => "Push Tag".into(),
            Popup::StashPush { .. } => "Stash Local Changes".into(),
            Popup::StashApply { .. } => "Apply Stash".into(),
            Popup::StashBranch { .. } => "Create Branch From Stash".into(),
            Popup::SetUpstream { .. } => "Set Tracking Branch".into(),
            Popup::AddSubmodule { .. } => "Add Submodule".into(),
            Popup::UpdateSubmodules { .. } => "Update Submodules".into(),
            Popup::AddWorktree { .. } => "Add Worktree".into(),
            Popup::Archive { .. } => "Archive".into(),
            Popup::ApplyPatch { .. } => "Apply Patch".into(),
            Popup::Discard { .. } => "Discard Changes".into(),
            Popup::RepoConfig { .. } => "Repository Configuration".into(),
            Popup::Confirm { title, .. } => title.clone(),
        }
    }

    /// Draws the form; returns a job when the user confirms.
    pub fn show(&mut self, ctx: &egui::Context, pal: &Palette, repo: &Path) -> PopupResult {
        let mut result = PopupResult::Keep;
        let title = self.title();
        let modal = egui::Modal::new(egui::Id::new("repo-popup")).show(ctx, |ui| {
            ui.set_min_width(520.0);
            ui.label(RichText::new(title).size(16.0).strong());
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            let (can_ok, ok_label) = egui::Grid::new("popup-grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| self.form(ui, pal, repo))
                .inner;
            ui.add_space(10.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancel").clicked() {
                        result = PopupResult::Close;
                    }
                    let ok = ui.add_enabled(can_ok, egui::Button::new(RichText::new(ok_label).strong()));
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift) && can_ok;
                    let is_multiline = matches!(self, Popup::CreateTag { annotated: true, .. } | Popup::Merge { .. });
                    if ok.clicked() || (enter && !is_multiline) {
                        if let Some(job) = self.make_job(repo) {
                            result = PopupResult::Run(job);
                        } else {
                            result = PopupResult::Close;
                        }
                    }
                });
            });
        });
        if modal.should_close() && matches!(result, PopupResult::Keep) {
            result = PopupResult::Close;
        }
        result
    }

    /// Returns (ok enabled, ok label).
    fn form(&mut self, ui: &mut egui::Ui, pal: &Palette, _repo: &Path) -> (bool, &'static str) {
        match self {
            Popup::CreateBranch { base_label, name, checkout, .. } => {
                static_row(ui, "Based on:", base_label);
                text_row(ui, "New branch name:", name, "Enter branch name").request_focus_once(ui);
                check_row(ui, checkout, "Check out after created");
                let ok = valid_ref_name(name);
                if !name.is_empty() && !ok {
                    ui.label("");
                    ui.colored_label(egui::Color32::LIGHT_RED, "Invalid branch name");
                    ui.end_row();
                }
                (ok, "Create")
            }
            Popup::RenameBranch { old, new } => {
                static_row(ui, "Branch:", old);
                text_row(ui, "New name:", new, "Unique name for this branch").request_focus_once(ui);
                (valid_ref_name(new) && new != old, "Rename")
            }
            Popup::DeleteBranch { branch, force, delete_upstream } => {
                static_row(ui, "Branch:", &branch.friendly_name());
                if branch.is_local {
                    check_row(ui, force, "Force delete (even if not merged)");
                    if !branch.upstream.is_empty() {
                        let up = branch.upstream_short();
                        check_row(ui, delete_upstream, &format!("Also delete remote branch {up}"));
                    }
                }
                (true, "Delete")
            }
            Popup::CheckoutRemote { remote_branch, local } => {
                static_row(ui, "Remote branch:", remote_branch);
                text_row(ui, "Local branch:", local, "Local branch name");
                (valid_ref_name(local), "Checkout")
            }
            Popup::Merge { source, into, mode, message } => {
                static_row(ui, "Merge from:", source);
                static_row(ui, "Into:", into);
                ui.label("Mode:");
                egui::ComboBox::from_id_salt("merge-mode").width(360.0).selected_text(mode.label()).show_ui(ui, |ui| {
                    for m in MergeMode::ALL {
                        ui.selectable_value(mode, m, m.label());
                    }
                });
                ui.end_row();
                ui.label("Message:");
                ui.add(egui::TextEdit::multiline(message).hint_text("(optional) custom merge message").desired_rows(2).desired_width(360.0));
                ui.end_row();
                (true, "Merge")
            }
            Popup::Rebase { label, current, autostash, .. } => {
                static_row(ui, "Rebase:", current);
                static_row(ui, "On:", label);
                check_row(ui, autostash, "Stash & reapply local changes");
                (true, "Rebase")
            }
            Popup::CherryPick { labels, no_commit, append_source, is_merge, mainline, .. } => {
                ui.label("Commit(s):");
                ui.vertical(|ui| {
                    for l in labels.iter() {
                        ui.label(RichText::new(l).monospace());
                    }
                });
                ui.end_row();
                if *is_merge {
                    ui.label("Mainline:");
                    ui.add(egui::DragValue::new(mainline).range(1..=8));
                    ui.end_row();
                }
                check_row(ui, append_source, "Append source to commit message (-x)");
                check_row(ui, no_commit, "Don't commit (stage only)");
                (true, "Cherry-Pick")
            }
            Popup::Revert { subject, auto_commit, .. } => {
                static_row(ui, "Commit:", subject);
                check_row(ui, auto_commit, "Commit the revert changes");
                (true, "Revert")
            }
            Popup::Reset { subject, branch, mode, .. } => {
                static_row(ui, "Current branch:", branch);
                static_row(ui, "Move to:", subject);
                ui.label("Reset mode:");
                ui.vertical(|ui| {
                    for m in ResetMode::ALL {
                        ui.radio_value(mode, m, m.desc());
                    }
                });
                ui.end_row();
                if *mode == ResetMode::Hard {
                    ui.label("");
                    ui.colored_label(egui::Color32::from_rgb(0xFF, 0x80, 0x60), "All uncommitted changes will be lost!");
                    ui.end_row();
                }
                (true, "Reset")
            }
            Popup::ResetBranch { branch, subject, .. } => {
                static_row(ui, "Branch:", branch);
                static_row(ui, "Move to:", subject);
                (true, "Reset")
            }
            Popup::Fetch { remotes, remote, prune, no_tags } => {
                let mut items = vec!["(all remotes)".to_string()];
                items.extend(remotes.iter().cloned());
                combo(ui, "fetch-remote", "Remote:", &items, remote);
                check_row(ui, prune, "Prune remote dead branches");
                check_row(ui, no_tags, "Don't fetch tags");
                (true, "Fetch")
            }
            Popup::Pull { remotes, remote, branches, branch, into, rebase, autostash } => {
                combo(ui, "pull-remote", "Remote:", remotes, remote);
                combo(ui, "pull-branch", "Branch:", branches, branch);
                static_row(ui, "Into:", into);
                check_row(ui, rebase, "Use rebase instead of merge");
                check_row(ui, autostash, "Stash & reapply local changes");
                (!remotes.is_empty(), "Pull")
            }
            Popup::Push { locals, local, remotes, remote, remote_branch, set_upstream, force, tags, no_verify } => {
                let before = *local;
                combo(ui, "push-local", "Local branch:", locals, local);
                if before != *local {
                    *remote_branch = locals.get(*local).cloned().unwrap_or_default();
                }
                combo(ui, "push-remote", "Remote:", remotes, remote);
                text_row(ui, "Remote branch:", remote_branch, "Remote branch name");
                check_row(ui, set_upstream, "Set as tracking branch (-u)");
                check_row(ui, tags, "Push all tags");
                check_row(ui, force, "Force push (--force-with-lease)");
                check_row(ui, no_verify, "Skip hooks (--no-verify)");
                (!remotes.is_empty() && !locals.is_empty() && valid_ref_name(remote_branch), "Push")
            }
            Popup::AddRemote { name, url } => {
                text_row(ui, "Name:", name, "Remote name").request_focus_once(ui);
                text_row(ui, "Repository URL:", url, "Remote git repository URL");
                (valid_ref_name(name) && !url.trim().is_empty(), "Add")
            }
            Popup::EditRemote { name, url, push_url, .. } => {
                text_row(ui, "Name:", name, "Remote name");
                text_row(ui, "Fetch URL:", url, "Remote git repository URL");
                text_row(ui, "Push URL:", push_url, "(optional) separate push URL");
                (valid_ref_name(name) && !url.trim().is_empty(), "Save")
            }
            Popup::CreateTag { label, name, annotated, message, push, remotes, remote, .. } => {
                static_row(ui, "New tag at:", label);
                text_row(ui, "Tag name:", name, "Recommended format: v1.0.0-alpha").request_focus_once(ui);
                ui.label("Kind:");
                ui.horizontal(|ui| {
                    ui.radio_value(annotated, true, "Annotated");
                    ui.radio_value(annotated, false, "Lightweight");
                });
                ui.end_row();
                if *annotated {
                    ui.label("Message:");
                    ui.add(egui::TextEdit::multiline(message).hint_text("Optional tag message").desired_rows(3).desired_width(360.0));
                    ui.end_row();
                }
                if !remotes.is_empty() {
                    check_row(ui, push, "Push to remote after created");
                    if *push {
                        combo(ui, "tag-remote", "Remote:", remotes, remote);
                    }
                }
                (valid_ref_name(name), "Create")
            }
            Popup::DeleteTag { name, remotes, remote, delete_remote } => {
                static_row(ui, "Tag:", name);
                if !remotes.is_empty() {
                    check_row(ui, delete_remote, "Also delete from remote");
                    if *delete_remote {
                        combo(ui, "deltag-remote", "Remote:", remotes, remote);
                    }
                }
                (true, "Delete")
            }
            Popup::PushTag { name, remotes, remote } => {
                static_row(ui, "Tag:", name);
                combo(ui, "pushtag-remote", "Remote:", remotes, remote);
                (!remotes.is_empty(), "Push")
            }
            Popup::StashPush { message, include_untracked, keep_index, only_staged, paths } => {
                if !paths.is_empty() {
                    static_row(ui, "Files:", &format!("{} selected", paths.len()));
                }
                text_row(ui, "Message:", message, "Optional. Name of this stash").request_focus_once(ui);
                if paths.is_empty() {
                    check_row(ui, only_staged, "Only staged changes");
                    if !*only_staged {
                        check_row(ui, include_untracked, "Include untracked files");
                    }
                }
                check_row(ui, keep_index, "Keep staged changes in the index");
                (true, "Stash")
            }
            Popup::StashApply { name, restore_index, drop_after } => {
                static_row(ui, "Stash:", name);
                check_row(ui, restore_index, "Reinstate index changes");
                check_row(ui, drop_after, "Drop stash after applied");
                (true, "Apply")
            }
            Popup::StashBranch { stash, branch } => {
                static_row(ui, "Stash:", stash);
                text_row(ui, "Branch name:", branch, "New branch").request_focus_once(ui);
                (valid_ref_name(branch), "Create")
            }
            Popup::SetUpstream { branch, options, selected } => {
                static_row(ui, "Branch:", branch);
                combo(ui, "upstream", "Upstream:", options, selected);
                (true, "Save")
            }
            Popup::AddSubmodule { url, path, recursive } => {
                text_row(ui, "URL:", url, "Remote repository URL").request_focus_once(ui);
                text_row(ui, "Relative path:", path, "Relative path to store this module");
                check_row(ui, recursive, "Initialize nested submodules");
                (!url.trim().is_empty() && !path.trim().is_empty(), "Add")
            }
            Popup::UpdateSubmodules { paths, init, recursive, remote } => {
                static_row(ui, "Submodules:", &if paths.is_empty() { "All".to_string() } else { paths.join(", ") });
                check_row(ui, init, "Initialize if needed");
                check_row(ui, recursive, "Recursive");
                check_row(ui, remote, "Update to remote tracking branch (--remote)");
                (true, "Update")
            }
            Popup::AddWorktree { path, branch, new_branch, tracking } => {
                ui.label("Location:");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(path).hint_text("Path for this worktree").desired_width(300.0));
                    if ui.button("…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            *path = p.display().to_string();
                        }
                    }
                });
                ui.end_row();
                ui.label("Branch:");
                ui.horizontal(|ui| {
                    ui.radio_value(new_branch, true, "Create new branch");
                    ui.radio_value(new_branch, false, "Existing branch");
                });
                ui.end_row();
                text_row(ui, "Branch name:", branch, if *new_branch { "New branch name (optional)" } else { "Existing branch" });
                if *new_branch {
                    text_row(ui, "Start point:", tracking, "(optional) e.g. origin/main");
                }
                (!path.trim().is_empty(), "Add")
            }
            Popup::Archive { label, file, .. } => {
                static_row(ui, "Revision:", label);
                ui.label("Save file to:");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(file).desired_width(300.0));
                    if ui.button("…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("zip", &["zip"]).set_file_name("archive.zip").save_file() {
                            *file = p.display().to_string();
                        }
                    }
                });
                ui.end_row();
                (!file.trim().is_empty(), "Save")
            }
            Popup::ApplyPatch { file, whitespace, three_way } => {
                ui.label("Patch file:");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(file).desired_width(300.0));
                    if ui.button("…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("patch", &["patch", "diff"]).pick_file() {
                            *file = p.display().to_string();
                        }
                    }
                });
                ui.end_row();
                let items = ["nowarn", "warn", "error", "fix"].map(String::from).to_vec();
                combo(ui, "ws", "Whitespace:", &items, whitespace);
                check_row(ui, three_way, "3-way merge (--3way)");
                (!file.trim().is_empty(), "Apply")
            }
            Popup::Discard { changes, all, include_untracked, include_ignored } => {
                if *all {
                    static_row(ui, "Changes:", "All local changes in working copy");
                    check_row(ui, include_untracked, "Include untracked files");
                    if *include_untracked {
                        check_row(ui, include_ignored, "Include ignored files");
                    }
                } else if changes.len() == 1 {
                    static_row(ui, "File:", &changes[0].path);
                } else {
                    static_row(ui, "Changes:", &format!("{} files", changes.len()));
                }
                ui.label("");
                ui.colored_label(egui::Color32::from_rgb(0xFF, 0x80, 0x60), "You can't undo this action!");
                ui.end_row();
                (true, "Discard")
            }
            Popup::RepoConfig { user_name, user_email, gpg_sign } => {
                text_row(ui, "User name:", user_name, "Use global setting");
                text_row(ui, "User email:", user_email, "Use global setting");
                check_row(ui, gpg_sign, "Sign commits with GPG (commit.gpgsign)");
                (true, "Save")
            }
            Popup::Confirm { message, danger, .. } => {
                ui.label("");
                if *danger {
                    ui.colored_label(pal.fg1, RichText::new(message.as_str()).strong());
                } else {
                    ui.label(message.as_str());
                }
                ui.end_row();
                (true, "OK")
            }
        }
    }

    fn make_job(&mut self, _repo: &Path) -> Option<Job> {
        let job = match self {
            Popup::CreateBranch { base, name, checkout, .. } => {
                let (b, n, c) = (base.clone(), name.trim().to_string(), *checkout);
                Job::git(format!("Create branch {n}"), move |p| ops::create_branch(p, &n, &b, c))
            }
            Popup::RenameBranch { old, new } => {
                let (o, n) = (old.clone(), new.trim().to_string());
                Job::git(format!("Rename {o} → {n}"), move |p| ops::rename_branch(p, &o, &n))
            }
            Popup::DeleteBranch { branch, force, delete_upstream } => {
                let b = branch.clone();
                let (f, du) = (*force, *delete_upstream);
                Job::new(format!("Delete branch {}", b.friendly_name()), move |p| {
                    if b.is_local {
                        ops::delete_branch(p, &b.name, f).into_result()?;
                        if du && !b.upstream.is_empty() {
                            let up = b.upstream_short();
                            if let Some((remote, name)) = up.split_once('/') {
                                ops::delete_remote_branch(p, remote, name).into_result()?;
                            }
                        }
                        Ok(String::new())
                    } else {
                        ops::delete_remote_branch(p, &b.remote, &b.name).into_result()
                    }
                })
            }
            Popup::CheckoutRemote { remote_branch, local } => {
                let (r, l) = (remote_branch.clone(), local.trim().to_string());
                Job::git(format!("Checkout {r}"), move |p| ops::checkout_new_tracking(p, &l, &r))
            }
            Popup::Merge { source, mode, message, .. } => {
                let (s, m, msg) = (source.clone(), *mode, message.clone());
                Job::git(format!("Merge {s}"), move |p| ops::merge(p, &s, m, Some(&msg)))
            }
            Popup::Rebase { onto, autostash, .. } => {
                let (o, a) = (onto.clone(), *autostash);
                Job::git("Rebase", move |p| ops::rebase(p, &o, a))
            }
            Popup::CherryPick { shas, no_commit, append_source, is_merge, mainline, .. } => {
                let (s, n, a) = (shas.clone(), *no_commit, *append_source);
                let m = is_merge.then_some(*mainline);
                Job::git("Cherry-pick", move |p| ops::cherry_pick(p, &s, n, a, m))
            }
            Popup::Revert { sha, auto_commit, is_merge, .. } => {
                let (s, a, m) = (sha.clone(), *auto_commit, *is_merge);
                Job::git("Revert", move |p| ops::revert(p, &s, a, m))
            }
            Popup::Reset { sha, mode, .. } => {
                let (s, m) = (sha.clone(), *mode);
                Job::git("Reset", move |p| ops::reset(p, &s, m))
            }
            Popup::ResetBranch { branch, sha, .. } => {
                let (b, s) = (branch.clone(), sha.clone());
                Job::git(format!("Reset {b}"), move |p| ops::reset_branch_to(p, &b, &s))
            }
            Popup::Fetch { remotes, remote, prune, no_tags } => {
                let r = if *remote == 0 { None } else { remotes.get(*remote - 1).cloned() };
                let (pr, nt) = (*prune, *no_tags);
                Job::git("Fetch", move |p| ops::fetch(p, r.as_deref(), pr, nt))
            }
            Popup::Pull { remotes, remote, branches, branch, rebase, autostash, .. } => {
                let r = remotes.get(*remote).cloned().unwrap_or_default();
                let b = branches.get(*branch).cloned().unwrap_or_default();
                let b = b.strip_prefix(&format!("{r}/")).unwrap_or(&b).to_string();
                let (rb, a) = (*rebase, *autostash);
                Job::git("Pull", move |p| ops::pull(p, &r, &b, rb, a))
            }
            Popup::Push { locals, local, remotes, remote, remote_branch, set_upstream, force, tags, no_verify } => {
                let l = locals.get(*local).cloned().unwrap_or_default();
                let r = remotes.get(*remote).cloned().unwrap_or_default();
                let rb = remote_branch.trim().to_string();
                let (su, f, t, nv) = (*set_upstream, *force, *tags, *no_verify);
                Job::git("Push", move |p| {
                    ops::push(p, &PushOptions { remote: &r, local: &l, remote_branch: &rb, set_upstream: su, force: f, tags: t, no_verify: nv })
                })
            }
            Popup::AddRemote { name, url } => {
                let (n, u) = (name.trim().to_string(), url.trim().to_string());
                Job::new(format!("Add remote {n}"), move |p| {
                    ops::add_remote(p, &n, &u).into_result()?;
                    ops::fetch(p, Some(&n), false, false).into_result()
                })
            }
            Popup::EditRemote { old_name, name, url, push_url } => {
                let (o, n, u, pu) = (old_name.clone(), name.trim().to_string(), url.trim().to_string(), push_url.trim().to_string());
                Job::new(format!("Edit remote {o}"), move |p| {
                    if o != n {
                        ops::rename_remote(p, &o, &n).into_result()?;
                    }
                    ops::set_remote_url(p, &n, &u, false).into_result()?;
                    if !pu.is_empty() && pu != u {
                        ops::set_remote_url(p, &n, &pu, true).into_result()?;
                    }
                    Ok(String::new())
                })
            }
            Popup::CreateTag { rev, name, annotated, message, push, remotes, remote, .. } => {
                let (r, n) = (rev.clone(), name.trim().to_string());
                let msg = if *annotated { Some(if message.trim().is_empty() { n.clone() } else { message.clone() }) } else { None };
                let push_to = if *push { remotes.get(*remote).cloned() } else { None };
                Job::new(format!("Create tag {n}"), move |p| {
                    ops::create_tag(p, &n, &r, msg.as_deref(), false).into_result()?;
                    if let Some(rm) = push_to {
                        ops::push_tag(p, &rm, &n).into_result()?;
                    }
                    Ok(String::new())
                })
            }
            Popup::DeleteTag { name, remotes, remote, delete_remote } => {
                let n = name.clone();
                let rm = if *delete_remote { remotes.get(*remote).cloned() } else { None };
                Job::new(format!("Delete tag {n}"), move |p| {
                    ops::delete_tag(p, &n).into_result()?;
                    if let Some(rm) = rm {
                        ops::delete_remote_tag(p, &rm, &n).into_result()?;
                    }
                    Ok(String::new())
                })
            }
            Popup::PushTag { name, remotes, remote } => {
                let (n, r) = (name.clone(), remotes.get(*remote).cloned().unwrap_or_default());
                Job::git(format!("Push tag {n}"), move |p| ops::push_tag(p, &r, &n))
            }
            Popup::StashPush { message, include_untracked, keep_index, only_staged, paths } => {
                let (m, u, k, s, ps) = (message.clone(), *include_untracked, *keep_index, *only_staged, paths.clone());
                Job::git("Stash", move |p| ops::stash_push(p, &m, u, k, s, &ps))
            }
            Popup::StashApply { name, restore_index, drop_after } => {
                let (n, r, d) = (name.clone(), *restore_index, *drop_after);
                Job::new(format!("Apply {n}"), move |p| {
                    ops::stash_apply(p, &n, r).into_result()?;
                    if d {
                        ops::stash_drop(p, &n).into_result()?;
                    }
                    Ok(String::new())
                })
            }
            Popup::StashBranch { stash, branch } => {
                let (s, b) = (stash.clone(), branch.trim().to_string());
                Job::git("Branch from stash", move |p| ops::stash_branch(p, &b, &s))
            }
            Popup::SetUpstream { branch, options, selected } => {
                let b = branch.clone();
                let up = if *selected == 0 { None } else { options.get(*selected).cloned() };
                Job::git("Set upstream", move |p| ops::set_upstream(p, &b, up.as_deref()))
            }
            Popup::AddSubmodule { url, path, recursive } => {
                let (u, pa, r) = (url.trim().to_string(), path.trim().to_string(), *recursive);
                Job::git("Add submodule", move |p| ops::submodule_add(p, &u, &pa, r))
            }
            Popup::UpdateSubmodules { paths, init, recursive, remote } => {
                let (ps, i, r, rm) = (paths.clone(), *init, *recursive, *remote);
                Job::git("Update submodules", move |p| ops::submodule_update(p, &ps, i, r, rm))
            }
            Popup::AddWorktree { path, branch, new_branch, tracking } => {
                let (pa, b, nb, t) = (path.trim().to_string(), branch.trim().to_string(), *new_branch, tracking.trim().to_string());
                Job::git("Add worktree", move |p| ops::worktree_add(p, &pa, &b, nb, Some(&t)))
            }
            Popup::Archive { rev, file, .. } => {
                let (r, f) = (rev.clone(), file.trim().to_string());
                Job::git("Archive", move |p| ops::archive(p, &r, &f))
            }
            Popup::ApplyPatch { file, whitespace, three_way } => {
                let ws = ["nowarn", "warn", "error", "fix"][*whitespace].to_string();
                let (f, t) = (file.trim().to_string(), *three_way);
                Job::git("Apply patch", move |p| ops::apply_patch_file(p, &f, &ws, t))
            }
            Popup::Discard { changes, all, include_untracked, include_ignored } => {
                let (c, a, u, i) = (changes.clone(), *all, *include_untracked, *include_ignored);
                Job::new("Discard", move |p| {
                    if a {
                        ops::discard_all(p, u, i)?;
                    } else {
                        ops::discard(p, &c)?;
                    }
                    Ok(String::new())
                })
            }
            Popup::RepoConfig { user_name, user_email, gpg_sign } => {
                let (n, e, g) = (user_name.trim().to_string(), user_email.trim().to_string(), *gpg_sign);
                Job::new("Save configuration", move |p| {
                    ops::set_config(Some(p), "user.name", &n, false);
                    ops::set_config(Some(p), "user.email", &e, false);
                    ops::set_config(Some(p), "commit.gpgsign", if g { "true" } else { "" }, false);
                    Ok(String::new())
                })
            }
            Popup::Confirm { job, .. } => return job.take(),
        };
        Some(job)
    }
}

trait FocusOnce {
    fn request_focus_once(self, ui: &egui::Ui) -> Self;
}

impl FocusOnce for egui::Response {
    fn request_focus_once(self, ui: &egui::Ui) -> Self {
        let id = egui::Id::new("popup-focused");
        let done = ui.data(|d| d.get_temp::<egui::Id>(id)) == Some(self.id);
        if !done {
            self.request_focus();
            ui.data_mut(|d| d.insert_temp(id, self.id));
        }
        self
    }
}

