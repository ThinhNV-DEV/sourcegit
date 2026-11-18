# SourceGit-rs

A Git GUI client written in Rust (egui/eframe), rebuilt from the behavior of SourceGit v2026.23 (MIT). It calls the `git` CLI (≥ 2.25.1) with the same arguments SourceGit uses. The Rust code was written from scratch; none of the original's C# or icons were copied.

## Build & run

```bash
cargo build --release
./target/release/sourcegit-rs [REPO_PATH...]
```

On Linux the window needs X11 or Wayland libs at runtime (`libxkbcommon-x11-0`, `libgl1`).

### Windows portable

Needs Rust (rustup) with the MSVC toolchain (Visual Studio Build Tools, "Desktop development with C++"). Run in `cmd`:

```bat
build-windows-portable.bat            :: x64
build-windows-portable.bat arm64      :: ARM64
build-windows-portable.bat --test     :: run the self-test before packaging
build-windows-portable.bat --clean    :: cargo clean first
```

The script always builds with the `stable-x86_64-pc-windows-msvc` toolchain (installing it if missing), even when the default rustup toolchain is `windows-gnu`. A GNU host toolchain with llvm-mingw cannot link the dependencies' build scripts (`undefined symbol: _Unwind_Resume`). If the MSVC Build Tools are missing, the script stops and prints the `winget` command to install them.

Output goes to `dist\SourceGit-rs-<version>-win-<arch>-portable\` (plus a `.zip`):
`SourceGit-rs.exe`, `README.md`, `LICENSE.txt`, and an empty `data\` folder.
If a `data\` folder sits next to the exe, all settings are written there (portable mode); otherwise they go to `%APPDATA%\SourceGit-rs`.
The exe links the C runtime statically (no VC++ Redistributable needed), has no console window, and does not open a cmd window when it calls git. Machines running it need Git for Windows on PATH.

Test modes (craft-anything contract):

```bash
sourcegit-rs --self-test out.json                     # 19-step end-to-end test on temporary repos
sourcegit-rs --screenshot out.png [--page histories|changes|stashes] [--theme dark|light] [REPO]
```

## Features

- **Workspace**: tabs for multiple repos, a welcome page with the repository list, clone / open / init, drag & drop a folder, session restored on restart, Light/Dark theme, preferences, git command log.
- **History**: commit graph (same lane algorithm as SourceGit), ref badges, filter for all branches / current branch / a single branch, first-parent and topo order, multi-select, keyboard navigation, search by message / author / committer / SHA / path / content.
- **Commit detail**: Information (author, committer, SHA, clickable parents, refs, full message, files), Changes (file list plus diff), Files (browse the tree at a revision, view and save files).
- **Diff**: unified or side-by-side, more/fewer context lines, ignore whitespace, stage/unstage/discard by **hunk** or by **selected lines**.
- **Local changes**: unstaged/staged lists, stage/unstage/discard per file or all at once, add to .gitignore, stash selected files, save as patch, commit / amend / sign-off / no-verify, stage all & commit, conventional-commit templates, 50-character subject guide.
- **Conflicts**: banner for merge/rebase/cherry-pick/revert/bisect in progress with Continue / Skip / Abort; Use mine / Use theirs / Mark resolved / Open merge tool.
- **Branches**: checkout (local, or remote → new tracking branch), create, rename, delete (with its upstream), set tracking branch, fast-forward, merge (5 modes), rebase, compare.
- **Commits**: cherry-pick (one or many, mainline for merges), revert, reset (soft/mixed/merge/keep/hard), reset another branch, detached checkout, create branch/tag here, compare with HEAD / working tree / another commit, bisect, save as patch, archive.
- **Remotes**: add, edit, rename, delete, fetch, prune, open in browser; fetch / pull (rebase, autostash) / push (upstream, force-with-lease, tags, no-verify).
- **Tags**: annotated or lightweight, push, delete (local and remote).
- **Stashes**: push (message, untracked, keep index, staged only), apply / pop / drop / clear, branch from stash, view stash changes and diffs.
- **Submodules**: add, update (init/recursive/remote), deinit, delete, open as a tab.
- **Worktrees**: add, remove, prune, lock/unlock, open as a tab.
- **Tools**: blame, file history (follows renames), apply patch, git gc.

## Not included (backlog, see ../UNKNOWNS.md U3)

AI assistant, Git LFS, git-flow, interactive rebase editor, 3-way merge editor, i18n, statistics, custom actions, image diff, self-update, SSH key generator, workspaces.

## Layout

- `src/git/`: runner, models, parsers (`parse.rs`), diff and write commands (`ops.rs`)
- `src/graph.rs`: commit graph layout
- `src/repo.rs`: repository state
- `src/ui/`: app, repo_view, sidebar, histories, working_copy, stashes, diff_view, popups, widgets
- `src/selftest.rs`: end-to-end self-test
- `MECHANICS.md` / `mechanics.json`: the numbers that drive layout and behavior, each with its source
