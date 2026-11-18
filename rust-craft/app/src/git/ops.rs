//! Diffs and every command that changes the repository.

use super::models::*;
use super::{exec, exec_in, exec_with_input, run, Output};
use std::path::Path;

pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// What a diff is computed between.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffTarget {
    /// Work tree vs index.
    Unstaged { path: String, untracked: bool },
    /// Index vs HEAD.
    Staged { path: String, original: String },
    /// Between two revisions (from may be empty → root commit).
    Revisions { from: String, to: String, path: String, original: String },
}

impl DiffTarget {
    pub fn path(&self) -> &str {
        match self {
            DiffTarget::Unstaged { path, .. } | DiffTarget::Staged { path, .. } | DiffTarget::Revisions { path, .. } => path,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffOptions {
    pub context: usize,
    pub ignore_whitespace: bool,
}

pub fn diff(repo: &Path, target: &DiffTarget, opt: DiffOptions) -> DiffContent {
    let unified = format!("--unified={}", opt.context.max(crate::consts::DIFF_MIN_CONTEXT_LINES));
    let mut args: Vec<String> =
        vec!["diff".into(), "--no-color".into(), "--no-ext-diff".into(), "--full-index".into(), "--patch".into()];
    if opt.ignore_whitespace {
        args.push("--ignore-space-change".into());
        args.push("--ignore-blank-lines".into());
    }
    args.push(unified);
    match target {
        DiffTarget::Unstaged { path, untracked } => {
            if *untracked {
                args.push("--no-index".into());
                args.push("--".into());
                args.push("/dev/null".into());
                args.push(path.clone());
            } else {
                args.push("--".into());
                args.push(path.clone());
            }
        }
        DiffTarget::Staged { path, original } => {
            args.push("--cached".into());
            args.push("-M".into());
            args.push("--".into());
            if !original.is_empty() {
                args.push(original.clone());
            }
            args.push(path.clone());
        }
        DiffTarget::Revisions { from, to, path, original } => {
            args.push("-M".into());
            args.push(if from.is_empty() { EMPTY_TREE.into() } else { from.clone() });
            if !to.is_empty() {
                args.push(to.clone());
            }
            args.push("--".into());
            if !original.is_empty() {
                args.push(original.clone());
            }
            args.push(path.clone());
        }
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let o = run(repo, &refs);
    parse_diff(&o.stdout)
}

pub fn parse_diff(out: &str) -> DiffContent {
    let mut td = TextDiff::default();
    let mut in_header = true;
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut mode_change = String::new();
    let mut sub_old = String::new();
    let mut sub_new = String::new();
    let mut is_submodule = false;

    for line in out.lines() {
        if in_header {
            if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
                return DiffContent::Binary;
            }
            if line.starts_with("@@") {
                in_header = false;
            } else {
                if line.starts_with("old mode ") || line.starts_with("new mode ") {
                    mode_change.push_str(line);
                    mode_change.push('\n');
                }
                td.header.push(line.to_string());
                continue;
            }
        }
        if line.starts_with("@@") {
            // @@ -a,b +c,d @@ ctx
            let (o, n) = parse_hunk_header(line);
            old_no = o;
            new_no = n;
            td.hunks.push(Hunk { header: line.to_string(), raw: Vec::new() });
            td.lines.push(DiffLine { kind: LineKind::Indicator, content: line.to_string(), old_no: 0, new_no: 0, hunk: td.hunks.len() - 1 });
            continue;
        }
        let hunk = td.hunks.len().saturating_sub(1);
        if let Some(h) = td.hunks.last_mut() {
            h.raw.push(line.to_string());
        }
        if line.starts_with('\\') {
            // "\ No newline at end of file" — keep in raw only.
            continue;
        }
        let (kind, content) = match line.chars().next() {
            Some('+') => (LineKind::Added, &line[1..]),
            Some('-') => (LineKind::Deleted, &line[1..]),
            Some(' ') => (LineKind::Normal, &line[1..]),
            None => (LineKind::Normal, ""),
            _ => (LineKind::Normal, line),
        };
        if let Some(s) = content.strip_prefix("Subproject commit ") {
            is_submodule = true;
            match kind {
                LineKind::Deleted => sub_old = s.trim().to_string(),
                LineKind::Added => sub_new = s.trim().to_string(),
                _ => {}
            }
        }
        match kind {
            LineKind::Added => {
                td.added += 1;
                td.lines.push(DiffLine { kind, content: content.to_string(), old_no: 0, new_no, hunk });
                new_no += 1;
            }
            LineKind::Deleted => {
                td.deleted += 1;
                td.lines.push(DiffLine { kind, content: content.to_string(), old_no, new_no: 0, hunk });
                old_no += 1;
            }
            _ => {
                td.lines.push(DiffLine { kind, content: content.to_string(), old_no, new_no, hunk });
                old_no += 1;
                new_no += 1;
            }
        }
    }
    if is_submodule {
        return DiffContent::Submodule { old: sub_old, new: sub_new };
    }
    if td.lines.is_empty() {
        if !mode_change.is_empty() {
            return DiffContent::ModeOnly(mode_change.trim().to_string());
        }
        return DiffContent::Empty;
    }
    DiffContent::Text(td)
}

fn parse_hunk_header(line: &str) -> (u32, u32) {
    let mut old = 0;
    let mut new = 0;
    for tok in line.split_whitespace().skip(1) {
        if let Some(r) = tok.strip_prefix('-') {
            old = r.split(',').next().and_then(|x| x.parse().ok()).unwrap_or(0);
        } else if let Some(r) = tok.strip_prefix('+') {
            new = r.split(',').next().and_then(|x| x.parse().ok()).unwrap_or(0);
            break;
        }
    }
    (old, new)
}

/// Builds a patch containing only hunk `idx` of `td`.
pub fn hunk_patch(td: &TextDiff, idx: usize) -> Option<String> {
    let h = td.hunks.get(idx)?;
    let mut s = String::new();
    for l in &td.header {
        s.push_str(l);
        s.push('\n');
    }
    s.push_str(&h.header);
    s.push('\n');
    for l in &h.raw {
        s.push_str(l);
        s.push('\n');
    }
    Some(s)
}

/// Builds a patch that applies only the selected lines (indices into `td.lines`).
/// `reverse` = the patch will be applied with -R (unstage / discard), which changes how
/// unselected lines are turned into context.
pub fn lines_patch(td: &TextDiff, selected: &std::collections::BTreeSet<usize>, reverse: bool) -> Option<String> {
    if selected.is_empty() {
        return None;
    }
    let mut s = String::new();
    for l in &td.header {
        s.push_str(l);
        s.push('\n');
    }
    let mut any = false;
    for (hi, h) in td.hunks.iter().enumerate() {
        // Line indices in td.lines belonging to this hunk (excluding indicator).
        let idxs: Vec<usize> = td.lines.iter().enumerate().filter(|(_, l)| l.hunk == hi && l.kind != LineKind::Indicator).map(|(i, _)| i).collect();
        if !idxs.iter().any(|i| selected.contains(i)) {
            continue;
        }
        let (old_start, new_start) = parse_hunk_header(&h.header);
        let mut body = Vec::new();
        let mut old_cnt = 0;
        let mut new_cnt = 0;
        for &i in &idxs {
            let l = &td.lines[i];
            let sel = selected.contains(&i);
            match l.kind {
                LineKind::Added => {
                    if sel {
                        body.push(format!("+{}", l.content));
                        new_cnt += 1;
                    } else if reverse {
                        // Already present in the "old" side for a reverse apply: keep as context.
                        body.push(format!(" {}", l.content));
                        old_cnt += 1;
                        new_cnt += 1;
                    }
                }
                LineKind::Deleted => {
                    if sel {
                        body.push(format!("-{}", l.content));
                        old_cnt += 1;
                    } else if !reverse {
                        body.push(format!(" {}", l.content));
                        old_cnt += 1;
                        new_cnt += 1;
                    }
                }
                _ => {
                    body.push(format!(" {}", l.content));
                    old_cnt += 1;
                    new_cnt += 1;
                }
            }
        }
        // Preserve "\ No newline" markers at the end of the hunk.
        if h.raw.last().map(|l| l.starts_with('\\')).unwrap_or(false) {
            body.push(h.raw.last().unwrap().clone());
        }
        s.push_str(&format!("@@ -{},{} +{},{} @@\n", old_start, old_cnt, new_start, new_cnt));
        for b in body {
            s.push_str(&b);
            s.push('\n');
        }
        any = true;
    }
    any.then_some(s)
}

pub fn apply_patch_text(repo: &Path, patch: &str, cached: bool, reverse: bool) -> Output {
    let mut args = vec!["apply", "--whitespace=nowarn", "--recount"];
    if cached {
        args.push("--cached");
    }
    if reverse {
        args.push("-R");
    }
    args.push("-");
    exec_with_input(repo, &args, patch.as_bytes())
}

// ---------------------------------------------------------------- staging

pub fn stage(repo: &Path, paths: &[String]) -> Output {
    if paths.is_empty() {
        return exec(repo, &["add", "-A"]);
    }
    let mut args = vec!["add", "-A", "--"];
    args.extend(paths.iter().map(String::as_str));
    exec(repo, &args)
}

pub fn unstage(repo: &Path, paths: &[String], has_head: bool) -> Output {
    if !has_head {
        let mut args = vec!["rm", "--cached", "-r", "-q", "--"];
        if paths.is_empty() {
            args.push(".");
        } else {
            args.extend(paths.iter().map(String::as_str));
        }
        return exec(repo, &args);
    }
    let mut args = vec!["reset", "-q", "HEAD", "--"];
    if paths.is_empty() {
        args.push(".");
    } else {
        args.extend(paths.iter().map(String::as_str));
    }
    exec(repo, &args)
}

/// Discards work-tree changes. Untracked files are removed; tracked files restored from index.
pub fn discard(repo: &Path, changes: &[Change]) -> Result<(), String> {
    let mut tracked = Vec::new();
    for c in changes {
        if c.worktree == ChangeState::Untracked || c.index == ChangeState::Added && c.worktree == ChangeState::None {
            if c.index == ChangeState::Added {
                let _ = exec(repo, &["rm", "--cached", "-q", "--", &c.path]);
            }
            let full = repo.join(&c.path);
            if full.is_dir() {
                std::fs::remove_dir_all(&full).map_err(|e| e.to_string())?;
            } else if full.exists() {
                std::fs::remove_file(&full).map_err(|e| e.to_string())?;
            }
        } else {
            tracked.push(c.path.clone());
        }
    }
    if !tracked.is_empty() {
        let mut args = vec!["checkout", "-f", "--"];
        args.extend(tracked.iter().map(String::as_str));
        exec(repo, &args).into_result()?;
    }
    Ok(())
}

pub fn discard_all(repo: &Path, include_untracked: bool, include_ignored: bool) -> Result<(), String> {
    exec(repo, &["reset", "--hard", "-q"]).into_result()?;
    if include_untracked {
        let args: Vec<&str> = if include_ignored { vec!["clean", "-qfdx"] } else { vec!["clean", "-qfd"] };
        exec(repo, &args).into_result()?;
    }
    Ok(())
}

pub fn commit(repo: &Path, message: &str, amend: bool, sign_off: bool, allow_empty: bool, no_verify: bool) -> Output {
    let tmp = std::env::temp_dir().join(format!("sourcegit-rs-commit-{}.txt", std::process::id()));
    if let Err(e) = std::fs::write(&tmp, message) {
        return Output { ok: false, stdout: String::new(), stderr: e.to_string() };
    }
    let file_arg = format!("--file={}", tmp.display());
    let mut args = vec!["commit", file_arg.as_str()];
    if amend {
        args.push("--amend");
        args.push("--reset-author");
        args.push("--no-edit");
    }
    if sign_off {
        args.push("--signoff");
    }
    if allow_empty {
        args.push("--allow-empty");
    }
    if no_verify {
        args.push("--no-verify");
    }
    let o = exec(repo, &args);
    let _ = std::fs::remove_file(&tmp);
    o
}

pub fn mark_resolved(repo: &Path, path: &str) -> Output {
    exec(repo, &["add", "--", path])
}

pub fn use_side(repo: &Path, path: &str, theirs: bool) -> Output {
    let o = exec(repo, &["checkout", if theirs { "--theirs" } else { "--ours" }, "--", path]);
    if !o.ok {
        return o;
    }
    exec(repo, &["add", "--", path])
}

pub fn continue_op(repo: &Path, op: &InProgress) -> Output {
    match op {
        InProgress::Rebase { .. } => {
            let mut cmd = vec!["-c", "core.editor=true", "rebase", "--continue"];
            cmd.shrink_to_fit();
            exec(repo, &cmd)
        }
        InProgress::Merge => exec(repo, &["-c", "core.editor=true", "commit", "--no-edit"]),
        InProgress::CherryPick => exec(repo, &["-c", "core.editor=true", "cherry-pick", "--continue"]),
        InProgress::Revert => exec(repo, &["-c", "core.editor=true", "revert", "--continue"]),
        InProgress::Bisect => exec(repo, &["bisect", "log"]),
    }
}

pub fn skip_op(repo: &Path, op: &InProgress) -> Output {
    match op {
        InProgress::Rebase { .. } => exec(repo, &["rebase", "--skip"]),
        InProgress::CherryPick => exec(repo, &["cherry-pick", "--skip"]),
        InProgress::Revert => exec(repo, &["revert", "--skip"]),
        _ => Output { ok: false, stdout: String::new(), stderr: "skip not supported".into() },
    }
}

pub fn abort_op(repo: &Path, op: &InProgress) -> Output {
    match op {
        InProgress::Bisect => exec(repo, &["bisect", "reset"]),
        _ => exec(repo, &[op.command(), "--abort"]),
    }
}

// ---------------------------------------------------------------- branches

pub fn checkout(repo: &Path, target: &str) -> Output {
    exec(repo, &["checkout", target])
}

pub fn checkout_detached(repo: &Path, sha: &str) -> Output {
    exec(repo, &["checkout", "--detach", sha])
}

pub fn checkout_new_tracking(repo: &Path, local: &str, remote_branch: &str) -> Output {
    exec(repo, &["checkout", "-b", local, "--track", remote_branch])
}

pub fn create_branch(repo: &Path, name: &str, base: &str, checkout_after: bool) -> Output {
    if checkout_after {
        exec(repo, &["checkout", "-b", name, base])
    } else {
        exec(repo, &["branch", name, base])
    }
}

pub fn rename_branch(repo: &Path, old: &str, new: &str) -> Output {
    exec(repo, &["branch", "-M", old, new])
}

pub fn delete_branch(repo: &Path, name: &str, force: bool) -> Output {
    exec(repo, &["branch", if force { "-D" } else { "-d" }, name])
}

pub fn delete_remote_branch(repo: &Path, remote: &str, name: &str) -> Output {
    let o = exec(repo, &["push", remote, "--delete", name]);
    if !o.ok {
        // Ref may already be gone on the server: drop the local tracking ref.
        let r = format!("{remote}/{name}");
        return exec(repo, &["branch", "-D", "-r", &r]);
    }
    o
}

pub fn set_upstream(repo: &Path, branch: &str, upstream: Option<&str>) -> Output {
    match upstream {
        Some(u) => {
            let arg = format!("--set-upstream-to={u}");
            exec(repo, &["branch", &arg, branch])
        }
        None => exec(repo, &["branch", "--unset-upstream", branch]),
    }
}

/// Fast-forward a non-current local branch to its upstream.
pub fn fast_forward(repo: &Path, local: &str, upstream: &str) -> Output {
    let spec = format!("{upstream}:{local}");
    exec(repo, &["fetch", ".", &spec])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeMode {
    Default,
    NoFastForward,
    FastForwardOnly,
    Squash,
    NoCommit,
}

impl MergeMode {
    pub const ALL: [MergeMode; 5] = [MergeMode::Default, MergeMode::NoFastForward, MergeMode::FastForwardOnly, MergeMode::Squash, MergeMode::NoCommit];
    pub fn label(self) -> &'static str {
        match self {
            MergeMode::Default => "Default (fast-forward if possible)",
            MergeMode::NoFastForward => "No fast-forward (--no-ff)",
            MergeMode::FastForwardOnly => "Fast-forward only (--ff-only)",
            MergeMode::Squash => "Squash (--squash)",
            MergeMode::NoCommit => "Don't commit (--no-ff --no-commit)",
        }
    }
}

pub fn merge(repo: &Path, source: &str, mode: MergeMode, edit_message: Option<&str>) -> Output {
    let mut args = vec!["-c", "core.editor=true", "merge", "--no-edit"];
    match mode {
        MergeMode::Default => {}
        MergeMode::NoFastForward => args.push("--no-ff"),
        MergeMode::FastForwardOnly => args.push("--ff-only"),
        MergeMode::Squash => args.push("--squash"),
        MergeMode::NoCommit => {
            args.push("--no-ff");
            args.push("--no-commit");
        }
    }
    let msg;
    if let Some(m) = edit_message.filter(|m| !m.trim().is_empty()) {
        msg = m.to_string();
        args.push("-m");
        args.push(&msg);
    }
    args.push(source);
    exec(repo, &args)
}

pub fn rebase(repo: &Path, onto: &str, autostash: bool) -> Output {
    let mut args = vec!["-c", "core.editor=true", "rebase"];
    if autostash {
        args.push("--autostash");
    }
    args.push(onto);
    exec(repo, &args)
}

pub fn cherry_pick(repo: &Path, shas: &[String], no_commit: bool, append_source: bool, mainline: Option<u32>) -> Output {
    let mut args: Vec<String> = vec!["-c".into(), "core.editor=true".into(), "cherry-pick".into()];
    if no_commit {
        args.push("--no-commit".into());
    }
    if append_source {
        args.push("-x".into());
    }
    if let Some(m) = mainline {
        args.push("-m".into());
        args.push(m.to_string());
    }
    args.extend(shas.iter().cloned());
    let r: Vec<&str> = args.iter().map(String::as_str).collect();
    exec(repo, &r)
}

pub fn revert(repo: &Path, sha: &str, auto_commit: bool, is_merge: bool) -> Output {
    let mut args = vec!["-c", "core.editor=true", "revert", "--no-edit"];
    if !auto_commit {
        args.push("--no-commit");
    }
    if is_merge {
        args.push("-m");
        args.push("1");
    }
    args.push(sha);
    exec(repo, &args)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Merge,
    Keep,
    Hard,
}

impl ResetMode {
    pub const ALL: [ResetMode; 5] = [ResetMode::Soft, ResetMode::Mixed, ResetMode::Merge, ResetMode::Keep, ResetMode::Hard];
    pub fn flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Merge => "--merge",
            ResetMode::Keep => "--keep",
            ResetMode::Hard => "--hard",
        }
    }
    pub fn desc(self) -> &'static str {
        match self {
            ResetMode::Soft => "Soft — keep all changes staged",
            ResetMode::Mixed => "Mixed — keep all changes unstaged",
            ResetMode::Merge => "Merge — reset, keep unmerged changes",
            ResetMode::Keep => "Keep — reset, keep local changes",
            ResetMode::Hard => "Hard — discard all changes",
        }
    }
}

