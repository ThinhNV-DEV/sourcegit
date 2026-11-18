//! One repository tab: toolbar, left icon bar, sidebar and the three pages.

use super::diff_view::DiffViewer;
use super::popups::{Job, Popup, PopupResult};
use super::widgets;
use crate::consts::*;
use crate::git::{self, ops, parse, Change, Commit, RevisionFile, Stash};
use crate::prefs::Preferences;
use crate::repo::{HistoryScope, LoadOptions, Repository};
use crate::theme::Palette;
use eframe::egui::{self, RichText};
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Histories,
    WorkingCopy,
    Stashes,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DetailTab {
    Info,
    Changes,
    Files,
}

pub struct CommitDetail {
    pub commit: Commit,
    pub message: String,
    pub changes: Vec<Change>,
    pub change_filter: String,
    pub selected_change: Option<usize>,
    pub diff: DiffViewer,
    pub files: Option<Vec<RevisionFile>>,
    pub file_filter: String,
    pub selected_file: Option<String>,
    pub file_content: Option<Result<String, String>>,
}

pub struct SearchState {
    pub open: bool,
    pub method: parse::SearchMethod,
    pub text: String,
    pub only_current: bool,
    pub results: Vec<Commit>,
    pub searched: bool,
}

pub struct WorkingCopy {
    pub sel_unstaged: BTreeSet<String>,
    pub sel_staged: BTreeSet<String>,
    pub focus_staged: bool,
    pub message: String,
    pub amend: bool,
    pub sign_off: bool,
    pub no_verify: bool,
    pub diff: DiffViewer,
    pub filter: String,
    pub last_selected: Option<String>,
}

pub struct StashPage {
    pub selected: Option<usize>,
    pub changes: Vec<Change>,
    pub selected_change: Option<usize>,
    pub diff: DiffViewer,
    pub filter: String,
}

pub struct BlameWin {
    pub path: String,
    pub rev: String,
    pub lines: Result<Vec<git::BlameLine>, String>,
}

pub struct HistoryWin {
    pub path: String,
    pub commits: Vec<Commit>,
    pub selected: Option<usize>,
    pub diff: DiffViewer,
}

pub struct CompareWin {
    pub from: String,
    pub to: String,
    pub from_label: String,
    pub to_label: String,
    pub changes: Vec<Change>,
    pub selected: Option<usize>,
    pub diff: DiffViewer,
    pub filter: String,
}

pub struct RunningTask {
    pub label: String,
    pub rx: Receiver<Result<String, String>>,
    pub started: Instant,
}

pub struct Notice {
    pub text: String,
    pub error: bool,
    pub at: Instant,
}

pub enum Action {
    Popup(Popup),
    Job(Job),
    SelectCommit(String),
    OpenBlame { path: String, rev: String },
    OpenFileHistory(String),
    OpenCompare { from: String, to: String, from_label: String, to_label: String },
    OpenRepo(PathBuf),
    Info(String),
    Error(String),
    SetScope(HistoryScope),
    LoadCommitDiff(usize),
}

/// Requests for the app (outside this tab).
pub enum AppRequest {
    OpenRepo(PathBuf),
}

pub struct RepoView {
    pub repo: Repository,
    pub page: Page,
    pub sidebar_filter: String,
    pub expanded: HashSet<String>,
    pub selected: Vec<usize>,
    pub anchor: Option<usize>,
    pub detail: Option<CommitDetail>,
    pub detail_tab: DetailTab,
    pub scroll_to: Option<usize>,
    pub search: SearchState,
    pub wc: WorkingCopy,
    pub stash: StashPage,
    pub popup: Option<Popup>,
    pub task: Option<RunningTask>,
    pub notices: Vec<Notice>,
    pub blame: Option<BlameWin>,
    pub file_history: Option<HistoryWin>,
    pub compare: Option<CompareWin>,
    pub actions: Vec<Action>,
    pub last_status_poll: Instant,
    /// Defaults copied from preferences each frame (used when opening dialogs).
    pub defaults: (bool, bool, bool),
}

