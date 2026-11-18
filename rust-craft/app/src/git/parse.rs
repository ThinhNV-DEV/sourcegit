//! Query commands and the parsers for their output.

use super::models::*;
use super::run;
use std::collections::HashSet;
use std::path::Path;

pub const LOG_FORMAT: &str = "--format=%H%x00%P%x00%D%x00%aN±%aE%x00%at%x00%cN±%cE%x00%ct%x00%s";

pub fn parse_decorators(s: &str) -> Vec<Decorator> {
    let mut out = Vec::new();
    for raw in s.split(", ") {
        let d = raw.trim();
        if d.is_empty() {
            continue;
        }
        if let Some(rest) = d.strip_prefix("HEAD -> refs/heads/") {
            out.push(Decorator { kind: DecoratorKind::CurrentBranchHead, name: rest.to_string() });
        } else if d == "HEAD" {
            out.push(Decorator { kind: DecoratorKind::CurrentCommitHead, name: "HEAD".into() });
        } else if let Some(rest) = d.strip_prefix("tag: refs/tags/") {
            out.push(Decorator { kind: DecoratorKind::Tag, name: rest.to_string() });
        } else if let Some(rest) = d.strip_prefix("refs/heads/") {
            out.push(Decorator { kind: DecoratorKind::LocalBranchHead, name: rest.to_string() });
        } else if let Some(rest) = d.strip_prefix("refs/remotes/") {
            if rest.ends_with("/HEAD") {
                continue;
            }
            out.push(Decorator { kind: DecoratorKind::RemoteBranchHead, name: rest.to_string() });
        }
    }
    // Same order as SourceGit: current head, local, tags... keep current first.
    out.sort_by_key(|d| match d.kind {
        DecoratorKind::CurrentBranchHead | DecoratorKind::CurrentCommitHead => 0,
        DecoratorKind::LocalBranchHead => 1,
        DecoratorKind::RemoteBranchHead => 2,
        DecoratorKind::Tag => 3,
    });
    out
}

pub fn parse_log_line(line: &str) -> Option<Commit> {
    let parts: Vec<&str> = line.split('\0').collect();
    if parts.len() != 8 {
        return None;
    }
    Some(Commit {
        sha: parts[0].to_string(),
        parents: parts[1].split_whitespace().map(str::to_string).collect(),
        decorators: parse_decorators(parts[2]),
        author: User::parse(parts[3]),
        author_time: parts[4].parse().unwrap_or(0),
        committer: User::parse(parts[5]),
        committer_time: parts[6].parse().unwrap_or(0),
        subject: parts[7].to_string(),
        ..Default::default()
    })
}

pub fn parse_log(out: &str) -> Vec<Commit> {
    out.lines().filter_map(parse_log_line).collect()
}

/// Loads history. `scope` is extra args such as `--branches --remotes --tags HEAD` or a branch name.
pub fn query_commits(repo: &Path, max: usize, scope: &[String], first_parent: bool, topo: bool) -> Vec<Commit> {
    let limit = format!("-{max}");
    let mut args: Vec<&str> = vec!["log", &limit, "--no-show-signature", "--decorate=full", LOG_FORMAT];
    args.push(if topo { "--topo-order" } else { "--date-order" });
    if first_parent {
        args.push("--first-parent");
    }
    for s in scope {
        args.push(s);
    }
    let o = run(repo, &args);
    let mut commits = parse_log(&o.stdout);
    mark_merged(repo, &mut commits);
    commits
}

/// Marks commits reachable from HEAD (walks parents through the loaded list).
pub fn mark_merged(repo: &Path, commits: &mut [Commit]) {
    let head = run(repo, &["rev-parse", "HEAD"]).stdout.trim().to_string();
    if head.is_empty() {
        return;
    }
    let mut want: HashSet<String> = HashSet::new();
    want.insert(head);
    for c in commits.iter_mut() {
        if want.remove(&c.sha) {
            c.is_merged = true;
            for p in &c.parents {
                want.insert(p.clone());
            }
        }
    }
}