pub fn reset(repo: &Path, rev: &str, mode: ResetMode) -> Output {
    exec(repo, &["reset", mode.flag(), rev])
}

/// Move a (non-current) branch to a revision without checking it out.
pub fn reset_branch_to(repo: &Path, branch: &str, rev: &str) -> Output {
    exec(repo, &["branch", "-f", branch, rev])
}

// ---------------------------------------------------------------- remotes

pub fn fetch(repo: &Path, remote: Option<&str>, prune: bool, no_tags: bool) -> Output {
    let mut args = vec!["fetch", "--progress", "--verbose"];
    match remote {
        Some(r) => args.push(r),
        None => args.push("--all"),
    }
    if prune {
        args.push("--prune");
    }
    if no_tags {
        args.push("--no-tags");
    } else {
        args.push("--tags");
        args.push("--force");
    }
    exec(repo, &args)
}

pub fn pull(repo: &Path, remote: &str, branch: &str, rebase: bool, autostash: bool) -> Output {
    let mut args = vec!["pull", "--verbose", "--progress"];
    if rebase {
        args.push("--rebase");
    } else {
        args.push("--no-rebase");
    }
    if autostash {
        args.push("--autostash");
    }
    args.push(remote);
    if !branch.is_empty() {
        args.push(branch);
    }
    exec(repo, &args)
}

