//! `--self-test <out.json>`: drives every core feature against throw-away repositories,
//! renders every page headlessly, and writes a step-by-step verdict (craft-anything contract).

use crate::git::{self, ops, parse, ChangeState, DecoratorKind};
use crate::graph::DotKind;
use crate::prefs::Preferences;
use crate::repo::{HistoryScope, LoadOptions, Repository};
use crate::ui::repo_view::{Page, RepoView};
use serde_json::json;
use std::path::{Path, PathBuf};

struct Steps {
    steps: Vec<serde_json::Value>,
}

impl Steps {
    fn add(&mut self, step: &str, passed: bool, detail: String) {
        println!("[{}] {step}: {detail}", if passed { "PASS" } else { "FAIL" });
        self.steps.push(json!({ "step": step, "passed": passed, "detail": detail }));
    }
}

fn sh(dir: &Path, args: &[&str]) -> String {
    let o = git::exec(dir, args);
    if !o.ok {
        eprintln!("git {:?} failed: {}", args, o.stderr);
    }
    o.stdout
}

fn write(dir: &Path, file: &str, content: &str) {
    let p = dir.join(file);
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(p, content).unwrap();
}

fn opts() -> LoadOptions {
    LoadOptions { max_commits: crate::consts::MAX_HISTORY_COMMITS, show_untracked: true, first_parent: false, topo: false }
}

fn setup_identity(dir: &Path) {
    sh(dir, &["config", "user.name", "Self Test"]);
    sh(dir, &["config", "user.email", "selftest@example.invalid"]);
    sh(dir, &["config", "commit.gpgsign", "false"]);
    sh(dir, &["config", "tag.gpgsign", "false"]);
}

