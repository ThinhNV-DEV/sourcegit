# Unknowns

## U1 — Width sidebar repository mặc định là bao nhiêu?
- severity: low
- status: open
- probes: đọc binding width của sidebar trong Views/Repository.axaml + LayoutInfo
- meanwhile: 260px (estimated từ screenshot, ~1/6 width cửa sổ)

## U2 — Font chữ
- severity: low
- status: resolved
- resolution: dùng font mặc định của egui (Ubuntu-Light / Hack mono). egui không có bold nên prefix conventional commit dùng màu đậm hơn thay cho chữ đậm — checks/round-2.md

## U3 — Ngoài scope phiên bản này (backlog, user đã chọn "Core đầy đủ")
- severity: medium
- status: open
- probes: n/a
- meanwhile: không build — AI assistant, Git LFS, git-flow, interactive rebase editor, merge conflict editor 3-way, i18n, statistics, custom actions, image diff, self-update, SSH key generator, workspace

## U4 — Avatar tác giả
- severity: low
- status: open
- probes: bản gốc tải Gravatar qua mạng
- meanwhile: ô màu với chữ cái đầu tên (không gọi mạng)

## U5 — Icon
- severity: low
- status: resolved
- resolution: icon gốc là SVG path (không copy). Dùng glyph có trong font egui, đã kiểm tra từng glyph bằng `Fonts::has_glyph` — checks/round-1.md, round-2.md