pub struct PushOptions<'a> {
    pub remote: &'a str,
    pub local: &'a str,
    pub remote_branch: &'a str,
    pub set_upstream: bool,
    pub force: bool,
    pub tags: bool,
    pub no_verify: bool,
}

pub fn push(repo: &Path, o: &PushOptions) -> Output {
    let mut args = vec!["push", "--progress", "--verbose"];
    if o.set_upstream {
        args.push("-u");
    }
    if o.force {
        args.push("--force-with-lease");
    }
    if o.tags {
        args.push("--tags");
    }
    if o.no_verify {
        args.push("--no-verify");
    }
    args.push(o.remote);
    let spec = format!("{}:refs/heads/{}", o.local, o.remote_branch);
    args.push(&spec);
    exec(repo, &args)
}

pub fn push_tag(repo: &Path, remote: &str, tag: &str) -> Output {
    let spec = format!("refs/tags/{tag}");
    exec(repo, &["push", remote, &spec])
}

pub fn delete_remote_tag(repo: &Path, remote: &str, tag: &str) -> Output {
    let spec = format!("refs/tags/{tag}");
    exec(repo, &["push", "--delete", remote, &spec])
}

pub fn add_remote(repo: &Path, name: &str, url: &str) -> Output {
    exec(repo, &["remote", "add", name, url])
}

