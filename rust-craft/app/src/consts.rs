//! Numbers from mechanics.json. Keep them in sync with the sheet.

/// src/ViewModels/Preferences.cs:821
pub const MAX_HISTORY_COMMITS: usize = 20000;
/// src/Views/Histories.axaml:35
pub const HISTORY_ROW_HEIGHT: f32 = 26.0;
/// src/Views/Histories.axaml:34
pub const HISTORY_HEADER_HEIGHT: f32 = 24.0;
/// src/Views/Repository.axaml:15
pub const LEFT_ICON_BAR_WIDTH: f32 = 49.0;
/// src/ViewModels/LayoutInfo.cs:80
pub const WORKING_COPY_LEFT_WIDTH: f32 = 300.0;
/// src/ViewModels/LayoutInfo.cs:82
pub const COMMIT_DETAIL_CHANGES_LEFT_WIDTH: f32 = 256.0;
/// estimated
pub const SIDEBAR_WIDTH: f32 = 260.0;

/// src/Models/CommitGraph.cs:74-89
pub const GRAPH_UNIT_WIDTH: f32 = 12.0;
pub const GRAPH_HALF_WIDTH: f32 = 6.0;
pub const GRAPH_START_OFFSET: f32 = 4.0;
pub const GRAPH_PEN_THICKNESS: f32 = 2.0;
/// src/Models/CommitGraph.cs:423-434 (10 colors)
pub const GRAPH_PEN_COUNT: usize = 10;

/// src/ViewModels/DiffContext.cs:84
pub const DIFF_MIN_CONTEXT_LINES: usize = 4;
/// src/ViewModels/Preferences.cs:822
pub const SUBJECT_GUIDE_LENGTH: usize = 50;
/// src/Commands/QueryCommits.cs:23
pub const COMMIT_SEARCH_LIMIT: usize = 1000;
/// SHA column in reference/theme_dark.png
pub const SHORT_SHA_LENGTH: usize = 10;
