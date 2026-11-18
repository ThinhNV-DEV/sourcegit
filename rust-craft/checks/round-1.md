# Round 1
- build: pass (cargo build, 12 unit tests pass)
- self-test: fail 4/19 — history_graph (test asserted a Merge dot on HEAD; SourceGit draws a Head dot when the merge commit is HEAD → test was wrong), remote_push_fetch_pull / blame_history_search_compare / worktree_submodule (the test bare repo's HEAD pointed at `master`, so the clone had no branch → fixed the fixture)
- screenshot: fail — needed libxkbcommon-x11 in the container; after installing it, `shots/round1-dark.png` came out, but many icons (⎇, ＋, 🩹, ⇔, ␣, ▦) showed as empty boxes because egui's bundled fonts don't have those glyphs
- mechanics: not run