pub fn search_commits(repo: &Path, method: SearchMethod, filter: &str, only_current: bool) -> Vec<Commit> {
    let limit = format!("-{}", crate::consts::COMMIT_SEARCH_LIMIT);
    let mut args: Vec<String> =
        vec!["log".into(), limit, "--date-order".into(), "--no-show-signature".into(), "--decorate=full".into(), LOG_FORMAT.into()];
    if !only_current {
        args.push("--branches".into());
        args.push("--remotes".into());
    }
    match method {
        SearchMethod::Author => {
            args.push("-i".into());
            args.push(format!("--author={filter}"));
        }
        SearchMethod::Committer => {
            args.push("-i".into());
            args.push(format!("--committer={filter}"));
        }
        SearchMethod::Message => {
            for w in filter.split_whitespace() {
                args.push(format!("--grep={w}"));
            }
            args.push("--all-match".into());
            args.push("-i".into());
        }
        SearchMethod::Path => {
            args.push("--".into());
            args.push(filter.into());
        }
        SearchMethod::Content => args.push(format!("-G{filter}")),
        SearchMethod::Sha => {
            let o = run(repo, &["log", "-1", "--no-show-signature", "--decorate=full", LOG_FORMAT, filter]);
            return parse_log(&o.stdout);
        }
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    parse_log(&run(repo, &refs).stdout)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMethod {
    Sha,
    Author,
    Committer,
    Message,
    Path,
    Content,
}

impl SearchMethod {
    pub const ALL: [SearchMethod; 6] =
        [SearchMethod::Message, SearchMethod::Author, SearchMethod::Committer, SearchMethod::Sha, SearchMethod::Path, SearchMethod::Content];
    pub fn label(self) -> &'static str {
        match self {
            SearchMethod::Sha => "Commit SHA",
            SearchMethod::Author => "Author",
            SearchMethod::Committer => "Committer",
            SearchMethod::Message => "Message",
            SearchMethod::Path => "File path",
            SearchMethod::Content => "Content change",
        }
    }
}

pub fn query_branches(repo: &Path) -> Vec<Branch> {
    let o = run(
        repo,
        &[
            "branch",
            "-l",
            "--all",
            "-v",
            "--format=%(refname)%00%(committerdate:unix)%00%(objectname)%00%(HEAD)%00%(upstream)%00%(upstream:trackshort)%00%(worktreepath)",
        ],
    );
    let remotes: Vec<String> = query_remotes(repo).into_iter().map(|r| r.name).collect();
    let mut branches = parse_branches(&o.stdout, &remotes);
    // Ahead / behind lists for local branches with an upstream that differs.
    let upstreams: HashSet<String> = branches.iter().filter(|b| !b.is_local).map(|b| b.full_name.clone()).collect();
    for b in branches.iter_mut().filter(|b| b.is_local && !b.upstream.is_empty()) {
        if !upstreams.contains(&b.upstream) {
            continue; // upstream gone
        }
        let range = format!("{}...{}", b.head, b.upstream);
        let o = run(repo, &["rev-list", "--left-right", &range]);
        for l in o.stdout.lines() {
            if let Some(s) = l.strip_prefix('<') {
                b.ahead.push(s.to_string());
            } else if let Some(s) = l.strip_prefix('>') {
                b.behind.push(s.to_string());
            }
        }
    }
    branches
}

pub fn parse_branches(out: &str, remotes: &[String]) -> Vec<Branch> {
    let mut list = Vec::new();
    for line in out.lines() {
        let line = line.trim_matches('"');
        let p: Vec<&str> = line.split('\0').collect();
        if p.len() != 7 {
            continue;
        }
        let refname = p[0];
        let mut b = Branch {
            full_name: refname.to_string(),
            committer_date: p[1].parse().unwrap_or(0),
            head: p[2].to_string(),
            is_current: p[3] == "*",
            upstream: p[4].to_string(),
            worktree_path: p[6].to_string(),
            ..Default::default()
        };
        if let Some(n) = refname.strip_prefix("refs/heads/") {
            b.is_local = true;
            b.name = n.to_string();
        } else if let Some(n) = refname.strip_prefix("refs/remotes/") {
            if n.ends_with("/HEAD") {
                continue;
            }
            // Longest remote name that prefixes this ref.
            let remote = remotes
                .iter()
                .filter(|r| n.starts_with(&format!("{r}/")))
                .max_by_key(|r| r.len())
                .cloned()
                .unwrap_or_else(|| n.split('/').next().unwrap_or("").to_string());
            b.name = n[(remote.len() + 1).min(n.len())..].to_string();
            b.remote = remote;
            b.is_local = false;
        } else if refname.starts_with('(') || refname == "HEAD" {
            // Detached HEAD line, e.g. "(HEAD detached at 1a2b3c)".
            b.is_local = true;
            b.is_detached = true;
            b.name = refname.trim_matches(['(', ')']).to_string();
        } else {
            continue;
        }
        list.push(b);
    }
    list
}

pub fn query_tags(repo: &Path) -> Vec<Tag> {
    const B: &str = "----- BOUNDARY OF TAGS -----";
    let fmt = format!(
        "--format={B}%(refname)%00%(objecttype)%00%(objectname)%00%(*objectname)%00%(taggername)±%(taggeremail:trim)%00%(creatordate:unix)%00%(contents:subject)%0a%0a%(contents:body)"
    );
    let o = run(repo, &["tag", "-l", "--sort=-creatordate", &fmt]);
    parse_tags(&o.stdout, B)
}

pub fn parse_tags(out: &str, boundary: &str) -> Vec<Tag> {
    let mut tags = Vec::new();
    for rec in out.split(boundary) {
        let p: Vec<&str> = rec.splitn(7, '\0').collect();
        if p.len() != 7 {
            continue;
        }
        let name = p[0].strip_prefix("refs/tags/").unwrap_or(p[0]).to_string();
        let annotated = p[1] == "tag";
        let sha = if annotated && !p[3].is_empty() { p[3] } else { p[2] };
        tags.push(Tag {
            name,
            sha: sha.to_string(),
            is_annotated: annotated,
            tagger: if annotated { User::parse(p[4]) } else { User::default() },
            date: p[5].parse().unwrap_or(0),
            message: if annotated { p[6].trim().to_string() } else { String::new() },
        });
    }
    tags
}

pub fn query_remotes(repo: &Path) -> Vec<Remote> {
    let o = run(repo, &["remote", "-v"]);
    parse_remotes(&o.stdout)
}

pub fn parse_remotes(out: &str) -> Vec<Remote> {
    let mut list: Vec<Remote> = Vec::new();
    for line in out.lines() {
        let mut it = line.split_whitespace();
        let (Some(name), Some(url), kind) = (it.next(), it.next(), it.next()) else { continue };
        let idx = match list.iter().position(|r| r.name == name) {
            Some(i) => i,
            None => {
                list.push(Remote { name: name.into(), ..Default::default() });
                list.len() - 1
            }
        };
        if kind == Some("(push)") {
            list[idx].push_url = url.into();
        } else {
            list[idx].url = url.into();
        }
    }
    list
}

pub fn query_stashes(repo: &Path) -> Vec<Stash> {
    let o = run(repo, &["stash", "list", "-z", "--no-show-signature", "--format=%H%n%P%n%ct%n%gd%n%B"]);
    parse_stashes(&o.stdout)
}

pub fn parse_stashes(out: &str) -> Vec<Stash> {
    let mut list = Vec::new();
    for rec in out.split('\0') {
        let mut lines = rec.splitn(5, '\n');
        let (Some(sha), Some(parents), Some(time), Some(name)) = (lines.next(), lines.next(), lines.next(), lines.next()) else {
            continue;
        };
        if sha.trim().is_empty() {
            continue;
        }
        list.push(Stash {
            sha: sha.trim().to_string(),
            parents: parents.split_whitespace().map(str::to_string).collect(),
            time: time.trim().parse().unwrap_or(0),
            name: name.trim().to_string(),
            message: lines.next().unwrap_or("").trim().to_string(),
        });
    }
    list
}

pub fn query_submodules(repo: &Path) -> Vec<Submodule> {
    let o = run(repo, &["submodule", "status"]);
    let mut list = Vec::new();
    for line in o.stdout.lines() {
        if line.len() < 2 {
            continue;
        }
        let flag = line.chars().next().unwrap_or(' ');
        let rest = &line[1..];
        let mut it = rest.splitn(2, ' ');
        let sha = it.next().unwrap_or("").to_string();
        let path_part = it.next().unwrap_or("");
        let path = match path_part.rfind(" (") {
            Some(i) if path_part.ends_with(')') => &path_part[..i],
            _ => path_part,
        };
        list.push(Submodule {
            path: path.to_string(),
            sha,
            url: String::new(),
            branch: String::new(),
            status: match flag {
                '-' => SubmoduleStatus::NotInitialized,
                '+' => SubmoduleStatus::RevisionChanged,
                'U' => SubmoduleStatus::Unmerged,
                _ => SubmoduleStatus::Normal,
            },
        });
    }
    if list.is_empty() {
        return list;
    }
    // URLs / branches from .gitmodules
    let cfg = run(repo, &["config", "--file", ".gitmodules", "--list"]);
    let mut names: Vec<(String, String, String)> = Vec::new(); // name, key, value
    for l in cfg.stdout.lines() {
        if let Some(rest) = l.strip_prefix("submodule.") {
            if let Some((k, v)) = rest.split_once('=') {
                if let Some(dot) = k.rfind('.') {
                    names.push((k[..dot].to_string(), k[dot + 1..].to_string(), v.to_string()));
                }
            }
        }
    }
    for s in list.iter_mut() {
        let name = names.iter().find(|(_, k, v)| k == "path" && v == &s.path).map(|x| x.0.clone());
        if let Some(name) = name {
            for (n, k, v) in &names {
                if n == &name && k == "url" {
                    s.url = v.clone();
                }
                if n == &name && k == "branch" {
                    s.branch = v.clone();
                }
            }
        }
    }
    // Dirty submodules
    let paths: Vec<&str> = list.iter().filter(|s| s.status == SubmoduleStatus::Normal).map(|s| s.path.as_str()).collect();
    if !paths.is_empty() {
        let mut args = vec!["--no-optional-locks", "status", "--porcelain", "--"];
        args.extend(paths.iter());
        let st = run(repo, &args);
        for l in st.stdout.lines() {
            if l.len() > 3 {
                let p = l[3..].trim();
                if let Some(s) = list.iter_mut().find(|s| s.path == p && s.status == SubmoduleStatus::Normal) {
                    s.status = SubmoduleStatus::Modified;
                }
            }
        }
    }
    list
}

pub fn query_worktrees(repo: &Path) -> Vec<Worktree> {
    let o = run(repo, &["worktree", "list", "--porcelain"]);
    parse_worktrees(&o.stdout)
}

pub fn parse_worktrees(out: &str) -> Vec<Worktree> {
    let mut list: Vec<Worktree> = Vec::new();
    for line in out.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            list.push(Worktree { path: p.to_string(), ..Default::default() });
            continue;
        }
        let Some(w) = list.last_mut() else { continue };
        if line == "bare" {
            w.is_bare = true;
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            w.head = h.to_string();
        } else if let Some(b) = line.strip_prefix("branch ") {
            w.branch = b.to_string();
        } else if line == "detached" {
            w.is_detached = true;
        } else if line.starts_with("locked") {
            w.is_locked = true;
        } else if line.starts_with("prunable") {
            w.is_prunable = true;
        }
    }
    list
}

