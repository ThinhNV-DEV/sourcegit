# Mechanics sheet — sourcegit-rs

Mọi giá trị định hình hành vi/bố cục. Code dùng đúng các con số này (xem `src/consts.rs`).

## history
- max history commits: 20000 — `src/ViewModels/Preferences.cs:821`
- short sha length: 10 ký tự — screenshot cột SHA

## layout
- history row height 26px, header 24px — `Histories.axaml:34-35`
- left icon bar 49px — `Repository.axaml:15`
- working copy / stash left panel 300px — `LayoutInfo.cs:80`
- commit detail changes left panel 256px — `LayoutInfo.cs:82`
- sidebar 260px — **estimated**

## graph
- unit width 12, half width 6, start offset 4, pen thickness 2, 10 màu pen — `CommitGraph.cs`

## diff
- context lines tối thiểu 4 — `DiffContext.cs:84`

## commit
- subject guide length 50 — `Preferences.cs:822`

## search
- giới hạn kết quả search commit 1000 — `QueryCommits.cs:23`

## theme
- Dark contents #1C1C1C, Light contents #FFFFFF; toàn bộ bảng màu trong `src/theme.rs` lấy từ `Themes.axaml`.