pub fn set_remote_url(repo: &Path, name: &str, url: &str, push: bool) -> Output {
    if push {
        exec(repo, &["remote", "set-url", "--push", name, url])
    } else {
        exec(repo, &["remote", "set-url", name, url])
    }
}

pub fn rename_remote(repo: &Path, old: &str, new: &str) -> Output {
    exec(repo, &["remote", "rename", old, new])
}

pub fn delete_remote(repo: &Path, name: &str) -> Output {
    exec(repo, &["remote", "remove", name])
}

pub fn prune_remote(repo: &Path, name: &str) -> Output {
    exec(repo, &["remote", "prune", name])
}

// ---------------------------------------------------------------- tags

pub fn create_tag(repo: &Path, name: &str, rev: &str, message: Option<&str>, sign: bool) -> Output {
    match message.filter(|m| !m.trim().is_empty()) {
        Some(m) => {
            let mut args = vec!["tag"];
            args.push(if sign { "-s" } else { "-a" });
            args.extend([name, rev, "-m", m]);
            exec(repo, &args)
        }
        None => exec(repo, &["tag", name, rev]),
    }
}

pub fn delete_tag(repo: &Path, name: &str) -> Output {
    exec(repo, &["tag", "-d", name])
}

// ---------------------------------------------------------------- stash