/// `git status --porcelain -z` (v1).
pub fn query_local_changes(repo: &Path, include_untracked: bool) -> Vec<Change> {
    let o = if include_untracked {
        run(
            repo,
            &[
                "--no-optional-locks",
                "-c",
                "core.untrackedCache=true",
                "-c",
                "status.showUntrackedFiles=all",
                "status",
                "-uall",
                "--ignore-submodules=dirty",
                "--porcelain",
                "-z",
            ],
        )
    } else {
        run(repo, &["--no-optional-locks", "status", "-uno", "--ignore-submodules=dirty", "--porcelain", "-z"])
    };
    parse_status_z(&o.stdout)
}

pub fn parse_status_z(out: &str) -> Vec<Change> {
    let mut list = Vec::new();
    let mut it = out.split('\0');
    while let Some(entry) = it.next() {
        if entry.len() < 4 {
            continue;
        }
        let code = &entry[..2];
        let path = &entry[3..];
        let x = code.chars().next().unwrap_or(' ');
        let y = code.chars().nth(1).unwrap_or(' ');
        let mut c = Change {
            path: path.to_string(),
            original_path: String::new(),
            index: ChangeState::None,
            worktree: ChangeState::None,
            code: code.to_string(),
        };
        if x == 'R' || x == 'C' {
            c.original_path = it.next().unwrap_or("").to_string();
        }
        let conflicted = matches!(code, "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU");
        if conflicted {
            c.index = ChangeState::Conflicted;
            c.worktree = ChangeState::Conflicted;
        } else if code == "??" {
            c.worktree = ChangeState::Untracked;
        } else {
            c.index = ChangeState::from_char(x);
            c.worktree = ChangeState::from_char(y);
        }
        list.push(c);
    }
    list
}