impl RepoView {
    pub fn new(repo: Repository, prefs: &Preferences) -> RepoView {
        let mut expanded = HashSet::new();
        expanded.insert("local".to_string());
        let mut v = RepoView {
            repo,
            page: Page::Histories,
            sidebar_filter: String::new(),
            expanded,
            selected: Vec::new(),
            anchor: None,
            detail: None,
            detail_tab: DetailTab::Info,
            scroll_to: None,
            search: SearchState { open: false, method: parse::SearchMethod::Message, text: String::new(), only_current: false, results: Vec::new(), searched: false },
            wc: WorkingCopy {
                sel_unstaged: BTreeSet::new(),
                sel_staged: BTreeSet::new(),
                focus_staged: false,
                message: String::new(),
                amend: false,
                sign_off: prefs.sign_off,
                no_verify: false,
                diff: DiffViewer::default(),
                filter: String::new(),
                last_selected: None,
            },
            stash: StashPage { selected: None, changes: Vec::new(), selected_change: None, diff: DiffViewer::default(), filter: String::new() },
            popup: None,
            task: None,
            notices: Vec::new(),
            blame: None,
            file_history: None,
            compare: None,
            actions: Vec::new(),
            last_status_poll: Instant::now(),
            defaults: (prefs.fetch_prune, prefs.pull_rebase, prefs.auto_stash),
        };
        if prefs.only_current_branch {
            v.repo.scope = HistoryScope::CurrentBranch;
        }
        v.refresh(prefs);
        // Select HEAD commit initially.
        if let Some(i) = v.repo.data.commits.iter().position(|c| c.is_current_head()) {
            v.select_commit(i, prefs);
        }
        v
    }

    pub fn load_options(prefs: &Preferences) -> LoadOptions {
        LoadOptions {
            max_commits: prefs.max_history_commits,
            show_untracked: prefs.show_untracked,
            first_parent: prefs.first_parent_only,
            topo: prefs.topo_order,
        }
    }

    pub fn refresh(&mut self, prefs: &Preferences) {
        let selected_sha: Vec<String> = self.selected.iter().filter_map(|&i| self.repo.data.commits.get(i).map(|c| c.sha.clone())).collect();
        self.repo.refresh_all(&Self::load_options(prefs));
        // Restore selection by SHA.
        self.selected = selected_sha.iter().filter_map(|s| self.repo.data.commits.iter().position(|c| &c.sha == s)).collect();
        if self.selected.is_empty() {
            self.detail = None;
        }
        // Prune working copy selections.
        let paths: HashSet<String> = self.repo.data.changes.iter().map(|c| c.path.clone()).collect();
        self.wc.sel_unstaged.retain(|p| paths.contains(p));
        self.wc.sel_staged.retain(|p| paths.contains(p));
        self.wc.diff.reload(&self.repo.path, prefs);
        if let Some(t) = &self.wc.diff.target {
            if !paths.contains(t.path()) {
                self.wc.diff.clear();
            }
        }
        if let Some(sel) = self.stash.selected {
            if sel >= self.repo.data.stashes.len() {
                self.stash.selected = None;
                self.stash.changes.clear();
                self.stash.diff.clear();
            }
        }
    }

    pub fn refresh_status(&mut self, prefs: &Preferences) {
        let before = self.repo.data.changes.clone();
        self.repo.refresh_changes(prefs.show_untracked);
        if before != self.repo.data.changes {
            self.wc.diff.reload(&self.repo.path, prefs);
        }
    }

    pub fn select_commit(&mut self, idx: usize, prefs: &Preferences) {
        self.selected = vec![idx];
        self.anchor = Some(idx);
        self.load_detail(idx, prefs);
    }

    pub fn load_detail(&mut self, idx: usize, _prefs: &Preferences) {
        let Some(c) = self.repo.data.commits.get(idx).cloned() else { return };
        let p = &self.repo.path;
        let message = parse::query_full_message(p, &c.sha);
        let changes = parse::query_commit_changes(p, &c.sha, c.parents.first().map(String::as_str));
        let keep_change = self.detail.as_ref().and_then(|d| d.selected_change.and_then(|i| d.changes.get(i)).map(|c| c.path.clone()));
        let selected_change = keep_change.and_then(|p| changes.iter().position(|c| c.path == p));
        self.detail = Some(CommitDetail {
            commit: c,
            message,
            changes,
            change_filter: String::new(),
            selected_change,
            diff: DiffViewer::default(),
            files: None,
            file_filter: String::new(),
            selected_file: None,
            file_content: None,
        });
        if let Some(i) = selected_change {
            self.actions.push(Action::LoadCommitDiff(i));
        }
        if self.detail_tab == DetailTab::Files {
            self.ensure_files();
        }
    }

    pub fn ensure_files(&mut self) {
        let path = self.repo.path.clone();
        if let Some(d) = &mut self.detail {
            if d.files.is_none() {
                d.files = Some(parse::query_revision_files(&path, &d.commit.sha).into_iter().filter(|f| f.kind == "blob").collect());
            }
        }
    }

