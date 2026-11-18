# Capture — SourceGit (C#/Avalonia) v2026.23

## Mode
Deep mode. Target là repo SourceGit nằm ngay trong working copy (fork của user, license MIT — cho phép đọc,
sửa, phân tích). Source gốc chỉ được đọc để lấy *hành vi* (git args, layout số, màu theme, thuật toán graph).
Không có dòng C# nào được paste vào project Rust; mọi thứ viết lại từ đầu. Không dùng asset gốc (icon, logo).

## Nguồn đã dùng
| Điều quan sát | Nguồn |
|---|---|
| Layout chính: tab bar, toolbar, sidebar trái (Local branches / Remotes / Tags / Submodules / Worktrees), bảng history 4 cột (GRAPH & SUBJECT, AUTHOR, SHA, COMMIT TIME), panel commit detail (INFORMATION / CHANGES / FILES) | `reference/theme_dark.png`, `reference/theme_light.png` |
| Thanh icon trái 49px: Histories / Local changes / Stashes | `src/Views/Repository.axaml:15-20` |
| Row height history 26, header 24 | `src/Views/Histories.axaml:34-35` |
| Bảng màu Light/Dark | `src/Resources/Themes.axaml:5-69` |
| 10 màu graph (Orange, ForestGreen, Turquoise, …) | `src/Models/CommitGraph.cs:423-434` |
| Thuật toán layout graph: unitWidth 12, halfWidth 6, offset 4 | `src/Models/CommitGraph.cs:73-80` |
| Format `git log` (8 trường phân cách NUL) | `src/Commands/QueryCommits.cs:15` |
| Giới hạn số commit = 20000 | `src/ViewModels/Preferences.cs:821` |
| Format `git branch`, `git tag`, `git stash list`, `status --porcelain`, `worktree list --porcelain`, `submodule status`, `blame -f -t` | `src/Commands/Query*.cs`, `Worktree.cs`, `Blame.cs` |
| Diff: `diff --no-color --no-ext-diff --full-index --patch --unified=N`, N tối thiểu 4 | `src/Commands/Diff.cs:46-57`, `src/ViewModels/DiffContext.cs:84` |
| Width panel trái working copy/stash 300, commit-detail changes 256 | `src/ViewModels/LayoutInfo.cs:80-83` |
| Subject guide length 50 | `src/ViewModels/Preferences.cs:822` |

## Giới hạn bằng chứng
- Chỉ có 2 screenshot tĩnh; hover state, animation, context menu đầy đủ chỉ suy ra từ code.
- Font gốc (Inter / JetBrains Mono nhúng) không dùng lại; bản Rust dùng font mặc định của egui.
- Icon gốc là path SVG trong `Resources/Icons.axaml` — không copy; thay bằng ký tự/glyph đơn giản.