/// `--name-status -z` output (used by diff-tree / diff / stash show).
pub fn parse_name_status_z(out: &str) -> Vec<Change> {
    let mut list = Vec::new();
    let mut it = out.split('\0').filter(|s| !s.is_empty());
    while let Some(status) = it.next() {
        let c = status.chars().next().unwrap_or(' ');
        let state = ChangeState::from_char(c);
        if state == ChangeState::Renamed || state == ChangeState::Copied {
            let orig = it.next().unwrap_or("");
            let new = it.next().unwrap_or("");
            list.push(Change { original_path: orig.to_string(), ..Change::simple(new, state) });
        } else {
            let p = it.next().unwrap_or("");
            list.push(Change::simple(p, state));
        }
    }
    list
}

/// Files changed by a commit (against its first parent, or the empty tree for roots).
pub fn query_commit_changes(repo: &Path, commit: &str, parent: Option<&str>) -> Vec<Change> {
    let o = match parent {
        Some(p) => run(repo, &["diff", "--name-status", "-M", "-z", p, commit]),
        None => run(repo, &["diff-tree", "-r", "--root", "--no-commit-id", "--name-status", "-M", "-z", commit]),
    };
    let mut v = parse_name_status_z(&o.stdout);
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

pub fn compare_revisions(repo: &Path, from: &str, to: &str) -> Vec<Change> {
    let o = if to.is_empty() {
        run(repo, &["diff", "--name-status", "-M", "-z", from])
    } else {
        run(repo, &["diff", "--name-status", "-M", "-z", from, to])
    };
    let mut v = parse_name_status_z(&o.stdout);
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

pub fn query_stash_changes(repo: &Path, stash: &Stash) -> Vec<Change> {
    let mut v = if let Some(p) = stash.parents.first() {
        parse_name_status_z(&run(repo, &["diff", "--name-status", "-M", "-z", p, &stash.sha]).stdout)
    } else {
        Vec::new()
    };
    // Untracked files are stored in the third parent.
    if let Some(u) = stash.parents.get(2) {
        let o = run(repo, &["diff-tree", "-r", "--root", "--no-commit-id", "--name-status", "-z", u]);
        for mut c in parse_name_status_z(&o.stdout) {
            c.index = ChangeState::Untracked;
            v.push(c);
        }
    }
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

pub fn query_full_message(repo: &Path, sha: &str) -> String {
    run(repo, &["log", "-1", "--no-show-signature", "--format=%B", sha]).stdout.trim_end().to_string()
}

pub fn query_revision_files(repo: &Path, rev: &str) -> Vec<RevisionFile> {
    let o = run(repo, &["ls-tree", "-r", "-z", rev]);
    let mut v = Vec::new();
    for rec in o.stdout.split('\0') {
        // <mode> SP <type> SP <object> TAB <file>
        let Some((meta, path)) = rec.split_once('\t') else { continue };
        let mut m = meta.split(' ');
        let _mode = m.next();
        let kind = m.next().unwrap_or("").to_string();
        v.push(RevisionFile { path: path.to_string(), kind });
    }
    v
}

pub fn query_file_content(repo: &Path, rev: &str, path: &str) -> Result<Vec<u8>, String> {
    let spec = format!("{rev}:{path}");
    let mut cmd = std::process::Command::new("git");
    super::no_window(&mut cmd);
    cmd.current_dir(repo).args(["--no-pager", "show", &spec]);
    match cmd.output() {
        Ok(o) if o.status.success() => Ok(o.stdout),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).into_owned()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn is_binary(data: &[u8]) -> bool {
    data.iter().take(8000).any(|&b| b == 0)
}

pub fn query_file_history(repo: &Path, path: &str, max: usize) -> Vec<Commit> {
    let limit = format!("-{max}");
    let o = run(repo, &["log", &limit, "--no-show-signature", "--decorate=full", LOG_FORMAT, "--follow", "--", path]);
    parse_log(&o.stdout)
}

pub fn blame(repo: &Path, file: &str, rev: &str, ignore_ws: bool) -> Result<Vec<BlameLine>, String> {
    let mut args = vec!["blame", "-f", "-t"];
    if ignore_ws {
        args.push("-w");
    }
    args.push(rev);
    args.push("--");
    args.push(file);
    let o = run(repo, &args);
    if !o.ok {
        return Err(o.stderr.trim().to_string());
    }
    Ok(parse_blame(&o.stdout))
}

/// Parses `git blame -f -t` lines: `^?sha file (author  time tz lineno) content`.
pub fn parse_blame(out: &str) -> Vec<BlameLine> {
    let mut v = Vec::new();
    for line in out.lines() {
        let Some(open) = line.find(" (") else { continue };
        let Some(close_rel) = line[open..].find(") ") .or_else(|| line[open..].find(')')) else { continue };
        let close = open + close_rel;
        let head = &line[..open];
        let mut hp = head.splitn(2, ' ');
        let sha = hp.next().unwrap_or("").trim_start_matches('^').to_string();
        let file = hp.next().unwrap_or("").trim().to_string();
        let meta = &line[open + 2..close];
        // meta: "author name    1700000000 +0100    12"
        let toks: Vec<&str> = meta.split_whitespace().collect();
        if toks.len() < 3 {
            continue;
        }
        let time: i64 = toks[toks.len() - 3].parse().unwrap_or(0);
        let author = toks[..toks.len() - 3].join(" ");
        let content = line.get(close + 2..).unwrap_or("").to_string();
        v.push(BlameLine { sha, author, time, content, file });
    }
    v
}

pub fn in_progress(git_dir: &Path) -> Option<InProgress> {
    if git_dir.join("rebase-merge").is_dir() || git_dir.join("rebase-apply").is_dir() {
        let base = if git_dir.join("rebase-merge").is_dir() { git_dir.join("rebase-merge") } else { git_dir.join("rebase-apply") };
        let head_name = std::fs::read_to_string(base.join("head-name")).unwrap_or_default();
        let onto = std::fs::read_to_string(base.join("onto")).unwrap_or_default();
        return Some(InProgress::Rebase {
            head_name: head_name.trim().trim_start_matches("refs/heads/").to_string(),
            onto: onto.trim().to_string(),
        });
    }
    if git_dir.join("CHERRY_PICK_HEAD").exists() {
        return Some(InProgress::CherryPick);
    }
    if git_dir.join("REVERT_HEAD").exists() {
        return Some(InProgress::Revert);
    }
    if git_dir.join("MERGE_HEAD").exists() {
        return Some(InProgress::Merge);
    }
    if git_dir.join("BISECT_LOG").exists() {
        return Some(InProgress::Bisect);
    }
    None
}

pub fn config_get(repo: Option<&Path>, key: &str, global: bool) -> String {
    let args: Vec<&str> = if global { vec!["config", "--global", "--get", key] } else { vec!["config", "--get", key] };
    super::run_in(repo, &args).stdout.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decorators() {
        let d = parse_decorators("HEAD -> refs/heads/main, tag: refs/tags/v1.0, refs/remotes/origin/main, refs/remotes/origin/HEAD");
        assert_eq!(d.len(), 3);
        assert_eq!(d[0], Decorator { kind: DecoratorKind::CurrentBranchHead, name: "main".into() });
        assert_eq!(d[1].kind, DecoratorKind::RemoteBranchHead);
        assert_eq!(d[2], Decorator { kind: DecoratorKind::Tag, name: "v1.0".into() });
    }

    #[test]
    fn log_line() {
        let l = "abc\0p1 p2\0HEAD\0Leo±leo@x.cn\0100\0Bob±b@y\0200\0fix: thing";
        let c = parse_log_line(l).unwrap();
        assert_eq!(c.parents.len(), 2);
        assert_eq!(c.author.email, "leo@x.cn");
        assert_eq!(c.committer_time, 200);
        assert!(c.is_current_head());
    }

    #[test]
    fn status_z() {
        let out = " M a.txt\0R  new.rs\0old.rs\0?? u.txt\0UU c.txt\0";
        let v = parse_status_z(out);
        assert_eq!(v.len(), 4);
        assert_eq!(v[0].worktree, ChangeState::Modified);
        assert_eq!(v[1].index, ChangeState::Renamed);
        assert_eq!(v[1].original_path, "old.rs");
        assert_eq!(v[2].worktree, ChangeState::Untracked);
        assert!(v[3].is_conflicted());
    }

    #[test]
    fn branches() {
        let out = "refs/heads/main\x001700\x00aaa\x00*\x00refs/remotes/origin/main\x00=\x00\nrefs/remotes/origin/main\x001700\x00aaa\x00 \x00\x00\x00\nrefs/remotes/origin/HEAD\x001700\x00aaa\x00 \x00\x00\x00\n";
        let b = parse_branches(out, &["origin".to_string()]);
        assert_eq!(b.len(), 2);
        assert!(b[0].is_current && b[0].is_local);
        assert_eq!(b[1].remote, "origin");
        assert_eq!(b[1].friendly_name(), "origin/main");
    }

    #[test]
    fn blame_line() {
        let out = "^1a2b3c4 src/main.rs (Leo Nguyen 1700000000 +0700  1) fn main() {}\n";
        let v = parse_blame(out);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].author, "Leo Nguyen");
        assert_eq!(v[0].time, 1700000000);
        assert_eq!(v[0].content, "fn main() {}");
        assert_eq!(v[0].file, "src/main.rs");
    }

    #[test]
    fn worktrees() {
        let out = "worktree /a\nHEAD 111\nbranch refs/heads/main\n\nworktree /b\nHEAD 222\ndetached\nlocked\n";
        let w = parse_worktrees(out);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].branch_short(), "main");
        assert!(w[1].is_detached && w[1].is_locked);
    }

    #[test]
    fn stashes() {
        let out = "s1\np1 p2\n100\nstash@{0}\nOn main: wip\0s2\np3\n90\nstash@{1}\nWIP on dev\n";
        let s = parse_stashes(out);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].name, "stash@{0}");
        assert_eq!(s[0].subject(), "On main: wip");
    }
}
