# Round 2
- build: pass — 0 warnings, 12/12 unit tests
- self-test: pass 19/19 (`checks/self-test.json`). Contract check: version=1, every step passed. The puzzle/action profiles in craft_check.py are for games and don't apply to this app.
- mechanics: PASS — 18 values, 17 sourced, 1 estimated (sidebar width)
- screenshot: pass — `shots/round2-dark.png`, `round2-light.png`, `round2-changes.png` side by side with `reference/theme_*.png`:
  - same: tab bar → toolbar (fetch/pull/push/stash/patch … current branch on the right) → 49px icon bar → sidebar (LOCAL BRANCHES / REMOTES / TAGS / SUBMODULES / WORKTREES with counts and buttons) → 4-column history (GRAPH & SUBJECT, AUTHOR, SHA, COMMIT TIME) → INFORMATION/CHANGES/FILES panel with AUTHOR, SHA, PARENTS, REFS, MESSAGE and the file list. Lane colors, dot shapes, ref badges with lane-colored borders, inline `code`, #issue links, relative time, Light/Dark palettes taken from Themes.axaml.
  - different: no bold (egui ships no bold font, so the conventional-commit prefix is drawn in a stronger color instead); emoji icons instead of the original SVG icons; letter avatars instead of Gravatar; no native window title bar; tab bar styling is simpler.
- not compared: animations/hover (no video evidence), real drag-drop and clipboard behavior (headless environment).