pub fn run(out: &Path) -> bool {
    let root = std::env::temp_dir().join(format!("sourcegit-rs-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // Isolated config so nothing touches the user's preferences.
    // SAFETY: single-threaded at this point (self-test runs before any UI/thread starts).
    unsafe { std::env::set_var("SOURCEGIT_RS_CONFIG", root.join("config")) };
    let r = run_inner(&root);
    let _ = std::fs::remove_dir_all(&root);
    let all = !r.steps.is_empty() && r.steps.iter().all(|s| s["passed"].as_bool() == Some(true));
    let doc = json!({ "version": 1, "steps": r.steps });
    std::fs::write(out, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    println!("self-test {} → {}", if all { "PASSED" } else { "FAILED" }, out.display());
    all
}

fn run_inner(root: &Path) -> Steps {
    let mut s = Steps { steps: Vec::new() };
    let work: PathBuf = root.join("work");
    std::fs::create_dir_all(&work).unwrap();

    // 1. init + first commit
    let o = ops::init(&work);
    setup_identity(&work);
    sh(&work, &["checkout", "-q", "-b", "main"]);
    write(&work, "README.md", "# demo\n");
    write(&work, "src/lib.rs", &(1..=40).map(|i| format!("line {i}\n")).collect::<String>());
    ops::stage(&work, &[]);
    let c = ops::commit(&work, "feat: initial commit", false, false, false, false);
    let mut repo = Repository::open(work.clone()).expect("open repo");
    repo.refresh_all(&opts());
    s.add("init_commit", o.ok && c.ok && repo.data.commits.len() == 1, format!("init ok={}, commit ok={}, commits loaded={}", o.ok, c.ok, repo.data.commits.len()));

    // 2. branches + merge → graph
    sh(&work, &["checkout", "-q", "-b", "feature/a"]);
    write(&work, "a.txt", "a\n");
    ops::stage(&work, &[]);
    ops::commit(&work, "feat(a): add a", false, false, false, false);
    ops::checkout(&work, "main");
    write(&work, "b.txt", "b\n");
    ops::stage(&work, &[]);
    ops::commit(&work, "fix: add b with `code` #12", false, false, false, false);
    let m = ops::merge(&work, "feature/a", ops::MergeMode::NoFastForward, None);
    repo.refresh_all(&opts());
    // HEAD is the merge commit, so its dot is drawn as Head (Head wins over Merge, as in SourceGit).
    let merges = repo.data.commits.iter().filter(|c| c.parents.len() == 2).count();
    let heads = repo.data.graph.dots.iter().filter(|d| d.kind == DotKind::Head).count();
    let lanes: std::collections::BTreeSet<i32> = repo.data.graph.dots.iter().map(|d| d.center.x as i32).collect();
    s.add(
        "history_graph",
        m.ok && repo.data.commits.len() == 4 && merges == 1 && heads == 1 && lanes.len() >= 2,
        format!("merge ok={}, commits={}, merge commits={}, head dots={}, lanes={}, paths={}, links={}", m.ok, repo.data.commits.len(), merges, heads, lanes.len(), repo.data.graph.paths.len(), repo.data.graph.links.len()),
    );

    // 3. decorators + branch list
    let head = &repo.data.commits[0];
    let cur = repo.current_branch().map(|b| b.name.clone()).unwrap_or_default();
    let deco_ok = head.decorators.iter().any(|d| d.kind == DecoratorKind::CurrentBranchHead && d.name == "main");
    s.add("branches", deco_ok && cur == "main" && repo.local_branches().count() == 2, format!("current={cur}, local branches={}, head decorators={:?}", repo.local_branches().count(), head.decorators.iter().map(|d| d.name.clone()).collect::<Vec<_>>()));

    // 4. status parsing
    write(&work, "README.md", "# demo\nchanged\n");
    write(&work, "new.txt", "untracked\n");
    repo.refresh_changes(true);
    let unstaged = repo.unstaged();
    let untracked = unstaged.iter().filter(|c| c.worktree == ChangeState::Untracked).count();
    s.add("status", unstaged.len() == 2 && untracked == 1, format!("unstaged={}, untracked={}", unstaged.len(), untracked));

    // 5. stage / unstage / discard file
    ops::stage(&work, &["README.md".into()]);
    repo.refresh_changes(true);
    let staged1 = repo.staged().len();
    ops::unstage(&work, &["README.md".into()], true);
    repo.refresh_changes(true);
    let staged2 = repo.staged().len();
    let to_discard: Vec<_> = repo.unstaged();
    let d = ops::discard(&work, &to_discard);
    repo.refresh_changes(true);
    s.add("stage_unstage_discard", staged1 == 1 && staged2 == 0 && d.is_ok() && repo.data.changes.is_empty(), format!("staged after add={staged1}, after unstage={staged2}, changes after discard={}", repo.data.changes.len()));

    // 6. hunk + line staging
    let mut lines: Vec<String> = (1..=40).map(|i| format!("line {i}")).collect();
    lines[1] = "line 2 CHANGED".into();
    lines[35] = "line 36 CHANGED".into();
    write(&work, "src/lib.rs", &(lines.join("\n") + "\n"));
    let prefs = Preferences::default();
    let target = ops::DiffTarget::Unstaged { path: "src/lib.rs".into(), untracked: false };
    let diff = ops::diff(&work, &target, ops::DiffOptions { context: prefs.diff_context, ignore_whitespace: false });
    let mut hunk_ok = false;
    let mut detail = String::from("no text diff");
    if let git::DiffContent::Text(td) = &diff {
        let patch = ops::hunk_patch(td, 0).unwrap_or_default();
        let a = ops::apply_patch_text(&work, &patch, true, false);
        let staged_diff = ops::diff(&work, &ops::DiffTarget::Staged { path: "src/lib.rs".into(), original: String::new() }, ops::DiffOptions { context: 4, ignore_whitespace: false });
        let unstaged_diff = ops::diff(&work, &target, ops::DiffOptions { context: 4, ignore_whitespace: false });
        let sh_ = if let git::DiffContent::Text(t) = &staged_diff { t.hunks.len() } else { 0 };
        let uh = if let git::DiffContent::Text(t) = &unstaged_diff { t.hunks.len() } else { 0 };
        hunk_ok = a.ok && td.hunks.len() == 2 && sh_ == 1 && uh == 1;
        detail = format!("hunks before={}, apply ok={}, staged hunks={sh_}, unstaged hunks={uh}", td.hunks.len(), a.ok);
    }
    s.add("stage_hunk", hunk_ok, detail);

    // line-level: unstage only the deleted line of the staged hunk → reverse partial
    let staged_diff = ops::diff(&work, &ops::DiffTarget::Staged { path: "src/lib.rs".into(), original: String::new() }, ops::DiffOptions { context: 4, ignore_whitespace: false });
    let mut line_ok = false;
    let mut detail = String::from("no staged diff");
    if let git::DiffContent::Text(td) = &staged_diff {
        let added: std::collections::BTreeSet<usize> = td.lines.iter().enumerate().filter(|(_, l)| l.kind == git::LineKind::Added).map(|(i, _)| i).collect();
        let patch = ops::lines_patch(td, &added, true).unwrap_or_default();
        let a = ops::apply_patch_text(&work, &patch, true, true);
        let after = ops::diff(&work, &ops::DiffTarget::Staged { path: "src/lib.rs".into(), original: String::new() }, ops::DiffOptions { context: 4, ignore_whitespace: false });
        let (adds, dels) = if let git::DiffContent::Text(t) = &after { (t.added, t.deleted) } else { (0, 0) };
        line_ok = a.ok && adds == 0 && dels == 1;
        detail = format!("unstage selected '+' lines ok={}, staged now +{adds}/-{dels}", a.ok);
    }
    s.add("stage_lines", line_ok, detail);
    ops::discard_all(&work, true, false).ok();

    // 7. commit amend + message
    write(&work, "c.txt", "c\n");
    ops::stage(&work, &[]);
    ops::commit(&work, "chore: c", false, false, false, false);
    let am = ops::commit(&work, "chore: c (amended)\n\nbody line", true, true, true, false);
    let msg = parse::query_full_message(&work, "HEAD");
    s.add("commit_amend", am.ok && msg.starts_with("chore: c (amended)") && msg.contains("Signed-off-by"), format!("amend ok={}, message first line='{}'", am.ok, msg.lines().next().unwrap_or("")));

    // 8. stash
    write(&work, "c.txt", "c changed\n");
    write(&work, "u.txt", "untracked\n");
    let sp = ops::stash_push(&work, "wip stash", true, false, false, &[]);
    repo.refresh_all(&opts());
    let n_stash = repo.data.stashes.len();
    let stash = repo.data.stashes.first().cloned();
    let stash_changes = stash.as_ref().map(|st| parse::query_stash_changes(&work, st)).unwrap_or_default();
    let clean = repo.data.changes.is_empty();
    let ap = ops::stash_pop(&work, "stash@{0}");
    repo.refresh_all(&opts());
    s.add(
        "stash",
        sp.ok && n_stash == 1 && clean && stash_changes.len() == 2 && ap.ok && repo.data.stashes.is_empty() && repo.data.changes.len() == 2,
        format!("push ok={}, stashes={n_stash}, stash files={}, clean after push={clean}, pop ok={}, changes restored={}", sp.ok, stash_changes.len(), ap.ok, repo.data.changes.len()),
    );
    ops::discard_all(&work, true, false).ok();

    // 9. tags + branch rename/delete
    let t1 = ops::create_tag(&work, "v1.0.0", "HEAD", Some("release 1"), false);
    let t2 = ops::create_tag(&work, "light", "HEAD~1", None, false);
    let rn = ops::rename_branch(&work, "feature/a", "feature/b");
    repo.refresh_all(&opts());
    let tags = repo.data.tags.clone();
    let annotated = tags.iter().find(|t| t.name == "v1.0.0").map(|t| t.is_annotated && t.message == "release 1").unwrap_or(false);
    let renamed = repo.local_branches().any(|b| b.name == "feature/b");
    let del = ops::delete_branch(&work, "feature/b", false);
    let dt = ops::delete_tag(&work, "light");
    repo.refresh_all(&opts());
    s.add(
        "tags_branches",
        t1.ok && t2.ok && annotated && rn.ok && renamed && del.ok && dt.ok && repo.data.tags.len() == 1 && repo.local_branches().count() == 1,
        format!("tags created={}, annotated parsed={annotated}, renamed={renamed}, deleted ok={}, tags left={}, branches left={}", tags.len(), del.ok, repo.data.tags.len(), repo.local_branches().count()),
    );

    // 10. remotes: push / fetch / pull with ahead-behind
    let bare = root.join("remote.git");
    git::exec_in(Some(root), &["init", "--bare", "-q", bare.to_str().unwrap()]);
    // Point the bare repo HEAD at main so clones / submodules check it out.
    git::exec(&bare, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    let ar = ops::add_remote(&work, "origin", bare.to_str().unwrap());
    let ps = ops::push(&work, &ops::PushOptions { remote: "origin", local: "main", remote_branch: "main", set_upstream: true, force: false, tags: true, no_verify: false });
    let clone_dir = root.join("clone2");
    let cl = ops::clone(bare.to_str().unwrap(), root, "clone2", "", false);
    setup_identity(&clone_dir);
    write(&clone_dir, "remote.txt", "from clone\n");
    ops::stage(&clone_dir, &[]);
    ops::commit(&clone_dir, "feat: remote change", false, false, false, false);
    git::exec(&clone_dir, &["push", "-q", "origin", "main"]);
    let fe = ops::fetch(&work, Some("origin"), true, false);
    repo.refresh_all(&opts());
    let behind = repo.current_branch().map(|b| b.behind.len()).unwrap_or(99);
    let remote_branches = repo.remote_branches("origin").len();
    let pl = ops::pull(&work, "origin", "main", false, true);
    repo.refresh_all(&opts());
    let behind_after = repo.current_branch().map(|b| b.behind.len()).unwrap_or(99);
    s.add(
        "remote_push_fetch_pull",
        ar.ok && ps.ok && cl.ok && fe.ok && behind == 1 && remote_branches == 1 && pl.ok && behind_after == 0 && work.join("remote.txt").exists(),
        format!("add ok={}, push ok={}, clone ok={}, fetch ok={}, behind after fetch={behind}, remote branches={remote_branches}, pull ok={}, behind after pull={behind_after}", ar.ok, ps.ok, cl.ok, fe.ok, pl.ok),
    );

    // 11. cherry-pick / revert / reset
    sh(&work, &["checkout", "-q", "-b", "topic"]);
    write(&work, "topic.txt", "t\n");
    ops::stage(&work, &[]);
    ops::commit(&work, "feat: topic", false, false, false, false);
    let topic_sha = git::run(&work, &["rev-parse", "HEAD"]).stdout.trim().to_string();
    ops::checkout(&work, "main");
    let cp = ops::cherry_pick(&work, &[topic_sha.clone()], false, true, None);
    let has_topic = work.join("topic.txt").exists();
    let rv = ops::revert(&work, "HEAD", true, false);
    let reverted = !work.join("topic.txt").exists();
    let before = git::run(&work, &["rev-list", "--count", "HEAD"]).stdout.trim().parse::<i32>().unwrap_or(0);
    let rs = ops::reset(&work, "HEAD~2", ops::ResetMode::Hard);
    let after = git::run(&work, &["rev-list", "--count", "HEAD"]).stdout.trim().parse::<i32>().unwrap_or(0);
    s.add("cherry_pick_revert_reset", cp.ok && has_topic && rv.ok && reverted && rs.ok && before - after == 2, format!("cherry-pick ok={} (file present={has_topic}), revert ok={} (file removed={reverted}), reset --hard HEAD~2: {before}→{after} commits", cp.ok, rv.ok));

    // 12. conflict → in-progress detection → resolve with theirs → continue
    sh(&work, &["checkout", "-q", "-b", "conflict"]);
    write(&work, "README.md", "# conflict side\n");
    ops::stage(&work, &[]);
    ops::commit(&work, "conflict side", false, false, false, false);
    ops::checkout(&work, "main");
    write(&work, "README.md", "# main side\n");
    ops::stage(&work, &[]);
    ops::commit(&work, "main side", false, false, false, false);
    let mg = ops::merge(&work, "conflict", ops::MergeMode::Default, None);
    repo.refresh_all(&opts());
    let in_merge = matches!(repo.data.in_progress, Some(git::InProgress::Merge));
    let conflicts = repo.conflicts();
    let us = ops::use_side(&work, "README.md", true);
    repo.refresh_all(&opts());
    let conflicts_after = repo.conflicts();
    let co = ops::continue_op(&work, &git::InProgress::Merge);
    repo.refresh_all(&opts());
    let content = std::fs::read_to_string(work.join("README.md")).unwrap_or_default();
    s.add(
        "conflict_resolve",
        !mg.ok && in_merge && conflicts == 1 && us.ok && conflicts_after == 0 && co.ok && repo.data.in_progress.is_none() && content.contains("conflict side"),
        format!("merge failed as expected={}, in-progress=merge:{in_merge}, conflicts={conflicts}→{conflicts_after}, continue ok={}, result='{}'", !mg.ok, co.ok, content.trim()),
    );

    // rebase conflict → abort
    sh(&work, &["checkout", "-q", "-b", "rb", "HEAD~1"]);
    write(&work, "README.md", "# rebase side\n");
    ops::stage(&work, &[]);
    ops::commit(&work, "rebase side", false, false, false, false);
    let rb = ops::rebase(&work, "main", false);
    repo.refresh_all(&opts());
    let in_rebase = matches!(repo.data.in_progress, Some(git::InProgress::Rebase { .. }));
    let ab = ops::abort_op(&work, repo.data.in_progress.as_ref().unwrap_or(&git::InProgress::Merge));
    repo.refresh_all(&opts());
    s.add("rebase_abort", !rb.ok && in_rebase && ab.ok && repo.data.in_progress.is_none(), format!("rebase stopped={}, detected rebase={in_rebase}, abort ok={}", !rb.ok, ab.ok));
    ops::checkout(&work, "main");

    // 13. blame / file history / search / compare
    let bl = parse::blame(&work, "src/lib.rs", "HEAD", false).unwrap_or_default();
    let fh = parse::query_file_history(&work, "README.md", 100);
    let found = parse::search_commits(&work, parse::SearchMethod::Message, "remote change", false);
    let by_author = parse::search_commits(&work, parse::SearchMethod::Author, "self test", false);
    let cmp = parse::compare_revisions(&work, "HEAD~3", "HEAD");
    s.add(
        "blame_history_search_compare",
        bl.len() == 40 && fh.len() >= 3 && found.len() == 1 && !by_author.is_empty() && !cmp.is_empty(),
        format!("blame lines={}, README history={}, search 'remote change'={}, by author={}, compare HEAD~3..HEAD files={}", bl.len(), fh.len(), found.len(), by_author.len(), cmp.len()),
    );

    // 14. worktree + submodule
    let wt_path = root.join("wt1");
    let wa = ops::worktree_add(&work, wt_path.to_str().unwrap(), "wt-branch", true, None);
    repo.refresh_all(&opts());
    let wts = repo.data.worktrees.len();
    let wt_branch = repo.data.worktrees.first().map(|w| w.branch_short()).unwrap_or_default();
    let wr = ops::worktree_remove(&work, wt_path.to_str().unwrap(), true);
    let sm = ops::submodule_add(&work, bare.to_str().unwrap(), "libs/sub", false);
    repo.refresh_all(&opts());
    let subs = repo.data.submodules.len();
    s.add("worktree_submodule", wa.ok && wts == 1 && wt_branch == "wt-branch" && wr.ok && sm.ok && subs == 1, format!("worktree add ok={}, listed={wts} ({wt_branch}), remove ok={}, submodule add ok={} ({}), submodules={subs}", wa.ok, wr.ok, sm.ok, sm.stderr.trim().lines().last().unwrap_or("")));
    ops::commit(&work, "chore: add submodule", false, false, false, false);

    // 15. history scope filter
    repo.scope = HistoryScope::CurrentBranch;
    repo.refresh_commits(&opts());
    let cur_only = repo.data.commits.len();
    repo.scope = HistoryScope::All;
    repo.refresh_commits(&opts());
    let all = repo.data.commits.len();
    let merge_dots = repo.data.graph.dots.iter().filter(|d| d.kind == DotKind::Merge).count();
    s.add("history_scope", cur_only > 0 && all > cur_only && merge_dots >= 1, format!("current-branch commits={cur_only}, all-branch commits={all}, merge dots below HEAD={merge_dots}"));

    // 16. headless render of every page (no panics, widgets laid out)
    let mut prefs = Preferences::default();
    let mut view = RepoView::new(Repository::open(work.clone()).unwrap(), &prefs);
    write(&work, "README.md", "# dirty for render\n");
    view.refresh(&prefs);
    let ctx = eframe::egui::Context::default();
    let pal = crate::theme::palette(prefs.theme);
    let mut frames = 0;
    let mut shapes = 0usize;
    for page in [Page::Histories, Page::WorkingCopy, Page::Stashes] {
        view.page = page;
        for _ in 0..3 {
            let input = eframe::egui::RawInput {
                screen_rect: Some(eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(1400.0, 900.0))),
                ..Default::default()
            };
            let out = ctx.run(input, |ctx| {
                crate::theme::apply(ctx, prefs.theme);
                let _ = view.ui(ctx, &pal, &mut prefs);
            });
            shapes += out.shapes.len();
            frames += 1;
        }
    }
    s.add("ui_render", frames == 9 && shapes > 100 && view.detail.is_some(), format!("frames={frames}, shapes={shapes}, commit detail loaded={}, history rows={}", view.detail.is_some(), view.repo.data.commits.len()));

    // 17. command log records mutating commands only
    let log = git::command_log();
    let has_commit = log.iter().any(|e| e.args.contains(" commit "));
    let has_query = log.iter().any(|e| e.args.contains(" status ") && e.args.contains("--porcelain"));
    s.add("command_log", has_commit && !has_query, format!("entries={}, has commit={has_commit}, has status query={has_query}", log.len()));

    s
}