pub fn stash_push(repo: &Path, message: &str, include_untracked: bool, keep_index: bool, only_staged: bool, paths: &[String]) -> Output {
    let mut args = vec!["stash", "push"];
    if include_untracked && !only_staged {
        args.push("--include-untracked");
    }
    if keep_index {
        args.push("--keep-index");
    }
    if only_staged {
        args.push("--staged");
    }
    if !message.trim().is_empty() {
        args.push("-m");
        args.push(message);
    }
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    exec(repo, &args)
}

pub fn stash_apply(repo: &Path, name: &str, restore_index: bool) -> Output {
    let mut args = vec!["stash", "apply", "-q"];
    if restore_index {
        args.push("--index");
    }
    args.push(name);
    exec(repo, &args)
}

pub fn stash_pop(repo: &Path, name: &str) -> Output {
    exec(repo, &["stash", "pop", "-q", name])
}

pub fn stash_drop(repo: &Path, name: &str) -> Output {
    exec(repo, &["stash", "drop", "-q", name])
}

pub fn stash_clear(repo: &Path) -> Output {
    exec(repo, &["stash", "clear"])
}

pub fn stash_branch(repo: &Path, branch: &str, stash: &str) -> Output {
    exec(repo, &["stash", "branch", branch, stash])
}

// ---------------------------------------------------------------- submodules / worktrees