    pub fn run_job(&mut self, job: Job) {
        if self.task.is_some() {
            self.notices.push(Notice { text: "Another operation is running".into(), error: true, at: Instant::now() });
            return;
        }
        let (tx, rx) = channel();
        let path = self.repo.path.clone();
        let label = job.label.clone();
        let f = job.run;
        std::thread::spawn(move || {
            let r = f(&path);
            let _ = tx.send(r);
        });
        self.task = Some(RunningTask { label, rx, started: Instant::now() });
    }

    fn poll_task(&mut self, ctx: &egui::Context, prefs: &Preferences) {
        let mut done = None;
        if let Some(t) = &self.task {
            match t.rx.try_recv() {
                Ok(r) => done = Some((t.label.clone(), r)),
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                Err(_) => done = Some((t.label.clone(), Err("operation aborted".into()))),
            }
        }
        if let Some((label, r)) = done {
            self.task = None;
            match r {
                Ok(_) => self.notices.push(Notice { text: format!("{label}: done"), error: false, at: Instant::now() }),
                Err(e) => self.notices.push(Notice { text: format!("{label} failed:\n{e}"), error: true, at: Instant::now() }),
            }
            self.refresh(prefs);
        }
    }

    pub fn process_actions(&mut self, prefs: &mut Preferences) -> Vec<AppRequest> {
        let mut out = Vec::new();
        let actions = std::mem::take(&mut self.actions);
        for a in actions {
            match a {
                Action::Popup(p) => self.popup = Some(p),
                Action::Job(j) => self.run_job(j),
                Action::SelectCommit(sha) => {
                    if let Some(i) = self.repo.data.commits.iter().position(|c| c.sha.starts_with(&sha) || sha.starts_with(&c.sha)) {
                        self.page = Page::Histories;
                        self.select_commit(i, prefs);
                        self.scroll_to = Some(i);
                    } else {
                        self.notices.push(Notice { text: format!("Commit {sha} is not in the loaded history"), error: true, at: Instant::now() });
                    }
                }
                Action::OpenBlame { path, rev } => {
                    let lines = parse::blame(&self.repo.path, &path, &rev, prefs.ignore_whitespace);
                    self.blame = Some(BlameWin { path, rev, lines });
                }
                Action::OpenFileHistory(path) => {
                    let commits = parse::query_file_history(&self.repo.path, &path, prefs.max_history_commits);
                    let mut w = HistoryWin { path, commits, selected: None, diff: DiffViewer::default() };
                    if !w.commits.is_empty() {
                        w.selected = Some(0);
                        load_history_diff(&self.repo.path, &mut w, prefs);
                    }
                    self.file_history = Some(w);
                }
                Action::OpenCompare { from, to, from_label, to_label } => {
                    let changes = parse::compare_revisions(&self.repo.path, &from, &to);
                    self.compare = Some(CompareWin { from, to, from_label, to_label, changes, selected: None, diff: DiffViewer::default(), filter: String::new() });
                }
                Action::OpenRepo(p) => out.push(AppRequest::OpenRepo(p)),
                Action::Info(t) => self.notices.push(Notice { text: t, error: false, at: Instant::now() }),
                Action::Error(t) => self.notices.push(Notice { text: t, error: true, at: Instant::now() }),
                Action::SetScope(s) => {
                    self.repo.scope = s;
                    prefs.only_current_branch = self.repo.scope == HistoryScope::CurrentBranch;
                    self.repo.refresh_commits(&Self::load_options(prefs));
                    self.selected.clear();
                    self.detail = None;
                }
                Action::LoadCommitDiff(i) => {
                    let path = self.repo.path.clone();
                    if let Some(d) = &mut self.detail {
                        if let Some(c) = d.changes.get(i).cloned() {
                            d.selected_change = Some(i);
                            let target = ops::DiffTarget::Revisions {
                                from: d.commit.parents.first().cloned().unwrap_or_default(),
                                to: d.commit.sha.clone(),
                                path: c.path.clone(),
                                original: c.original_path.clone(),
                            };
                            d.diff.load(&path, target, prefs);
                        }
                    }
                }
            }
        }
        out
    }

