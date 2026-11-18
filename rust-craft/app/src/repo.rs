//! In-memory model of one opened repository.

use crate::git::{self, parse, *};
use crate::graph::{self, Graph};
use std::path::PathBuf;

#[derive(Default)]
pub struct RepoData {
    pub branches: Vec<Branch>,
    pub remotes: Vec<Remote>,
    pub tags: Vec<Tag>,
    pub stashes: Vec<Stash>,
    pub submodules: Vec<Submodule>,
    pub worktrees: Vec<Worktree>,
    pub commits: Vec<Commit>,
    pub graph: Graph,
    pub changes: Vec<Change>,
    pub in_progress: Option<InProgress>,
    pub has_head: bool,
    pub is_bare: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryScope {
    All,
    CurrentBranch,
    /// Only these refs (full ref names or revisions).
    Refs(Vec<String>),
}

pub struct Repository {
    pub path: PathBuf,
    pub name: String,
    pub git_dir: PathBuf,
    pub data: RepoData,
    pub scope: HistoryScope,
}

pub struct LoadOptions {
    pub max_commits: usize,
    pub show_untracked: bool,
    pub first_parent: bool,
    pub topo: bool,
}

impl Repository {
    pub fn open(path: PathBuf) -> Result<Repository, String> {
        let root = git::repo_root(&path).ok_or_else(|| format!("'{}' is not a git repository", path.display()))?;
        let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.display().to_string());
        let git_dir = git::git_dir(&root);
        Ok(Repository { path: root, name, git_dir, data: RepoData::default(), scope: HistoryScope::All })
    }

    pub fn current_branch(&self) -> Option<&Branch> {
        self.data.branches.iter().find(|b| b.is_local && b.is_current)
    }

    pub fn local_branches(&self) -> impl Iterator<Item = &Branch> {
        self.data.branches.iter().filter(|b| b.is_local && !b.is_detached)
    }

    pub fn remote_branches(&self, remote: &str) -> Vec<&Branch> {
        self.data.branches.iter().filter(|b| !b.is_local && b.remote == remote).collect()
    }

    pub fn refresh_all(&mut self, opt: &LoadOptions) {
        self.refresh_refs();
        self.refresh_commits(opt);
        self.refresh_changes(opt.show_untracked);
    }

    pub fn refresh_refs(&mut self) {
        let p = &self.path;
        self.data.is_bare = git::run(p, &["rev-parse", "--is-bare-repository"]).stdout.trim() == "true";
        self.data.has_head = ops::has_head(p);
        self.data.remotes = parse::query_remotes(p);
        self.data.branches = parse::query_branches(p);
        self.data.tags = parse::query_tags(p);
        self.data.stashes = parse::query_stashes(p);
        self.data.submodules = parse::query_submodules(p);
        self.data.worktrees = parse::query_worktrees(p);
        // Only show linked worktrees (the first entry is the main one).
        if !self.data.worktrees.is_empty() {
            self.data.worktrees.remove(0);
        }
        self.data.in_progress = parse::in_progress(&self.git_dir);
    }

    pub fn refresh_commits(&mut self, opt: &LoadOptions) {
        let scope: Vec<String> = match &self.scope {
            HistoryScope::All => vec!["--branches".into(), "--remotes".into(), "--tags".into(), "HEAD".into()],
            HistoryScope::CurrentBranch => vec!["HEAD".into()],
            HistoryScope::Refs(r) => r.clone(),
        };
        let scope = if self.data.has_head || !matches!(self.scope, HistoryScope::All) {
            scope
        } else {
            vec!["--branches".into(), "--remotes".into(), "--tags".into()]
        };
        let mut commits = if self.data.has_head || !self.data.branches.is_empty() {
            parse::query_commits(&self.path, opt.max_commits, &scope, opt.first_parent, opt.topo)
        } else {
            Vec::new()
        };
        self.data.graph = graph::generate(&mut commits, opt.first_parent);
        self.data.commits = commits;
    }

    pub fn refresh_changes(&mut self, show_untracked: bool) {
        self.data.changes = parse::query_local_changes(&self.path, show_untracked);
        self.data.in_progress = parse::in_progress(&self.git_dir);
    }

    pub fn unstaged(&self) -> Vec<Change> {
        self.data.changes.iter().filter(|c| c.has_unstaged()).cloned().collect()
    }

    pub fn staged(&self) -> Vec<Change> {
        self.data.changes.iter().filter(|c| c.has_staged()).cloned().collect()
    }

    pub fn conflicts(&self) -> usize {
        self.data.changes.iter().filter(|c| c.is_conflicted()).count()
    }
}