pub fn submodule_add(repo: &Path, url: &str, path: &str, recursive: bool) -> Output {
    let o = exec(repo, &["-c", "protocol.file.allow=always", "submodule", "add", url, path]);
    if o.ok && recursive {
        return exec(repo, &["submodule", "update", "--init", "--recursive", "--", path]);
    }
    o
}

pub fn submodule_update(repo: &Path, paths: &[String], init: bool, recursive: bool, remote: bool) -> Output {
    let mut args = vec!["submodule", "update"];
    if init {
        args.push("--init");
    }
    if recursive {
        args.push("--recursive");
    }
    if remote {
        args.push("--remote");
    }
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    exec(repo, &args)
}

pub fn submodule_deinit(repo: &Path, path: &str) -> Output {
    exec(repo, &["submodule", "deinit", "-f", "--", path])
}

pub fn submodule_delete(repo: &Path, path: &str) -> Output {
    let o = submodule_deinit(repo, path);
    if !o.ok {
        return o;
    }
    exec(repo, &["rm", "-rf", "--", path])
}

pub fn worktree_add(repo: &Path, path: &str, branch: &str, new_branch: bool, tracking: Option<&str>) -> Output {
    let mut args = vec!["worktree", "add"];
    if new_branch && !branch.is_empty() {
        args.push("-b");
        args.push(branch);
    }
    args.push(path);
    if let Some(t) = tracking.filter(|t| !t.is_empty()) {
        args.push(t);
    } else if !new_branch && !branch.is_empty() {
        args.push(branch);
    }
    exec(repo, &args)
}

