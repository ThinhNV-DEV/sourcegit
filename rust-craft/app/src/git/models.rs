use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct User {
    pub name: String,
    pub email: String,
}

impl User {
    /// Parses `name±email` (the separator SourceGit's log format uses).
    pub fn parse(s: &str) -> User {
        match s.split_once('±') {
            Some((n, e)) => User { name: n.to_string(), email: e.trim_matches(['<', '>']).to_string() },
            None => User { name: s.to_string(), email: String::new() },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecoratorKind {
    CurrentBranchHead,
    LocalBranchHead,
    CurrentCommitHead,
    RemoteBranchHead,
    Tag,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decorator {
    pub kind: DecoratorKind,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct Commit {
    pub sha: String,
    pub parents: Vec<String>,
    pub decorators: Vec<Decorator>,
    pub author: User,
    pub author_time: i64,
    pub committer: User,
    pub committer_time: i64,
    pub subject: String,
    /// Reachable from HEAD.
    pub is_merged: bool,
    // Filled by graph layout.
    pub color: usize,
    pub left_margin: f32,
}

impl Commit {
    pub fn short_sha(&self) -> &str {
        let n = crate::consts::SHORT_SHA_LENGTH.min(self.sha.len());
        &self.sha[..n]
    }
    pub fn is_current_head(&self) -> bool {
        self.decorators
            .iter()
            .any(|d| matches!(d.kind, DecoratorKind::CurrentBranchHead | DecoratorKind::CurrentCommitHead))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Branch {
    /// Short name, e.g. `main` or `feature/x` (for remotes: without the remote prefix).
    pub name: String,
    /// Full ref name, e.g. `refs/heads/main`.
    pub full_name: String,
    pub committer_date: i64,
    pub head: String,
    pub is_local: bool,
    pub is_current: bool,
    pub is_detached: bool,
    pub upstream: String,
    pub ahead: Vec<String>,
    pub behind: Vec<String>,
    pub remote: String,
    pub worktree_path: String,
}

impl Branch {
    /// Name usable on the command line: `main` or `origin/main`.
    pub fn friendly_name(&self) -> String {
        if self.is_local { self.name.clone() } else { format!("{}/{}", self.remote, self.name) }
    }
    pub fn upstream_short(&self) -> String {
        self.upstream.strip_prefix("refs/remotes/").unwrap_or(&self.upstream).to_string()
    }
    pub fn track_text(&self) -> String {
        let mut s = String::new();
        if !self.ahead.is_empty() {
            s.push_str(&format!("{}↑", self.ahead.len()));
        }
        if !self.behind.is_empty() {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(&format!("{}↓", self.behind.len()));
        }
        s
    }
}

#[derive(Clone, Debug, Default)]
pub struct Tag {
    pub name: String,
    pub sha: String,
    pub is_annotated: bool,
    pub tagger: User,
    pub date: i64,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct Remote {
    pub name: String,
    pub url: String,
    pub push_url: String,
}

impl Remote {
    /// Web page for the remote if it looks like a common hosting URL.
    pub fn web_url(&self) -> Option<String> {
        let u = self.url.trim().trim_end_matches(".git");
        if u.starts_with("http://") || u.starts_with("https://") {
            return Some(u.to_string());
        }
        // git@host:owner/repo
        if let Some(rest) = u.strip_prefix("git@") {
            if let Some((host, path)) = rest.split_once(':') {
                return Some(format!("https://{host}/{path}"));
            }
        }
        if let Some(rest) = u.strip_prefix("ssh://") {
            let rest = rest.split_once('@').map(|x| x.1).unwrap_or(rest);
            if let Some((host, path)) = rest.split_once('/') {
                let host = host.split(':').next().unwrap_or(host);
                return Some(format!("https://{host}/{path}"));
            }
        }
        None
    }
}

#[derive(Clone, Debug, Default)]
pub struct Stash {
    pub sha: String,
    pub parents: Vec<String>,
    pub time: i64,
    pub name: String,
    pub message: String,
}

impl Stash {
    pub fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or("")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmoduleStatus {
    Normal,
    NotInitialized,
    RevisionChanged,
    Unmerged,
    Modified,
}

#[derive(Clone, Debug)]
pub struct Submodule {
    pub path: String,
    pub sha: String,
    pub url: String,
    pub branch: String,
    pub status: SubmoduleStatus,
}

#[derive(Clone, Debug, Default)]
pub struct Worktree {
    pub path: String,
    pub head: String,
    pub branch: String,
    pub is_bare: bool,
    pub is_detached: bool,
    pub is_locked: bool,
    pub is_prunable: bool,
}

impl Worktree {
    pub fn branch_short(&self) -> String {
        self.branch.strip_prefix("refs/heads/").unwrap_or(&self.branch).to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeState {
    None,
    Modified,
    TypeChanged,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Conflicted,
}

impl ChangeState {
    pub fn from_char(c: char) -> ChangeState {
        match c {
            'M' => ChangeState::Modified,
            'T' => ChangeState::TypeChanged,
            'A' => ChangeState::Added,
            'D' => ChangeState::Deleted,
            'R' => ChangeState::Renamed,
            'C' => ChangeState::Copied,
            '?' => ChangeState::Untracked,
            'U' => ChangeState::Conflicted,
            _ => ChangeState::None,
        }
    }
    pub fn letter(self) -> &'static str {
        match self {
            ChangeState::None => " ",
            ChangeState::Modified => "M",
            ChangeState::TypeChanged => "T",
            ChangeState::Added => "A",
            ChangeState::Deleted => "D",
            ChangeState::Renamed => "R",
            ChangeState::Copied => "C",
            ChangeState::Untracked => "?",
            ChangeState::Conflicted => "!",
        }
    }
}

/// One file in `git status` or a commit/revision diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub original_path: String,
    /// Index (staged) state; for commit changes this is the only state used.
    pub index: ChangeState,
    /// Work-tree (unstaged) state.
    pub worktree: ChangeState,
    /// Raw XY code from porcelain status (for conflicts).
    pub code: String,
}

impl Change {
    pub fn simple(path: &str, state: ChangeState) -> Change {
        Change { path: path.to_string(), original_path: String::new(), index: state, worktree: ChangeState::None, code: String::new() }
    }
    pub fn is_conflicted(&self) -> bool {
        self.index == ChangeState::Conflicted || self.worktree == ChangeState::Conflicted
    }
    pub fn has_unstaged(&self) -> bool {
        self.worktree != ChangeState::None || self.is_conflicted()
    }
    pub fn has_staged(&self) -> bool {
        self.index != ChangeState::None && self.index != ChangeState::Untracked && !self.is_conflicted()
    }
    pub fn conflict_desc(&self) -> &'static str {
        match self.code.as_str() {
            "DD" => "Both deleted",
            "AU" => "Added by us",
            "UD" => "Deleted by them",
            "UA" => "Added by them",
            "DU" => "Deleted by us",
            "AA" => "Both added",
            "UU" => "Both modified",
            _ => "Conflict",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Normal,
    Added,
    Deleted,
    Indicator,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub kind: LineKind,
    pub content: String,
    pub old_no: u32,
    pub new_no: u32,
    /// Index of the hunk this line belongs to.
    pub hunk: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Hunk {
    /// The `@@ -a,b +c,d @@` header line.
    pub header: String,
    /// Raw lines (with +/-/space prefix), excluding the header.
    pub raw: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct TextDiff {
    /// `diff --git`, `index`, `---`, `+++` lines; used to build patches.
    pub header: Vec<String>,
    pub hunks: Vec<Hunk>,
    pub lines: Vec<DiffLine>,
    pub added: usize,
    pub deleted: usize,
}

#[derive(Clone, Debug)]
pub enum DiffContent {
    Empty,
    Text(TextDiff),
    Binary,
    Submodule { old: String, new: String },
    ModeOnly(String),
}

#[derive(Clone, Debug)]
pub struct BlameLine {
    pub sha: String,
    pub author: String,
    pub time: i64,
    pub content: String,
    pub file: String,
}

#[derive(Clone, Debug)]
pub enum InProgress {
    Merge,
    Rebase { head_name: String, onto: String },
    CherryPick,
    Revert,
    Bisect,
}

impl InProgress {
    pub fn title(&self) -> String {
        match self {
            InProgress::Merge => "Merge in progress".into(),
            InProgress::Rebase { head_name, onto } => {
                format!("Rebase in progress: {} onto {}", head_name, &onto[..onto.len().min(10)])
            }
            InProgress::CherryPick => "Cherry-pick in progress".into(),
            InProgress::Revert => "Revert in progress".into(),
            InProgress::Bisect => "Bisect in progress".into(),
        }
    }
    pub fn command(&self) -> &'static str {
        match self {
            InProgress::Merge => "merge",
            InProgress::Rebase { .. } => "rebase",
            InProgress::CherryPick => "cherry-pick",
            InProgress::Revert => "revert",
            InProgress::Bisect => "bisect",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RevisionFile {
    pub path: String,
    pub kind: String,
}

pub fn expand_home(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(h) = dirs::home_dir() {
            return h.join(rest);
        }
    }
    PathBuf::from(p)
}