    pub fn ui(&mut self, ctx: &egui::Context, pal: &Palette, prefs: &mut Preferences) -> Vec<AppRequest> {
        self.poll_task(ctx, prefs);
        self.defaults = (prefs.fetch_prune, prefs.pull_rebase, prefs.auto_stash);

        // Cheap status poll so external edits show up (like SourceGit's file watcher).
        if self.task.is_none() && self.popup.is_none() && self.last_status_poll.elapsed().as_secs() >= 5 {
            self.last_status_poll = Instant::now();
            self.refresh_status(prefs);
        }

        egui::TopBottomPanel::top("repo-toolbar")
            .exact_height(36.0)
            .frame(egui::Frame::new().fill(pal.tool_bar).inner_margin(egui::Margin::symmetric(8, 4)))
            .show(ctx, |ui| self.toolbar(ui, pal, prefs));

        egui::SidePanel::left("repo-iconbar")
            .exact_width(LEFT_ICON_BAR_WIDTH)
            .resizable(false)
            .frame(egui::Frame::new().fill(pal.title_bar).inner_margin(egui::Margin::symmetric(4, 8)))
            .show(ctx, |ui| self.icon_bar(ui, pal));

        egui::SidePanel::left("repo-sidebar")
            .default_width(SIDEBAR_WIDTH)
            .width_range(180.0..=600.0)
            .frame(egui::Frame::new().fill(pal.window).inner_margin(egui::Margin::symmetric(6, 6)))
            .show(ctx, |ui| self.sidebar(ui, pal, prefs));

        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.contents)).show(ctx, |ui| {
            if let Some(ip) = self.repo.data.in_progress.clone() {
                self.in_progress_banner(ui, pal, &ip);
            }
            match self.page {
                Page::Histories => self.histories(ui, pal, prefs),
                Page::WorkingCopy => self.working_copy(ui, pal, prefs),
                Page::Stashes => self.stashes(ui, pal, prefs),
            }
        });

        self.windows(ctx, pal, prefs);

        if let Some(p) = &mut self.popup {
            match p.show(ctx, pal, &self.repo.path) {
                PopupResult::Keep => {}
                PopupResult::Close => self.popup = None,
                PopupResult::Run(job) => {
                    self.popup = None;
                    self.run_job(job);
                }
            }
        }

        self.notices_ui(ctx, pal);
        self.process_actions(prefs)
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, pal: &Palette, prefs: &mut Preferences) {
        ui.horizontal_centered(|ui| {
            if widgets::tool_button(ui, "📂", "Open in file manager").clicked() {
                widgets::open_path(&self.repo.path);
            }
            if widgets::tool_button(ui, "🖳", "Open in terminal").clicked() {
                widgets::open_terminal(&self.repo.path);
            }
            if widgets::tool_button(ui, "⚙", "Repository configuration").clicked() {
                let p = &self.repo.path;
                self.popup = Some(Popup::RepoConfig {
                    user_name: parse::config_get(Some(p), "user.name", false),
                    user_email: parse::config_get(Some(p), "user.email", false),
                    gpg_sign: parse::config_get(Some(p), "commit.gpgsign", false) == "true",
                });
            }
            ui.add_space(ui.available_width() / 2.0 - 230.0);
            let busy = self.task.is_some();
            ui.add_enabled_ui(!busy, |ui| {
                if widgets::tool_button(ui, "⬇", "Fetch").clicked() {
                    self.open_fetch();
                }
                if widgets::tool_button(ui, "⏬", "Pull").clicked() {
                    self.open_pull();
                }
                if widgets::tool_button(ui, "⬆", "Push").clicked() {
                    self.open_push(None);
                }
                if widgets::tool_button(ui, "📥", "Stash all changes").clicked() {
                    self.popup = Some(Popup::StashPush { message: String::new(), include_untracked: true, keep_index: false, only_staged: false, paths: vec![] });
                }
                if widgets::tool_button(ui, "📃", "Apply patch").clicked() {
                    self.popup = Some(Popup::ApplyPatch { file: String::new(), whitespace: 0, three_way: false });
                }
                ui.separator();
                if widgets::tool_button(ui, "🔀", "Create branch").clicked() {
                    let (base, label) = match self.repo.current_branch() {
                        Some(b) => (b.name.clone(), b.name.clone()),
                        None => ("HEAD".into(), "HEAD".into()),
                    };
                    self.popup = Some(Popup::CreateBranch { base, base_label: label, name: String::new(), checkout: true });
                }
                if widgets::tool_button(ui, "🗑", "Cleanup (git gc --prune=now)").clicked() {
                    self.run_job(Job::git("Cleanup", ops::gc));
                }
                if widgets::tool_button(ui, "🔍", "Search commits").clicked() {
                    self.page = Page::Histories;
                    self.search.open = !self.search.open;
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::tool_button(ui, "🔄", "Refresh").clicked() {
                    self.refresh(prefs);
                }
                let label = match self.repo.current_branch() {
                    Some(b) if !b.is_detached => b.name.clone(),
                    Some(b) => b.name.clone(),
                    None if self.repo.data.is_bare => "(bare)".into(),
                    None => "(detached / no commits)".into(),
                };
                ui.label(RichText::new(label).strong());
                ui.label(RichText::new("🔀").color(pal.fg2));
                if let Some(t) = &self.task {
                    ui.separator();
                    ui.spinner();
                    ui.label(RichText::new(format!("{} ({}s)", t.label, t.started.elapsed().as_secs())).color(pal.fg2));
                }
            });
        });
    }

    fn icon_bar(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui.vertical_centered(|ui| {
            let unstaged = self.repo.data.changes.len();
            let stashes = self.repo.data.stashes.len();
            for (page, glyph, tip, count) in [
                (Page::Histories, "🕓", "Histories", 0usize),
                (Page::WorkingCopy, "📝", "Local changes", unstaged),
                (Page::Stashes, "📚", "Stashes", stashes),
            ] {
                let sel = self.page == page;
                let btn = egui::Button::new(RichText::new(glyph).size(20.0))
                    .fill(if sel { pal.flat_button_hover } else { egui::Color32::TRANSPARENT })
                    .min_size(egui::vec2(40.0, 40.0));
                let r = ui.add(btn).on_hover_text(tip);
                if count > 0 {
                    let pos = r.rect.right_top() + egui::vec2(-8.0, 6.0);
                    ui.painter().circle_filled(pos, 7.0, pal.badge);
                    ui.painter().text(pos, egui::Align2::CENTER_CENTER, count.min(99).to_string(), egui::FontId::proportional(9.0), pal.badge_fg);
                }
                if r.clicked() {
                    self.page = page;
                }
                ui.add_space(4.0);
            }
        });
    }

    pub fn open_fetch(&mut self) {
        let remotes: Vec<String> = self.repo.data.remotes.iter().map(|r| r.name.clone()).collect();
        if remotes.is_empty() {
            self.actions.push(Action::Error("No remotes configured".into()));
            return;
        }
        self.popup = Some(Popup::Fetch { remotes, remote: 0, prune: self.defaults.0, no_tags: false });
    }

    pub fn open_pull(&mut self) {
        let remotes: Vec<String> = self.repo.data.remotes.iter().map(|r| r.name.clone()).collect();
        if remotes.is_empty() {
            self.actions.push(Action::Error("No remotes configured".into()));
            return;
        }
        let cur = self.repo.current_branch().cloned();
        let upstream = cur.as_ref().map(|b| b.upstream_short()).unwrap_or_default();
        let remote_idx = upstream.split_once('/').and_then(|(r, _)| remotes.iter().position(|x| x == r)).unwrap_or(0);
        let rname = remotes[remote_idx].clone();
        let branches: Vec<String> = self.repo.remote_branches(&rname).iter().map(|b| b.friendly_name()).collect();
        let branch = branches.iter().position(|b| *b == upstream).unwrap_or(0);
        self.popup = Some(Popup::Pull {
            remotes,
            remote: remote_idx,
            branches,
            branch,
            into: cur.map(|b| b.name).unwrap_or_else(|| "HEAD".into()),
            rebase: self.defaults.1,
            autostash: self.defaults.2,
        });
    }

    pub fn open_push(&mut self, local: Option<&str>) {
        let remotes: Vec<String> = self.repo.data.remotes.iter().map(|r| r.name.clone()).collect();
        if remotes.is_empty() {
            self.actions.push(Action::Error("No remotes configured".into()));
            return;
        }
        let locals: Vec<String> = self.repo.local_branches().map(|b| b.name.clone()).collect();
        let want = local.map(str::to_string).or_else(|| self.repo.current_branch().map(|b| b.name.clone()));
        let li = want.as_ref().and_then(|w| locals.iter().position(|l| l == w)).unwrap_or(0);
        let b = self.repo.local_branches().find(|b| Some(&b.name) == locals.get(li)).cloned();
        let up = b.as_ref().map(|b| b.upstream_short()).unwrap_or_default();
        let (ri, rb) = match up.split_once('/') {
            Some((r, n)) => (remotes.iter().position(|x| x == r).unwrap_or(0), n.to_string()),
            None => (0, locals.get(li).cloned().unwrap_or_default()),
        };
        self.popup = Some(Popup::Push {
            locals,
            local: li,
            remotes,
            remote: ri,
            remote_branch: rb,
            set_upstream: up.is_empty(),
            force: false,
            tags: false,
            no_verify: false,
        });
    }

    fn in_progress_banner(&mut self, ui: &mut egui::Ui, pal: &Palette, ip: &git::InProgress) {
        let conflicts = self.repo.conflicts();
        egui::Frame::new().fill(pal.conflict).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(ip.title()).strong().color(pal.conflict_fg));
                if conflicts > 0 {
                    ui.label(RichText::new(format!("— {conflicts} conflict(s) to resolve")).color(pal.conflict_fg));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ip2 = ip.clone();
                    if ui.button("Abort").clicked() {
                        self.actions.push(Action::Job(Job::git("Abort", move |p| ops::abort_op(p, &ip2))));
                    }
                    if matches!(ip, git::InProgress::Rebase { .. } | git::InProgress::CherryPick | git::InProgress::Revert) {
                        let ip3 = ip.clone();
                        if ui.button("Skip").clicked() {
                            self.actions.push(Action::Job(Job::git("Skip", move |p| ops::skip_op(p, &ip3))));
                        }
                    }
                    if !matches!(ip, git::InProgress::Bisect) {
                        let ip4 = ip.clone();
                        if ui.add_enabled(conflicts == 0, egui::Button::new("Continue")).clicked() {
                            self.actions.push(Action::Job(Job::git("Continue", move |p| ops::continue_op(p, &ip4))));
                        }
                    } else {
                        for (label, sub) in [("Good", "good"), ("Bad", "bad"), ("Skip", "skip")] {
                            if ui.button(label).clicked() {
                                self.actions.push(Action::Job(Job::git(format!("Bisect {sub}"), move |p| ops::bisect(p, sub))));
                            }
                        }
                    }
                });
            });
        });
    }

    fn notices_ui(&mut self, ctx: &egui::Context, pal: &Palette) {
        self.notices.retain(|n| n.error || n.at.elapsed().as_secs() < 4);
        if self.notices.is_empty() {
            return;
        }
        let mut remove = None;
        egui::Area::new(egui::Id::new("notices")).anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0)).show(ctx, |ui| {
            ui.set_max_width(420.0);
            for (i, n) in self.notices.iter().enumerate() {
                let fill = if n.error { egui::Color32::from_rgb(0x8B, 0x2A, 0x2A) } else { pal.popup };
                egui::Frame::new().fill(fill).stroke(egui::Stroke::new(1.0_f32, pal.border1)).corner_radius(4).inner_margin(10).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let color = if n.error { egui::Color32::WHITE } else { pal.fg1 };
                        egui::ScrollArea::vertical().max_height(200.0).id_salt(("notice", i)).show(ui, |ui| {
                            ui.label(RichText::new(&n.text).color(color));
                        });
                        if ui.small_button("✖").clicked() {
                            remove = Some(i);
                        }
                    });
                });
                ui.add_space(4.0);
            }
        });
        if let Some(i) = remove {
            self.notices.remove(i);
        }
        if self.notices.iter().any(|n| !n.error) {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    pub fn stash_select(&mut self, idx: usize, _prefs: &Preferences) {
        let Some(s): Option<Stash> = self.repo.data.stashes.get(idx).cloned() else { return };
        self.stash.selected = Some(idx);
        self.stash.changes = parse::query_stash_changes(&self.repo.path, &s);
        self.stash.selected_change = None;
        self.stash.diff.clear();
    }
}

pub fn load_history_diff(repo: &std::path::Path, w: &mut HistoryWin, prefs: &Preferences) {
    let Some(i) = w.selected else { return };
    let Some(c) = w.commits.get(i) else { return };
    // The file may have been renamed in that commit; find its name there.
    let changes = parse::query_commit_changes(repo, &c.sha, c.parents.first().map(String::as_str));
    let change = changes.iter().find(|x| x.path == w.path).or_else(|| changes.first());
    let (path, original) = change.map(|c| (c.path.clone(), c.original_path.clone())).unwrap_or((w.path.clone(), String::new()));
    w.diff.load(
        repo,
        ops::DiffTarget::Revisions { from: c.parents.first().cloned().unwrap_or_default(), to: c.sha.clone(), path, original },
        prefs,
    );
}