pub fn worktree_remove(repo: &Path, path: &str, force: bool) -> Output {
    if force {
        exec(repo, &["worktree", "remove", "--force", path])
    } else {
        exec(repo, &["worktree", "remove", path])
    }
}

pub fn worktree_prune(repo: &Path) -> Output {
    exec(repo, &["worktree", "prune", "-v"])
}

pub fn worktree_lock(repo: &Path, path: &str, lock: bool) -> Output {
    exec(repo, &["worktree", if lock { "lock" } else { "unlock" }, path])
}

// ---------------------------------------------------------------- misc

pub fn clone(url: &str, parent: &Path, name: &str, extra: &str, recursive: bool) -> Output {
    let mut args = vec!["clone", "--progress", "--verbose"];
    if recursive {
        args.push("--recurse-submodules");
    }
    let extras: Vec<&str> = extra.split_whitespace().collect();
    args.extend(extras.iter());
    args.push(url);
    if !name.is_empty() {
        args.push(name);
    }
    exec_in(Some(parent), &args)
}

pub fn init(path: &Path) -> Output {
    exec_in(Some(path), &["init"])
}

pub fn gc(repo: &Path) -> Output {
    exec(repo, &["gc", "--prune=now"])
}

pub fn format_patch(repo: &Path, sha: &str, out_file: &Path) -> Result<(), String> {
    let o = run(repo, &["format-patch", "-1", "--stdout", sha]);
    if !o.ok {
        return Err(o.stderr);
    }
    std::fs::write(out_file, o.stdout).map_err(|e| e.to_string())
}

