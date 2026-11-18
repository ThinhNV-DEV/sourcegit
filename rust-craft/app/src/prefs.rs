//! User preferences and the repository list, stored as JSON in the config dir.
use crate::theme::ThemeMode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepoEntry {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub bookmark: usize,
    #[serde(default)]
    pub group: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub theme: ThemeMode,
    pub font_size: f32,
    pub max_history_commits: usize,
    pub default_clone_dir: String,
    pub repositories: Vec<RepoEntry>,
    pub open_tabs: Vec<String>,
    pub active_tab: usize,
    pub show_untracked: bool,
    pub diff_context: usize,
    pub diff_side_by_side: bool,
    pub ignore_whitespace: bool,
    pub relative_time: bool,
    pub sign_off: bool,
    pub fetch_prune: bool,
    pub pull_rebase: bool,
    pub auto_stash: bool,
    pub topo_order: bool,
    pub show_tags_in_graph: bool,
    pub first_parent_only: bool,
    pub only_current_branch: bool,
    pub confirm_dangerous: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            theme: ThemeMode::Dark,
            font_size: 13.0,
            max_history_commits: crate::consts::MAX_HISTORY_COMMITS,
            default_clone_dir: dirs::home_dir().map(|h| h.display().to_string()).unwrap_or_default(),
            repositories: Vec::new(),
            open_tabs: Vec::new(),
            active_tab: 0,
            show_untracked: true,
            diff_context: crate::consts::DIFF_MIN_CONTEXT_LINES,
            diff_side_by_side: false,
            ignore_whitespace: false,
            relative_time: true,
            sign_off: false,
            fetch_prune: true,
            pull_rebase: false,
            auto_stash: true,
            topo_order: false,
            show_tags_in_graph: true,
            first_parent_only: false,
            only_current_branch: false,
            confirm_dangerous: true,
        }
    }
}

pub fn config_dir() -> PathBuf {
    if let Ok(p) = std::env::var("SOURCEGIT_RS_CONFIG") {
        return PathBuf::from(p);
    }
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("SourceGit-rs")
}

impl Preferences {
    pub fn load() -> Preferences {
        let p = config_dir().join("preference.json");
        std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let dir = config_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(dir.join("preference.json"), s);
        }
    }

    pub fn add_repo(&mut self, path: &str) {
        if self.repositories.iter().any(|r| r.path == path) {
            return;
        }
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string());
        self.repositories.push(RepoEntry { path: path.to_string(), name, bookmark: 0, group: String::new() });
    }
}