pub fn save_changes_as_patch(repo: &Path, staged: bool, out_file: &Path) -> Result<(), String> {
    let args: Vec<&str> = if staged { vec!["diff", "--cached", "--binary"] } else { vec!["diff", "--binary"] };
    let o = run(repo, &args);
    if !o.ok {
        return Err(o.stderr);
    }
    std::fs::write(out_file, o.stdout).map_err(|e| e.to_string())
}

pub fn apply_patch_file(repo: &Path, file: &str, whitespace: &str, three_way: bool) -> Output {
    let ws = format!("--whitespace={whitespace}");
    let mut args = vec!["apply", ws.as_str()];
    if three_way {
        args.push("--3way");
    }
    args.push(file);
    exec(repo, &args)
}

pub fn archive(repo: &Path, rev: &str, out_file: &str) -> Output {
    let o = format!("--output={out_file}");
    exec(repo, &["archive", "--format=zip", "--verbose", &o, rev])
}

pub fn set_config(repo: Option<&Path>, key: &str, value: &str, global: bool) -> Output {
    let mut args = vec!["config"];
    if global {
        args.push("--global");
    }
    if value.is_empty() {
        args.push("--unset");
        args.push(key);
    } else {
        args.push(key);
        args.push(value);
    }
    exec_in(repo, &args)
}

pub fn add_to_gitignore(repo: &Path, pattern: &str) -> Result<(), String> {
    let p = repo.join(".gitignore");
    let mut s = std::fs::read_to_string(&p).unwrap_or_default();
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(pattern);
    s.push('\n');
    std::fs::write(p, s).map_err(|e| e.to_string())
}

pub fn has_head(repo: &Path) -> bool {
    run(repo, &["rev-parse", "--verify", "-q", "HEAD"]).ok
}

pub fn bisect(repo: &Path, sub: &str) -> Output {
    exec(repo, &["bisect", sub])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "diff --git a/f.txt b/f.txt\nindex 111..222 100644\n--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n";

    #[test]
    fn parse_text_diff() {
        let DiffContent::Text(td) = parse_diff(SAMPLE) else { panic!() };
        assert_eq!(td.header.len(), 4);
        assert_eq!(td.hunks.len(), 1);
        assert_eq!(td.added, 1);
        assert_eq!(td.deleted, 1);
        assert_eq!(td.lines[2].old_no, 2);
        assert_eq!(td.lines[3].new_no, 2);
    }

    #[test]
    fn partial_patch() {
        let DiffContent::Text(td) = parse_diff(SAMPLE) else { panic!() };
        let mut sel = std::collections::BTreeSet::new();
        sel.insert(3); // "+B" only
        let p = lines_patch(&td, &sel, false).unwrap();
        assert!(p.contains("@@ -1,3 +1,4 @@"));
        assert!(p.contains(" b\n+B\n"));
    }

    #[test]
    fn binary() {
        assert!(matches!(parse_diff("diff --git a/x b/x\nBinary files a/x and b/x differ\n"), DiffContent::Binary));
    }
}
