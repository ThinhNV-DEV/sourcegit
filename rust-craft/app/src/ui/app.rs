//! Top-level window: tab bar, welcome page, preferences and command log.

use super::repo_view::{AppRequest, Page, RepoView};
use super::widgets;
use crate::git::{self, ops, parse};
use crate::prefs::Preferences;
use crate::repo::Repository;
use crate::theme::{self, Palette, ThemeMode};
use eframe::egui::{self, RichText};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

pub enum Tab {
    Welcome,
    Repo(Box<RepoView>),
}

enum AppPopup {
    Clone { url: String, parent: String, name: String, extra: String, recursive: bool },
    Init { path: String },
}

pub struct ScreenshotCfg {
    pub path: PathBuf,
    pub frames: u32,
    pub requested: bool,
    pub page: Option<Page>,
}

pub struct App {
    pub prefs: Preferences,
    pub tabs: Vec<Tab>,
    pub active: usize,
    welcome_filter: String,
    popup: Option<AppPopup>,
    show_prefs: bool,
    show_log: bool,
    log_selected: Option<usize>,
    global_name: String,
    global_email: String,
    git_version: String,
    applied_theme: Option<(ThemeMode, f32)>,
    task: Option<(String, Receiver<Result<PathBuf, String>>)>,
    errors: Vec<String>,
    frame: u64,
    pub screenshot: Option<ScreenshotCfg>,
}

impl App {
    pub fn new(prefs: Preferences, open: Vec<PathBuf>) -> App {
        let mut app = App {
            prefs,
            tabs: Vec::new(),
            active: 0,
            welcome_filter: String::new(),
            popup: None,
            show_prefs: false,
            show_log: false,
            log_selected: None,
            global_name: parse::config_get(None, "user.name", true),
            global_email: parse::config_get(None, "user.email", true),
            git_version: git::version().unwrap_or_else(|| "git not found".into()),
            applied_theme: None,
            task: None,
            errors: Vec::new(),
            frame: 0,
            screenshot: None,
        };
        let mut to_open: Vec<PathBuf> = open;
        if to_open.is_empty() {
            to_open = app.prefs.open_tabs.iter().map(PathBuf::from).collect();
        }
        for p in to_open {
            app.open_repo(p);
        }
        if app.tabs.is_empty() {
            app.tabs.push(Tab::Welcome);
        }
        app.active = app.prefs.active_tab.min(app.tabs.len() - 1);
        app
    }

    pub fn open_repo(&mut self, path: PathBuf) {
        // Already open? Switch to it.
        let canonical = git::repo_root(&path).unwrap_or(path.clone());
        if let Some(i) = self.tabs.iter().position(|t| matches!(t, Tab::Repo(r) if r.repo.path == canonical)) {
            self.active = i;
            return;
        }
        match Repository::open(path) {
            Ok(repo) => {
                self.prefs.add_repo(&repo.path.display().to_string());
                let view = RepoView::new(repo, &self.prefs);
                // Replace the current welcome tab if that's what is showing.
                if let Some(Tab::Welcome) = self.tabs.get(self.active) {
                    self.tabs[self.active] = Tab::Repo(Box::new(view));
                } else {
                    self.tabs.push(Tab::Repo(Box::new(view)));
                    self.active = self.tabs.len() - 1;
                }
                self.save_session();
            }
            Err(e) => self.errors.push(e),
        }
    }

    fn save_session(&mut self) {
        self.prefs.open_tabs = self
            .tabs
            .iter()
            .filter_map(|t| match t {
                Tab::Repo(r) => Some(r.repo.path.display().to_string()),
                Tab::Welcome => None,
            })
            .collect();
        self.prefs.active_tab = self.active;
        self.prefs.save();
    }

    fn close_tab(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.tabs.remove(i);
        }
        if self.tabs.is_empty() {
            self.tabs.push(Tab::Welcome);
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.save_session();
    }

    fn tab_bar(&mut self, ctx: &egui::Context, pal: &Palette) {
        egui::TopBottomPanel::top("tab-bar")
            .exact_height(38.0)
            .frame(egui::Frame::new().fill(pal.title_bar).inner_margin(egui::Margin::symmetric(8, 4)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if widgets::tool_button(ui, "🏠", "Welcome / repositories").clicked() {
                        self.new_tab();
                    }
                    ui.separator();
                    let mut close = None;
                    let mut activate = None;
                    for (i, t) in self.tabs.iter().enumerate() {
                        let name = match t {
                            Tab::Welcome => "New Page".to_string(),
                            Tab::Repo(r) => r.repo.name.clone(),
                        };
                        let sel = i == self.active;
                        let fill = if sel { pal.window } else { pal.title_bar };
                        egui::Frame::new().fill(fill).corner_radius(egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }).inner_margin(egui::Margin::symmetric(10, 4)).show(ui, |ui| {
                            ui.set_min_width(160.0);
                            ui.horizontal(|ui| {
                                let icon = if matches!(t, Tab::Repo(_)) { "🔖" } else { "✨" };
                                ui.label(RichText::new(icon).color(if sel { theme::GRAPH_COLORS[1] } else { pal.fg2 }));
                                let r = ui.add(egui::Label::new(RichText::new(&name).color(if sel { pal.fg1 } else { pal.fg2 })).sense(egui::Sense::click()));
                                if r.clicked() {
                                    activate = Some(i);
                                }
                                if r.middle_clicked() {
                                    close = Some(i);
                                }
                                if let Tab::Repo(rv) = t {
                                    r.on_hover_text(rv.repo.path.display().to_string());
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.add(egui::Button::new(RichText::new("✖").size(11.0)).frame(false)).clicked() {
                                        close = Some(i);
                                    }
                                });
                            });
                        });
                    }
                    if widgets::tool_button(ui, "➕", "New tab (Ctrl+T)").clicked() {
                        self.new_tab();
                    }
                    if let Some(i) = activate {
                        self.active = i;
                        self.save_session();
                    }
                    if let Some(i) = close {
                        self.close_tab(i);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::tool_button(ui, "⚙", "Preferences").clicked() {
                            self.show_prefs = !self.show_prefs;
                        }
                        if widgets::tool_button(ui, "📜", "Git command log").clicked() {
                            self.show_log = !self.show_log;
                        }
                        let (g, tip) = match self.prefs.theme {
                            ThemeMode::Dark => ("☀", "Switch to light theme"),
                            ThemeMode::Light => ("🌙", "Switch to dark theme"),
                        };
                        if widgets::tool_button(ui, g, tip).clicked() {
                            self.prefs.theme = if self.prefs.theme == ThemeMode::Dark { ThemeMode::Light } else { ThemeMode::Dark };
                            self.prefs.save();
                        }
                    });
                });
            });
    }

    fn new_tab(&mut self) {
        if let Some(i) = self.tabs.iter().position(|t| matches!(t, Tab::Welcome)) {
            self.active = i;
        } else {
            self.tabs.push(Tab::Welcome);
            self.active = self.tabs.len() - 1;
        }
    }

    fn welcome(&mut self, ctx: &egui::Context, pal: &Palette) {
        let mut open: Option<PathBuf> = None;
        let mut remove: Option<usize> = None;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.window)).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(RichText::new("SourceGit-rs").size(26.0).strong());
                ui.label(RichText::new("Open-source Git GUI client — Rust edition").color(pal.fg2));
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 520.0).max(0.0) / 2.0);
                    if ui.button("⬇  Clone repository").clicked() {
                        self.popup = Some(AppPopup::Clone { url: String::new(), parent: self.prefs.default_clone_dir.clone(), name: String::new(), extra: String::new(), recursive: true });
                    }
                    if ui.button("📂  Open local repository").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            open = Some(p);
                        }
                    }
                    if ui.button("✨  Init new repository").clicked() {
                        self.popup = Some(AppPopup::Init { path: String::new() });
                    }
                });
                ui.add_space(16.0);
                ui.add(egui::TextEdit::singleline(&mut self.welcome_filter).hint_text("🔍 Search repositories…").desired_width(520.0));
                ui.add_space(8.0);
            });
            let f = self.welcome_filter.to_lowercase();
            let width = 640.0f32.min(ui.available_width() - 32.0);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - width) / 2.0);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        if self.prefs.repositories.is_empty() {
                            ui.add_space(20.0);
                            ui.label(RichText::new("No repositories yet. Clone or open one, or drop a folder here.").color(pal.fg2));
                        }
                        for (i, r) in self.prefs.repositories.iter().enumerate() {
                            if !f.is_empty() && !r.name.to_lowercase().contains(&f) && !r.path.to_lowercase().contains(&f) {
                                continue;
                            }
                            let exists = std::path::Path::new(&r.path).exists();
                            let resp = widgets::row(ui, false, 44.0, pal, |ui| {
                                ui.label(RichText::new("🔖").size(18.0).color(if exists { theme::GRAPH_COLORS[1] } else { pal.fg2 }));
                                ui.vertical(|ui| {
                                    ui.add_space(4.0);
                                    ui.label(RichText::new(&r.name).strong());
                                    ui.label(RichText::new(&r.path).size(11.5).color(pal.fg2));
                                });
                            });
                            if resp.double_clicked() || resp.clicked() {
                                open = Some(PathBuf::from(&r.path));
                            }
                            resp.context_menu(|ui| {
                                if ui.button("Open").clicked() {
                                    open = Some(PathBuf::from(&r.path));
                                    ui.close_menu();
                                }
                                if ui.button("Open in file manager").clicked() {
                                    widgets::open_path(std::path::Path::new(&r.path));
                                    ui.close_menu();
                                }
                                if ui.button("Open in terminal").clicked() {
                                    widgets::open_terminal(std::path::Path::new(&r.path));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button("Remove from list").clicked() {
                                    remove = Some(i);
                                    ui.close_menu();
                                }
                            });
                        }
                    });
                });
            });
        });
        // Drag & drop folders
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for p in dropped {
            if p.is_dir() {
                open = Some(p);
            }
        }
        if let Some(i) = remove {
            self.prefs.repositories.remove(i);
            self.prefs.save();
        }
        if let Some(p) = open {
            self.open_repo(p);
        }
    }

    fn app_popup(&mut self, ctx: &egui::Context) {
        let Some(p) = &mut self.popup else { return };
        let mut close = false;
        let mut start: Option<(String, Box<dyn FnOnce() -> Result<PathBuf, String> + Send>)> = None;
        let modal = egui::Modal::new(egui::Id::new("app-popup")).show(ctx, |ui| {
            ui.set_min_width(520.0);
            match p {
                AppPopup::Clone { url, parent, name, extra, recursive } => {
                    ui.label(RichText::new("Clone Remote Repository").size(16.0).strong());
                    ui.separator();
                    egui::Grid::new("clone-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("Repository URL:");
                        ui.add(egui::TextEdit::singleline(url).hint_text("https://… or git@…").desired_width(360.0));
                        ui.end_row();
                        ui.label("Parent folder:");
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(parent).desired_width(300.0));
                            if ui.button("…").clicked() {
                                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                    *parent = d.display().to_string();
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("Local name:");
                        ui.add(egui::TextEdit::singleline(name).hint_text("Optional (defaults to repository name)").desired_width(360.0));
                        ui.end_row();
                        ui.label("Extra arguments:");
                        ui.add(egui::TextEdit::singleline(extra).hint_text("e.g. --depth 1 --branch dev").desired_width(360.0));
                        ui.end_row();
                        ui.label("");
                        ui.checkbox(recursive, "Initialize & update submodules");
                        ui.end_row();
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                        if ui.add_enabled(!url.trim().is_empty() && !parent.trim().is_empty(), egui::Button::new("Clone")).clicked() {
                            let (u, pa, n, e, r) = (url.trim().to_string(), git::expand_home(parent.trim()), name.trim().to_string(), extra.clone(), *recursive);
                            start = Some((
                                format!("Clone {u}"),
                                Box::new(move || {
                                    let _ = std::fs::create_dir_all(&pa);
                                    ops::clone(&u, &pa, &n, &e, r).into_result()?;
                                    let dir = if n.is_empty() {
                                        u.trim_end_matches('/').trim_end_matches(".git").rsplit(['/', ':']).next().unwrap_or("repo").to_string()
                                    } else {
                                        n
                                    };
                                    Ok(pa.join(dir))
                                }),
                            ));
                            close = true;
                        }
                    });
                }
                AppPopup::Init { path } => {
                    ui.label(RichText::new("Initialize Repository").size(16.0).strong());
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Folder:");
                        ui.add(egui::TextEdit::singleline(path).desired_width(320.0));
                        if ui.button("…").clicked() {
                            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                *path = d.display().to_string();
                            }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                        if ui.add_enabled(!path.trim().is_empty(), egui::Button::new("Init")).clicked() {
                            let pa = git::expand_home(path.trim());
                            start = Some((
                                "Init".into(),
                                Box::new(move || {
                                    std::fs::create_dir_all(&pa).map_err(|e| e.to_string())?;
                                    ops::init(&pa).into_result()?;
                                    Ok(pa)
                                }),
                            ));
                            close = true;
                        }
                    });
                }
            }
        });
        if modal.should_close() {
            close = true;
        }
        if close {
            self.popup = None;
        }
        if let Some((label, f)) = start {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let _ = tx.send(f());
            });
            self.task = Some((label, rx));
        }
    }

    fn poll_task(&mut self, ctx: &egui::Context) {
        let Some((_, rx)) = &self.task else { return };
        match rx.try_recv() {
            Ok(Ok(p)) => {
                self.task = None;
                self.open_repo(p);
            }
            Ok(Err(e)) => {
                self.task = None;
                self.errors.push(e);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            Err(_) => self.task = None,
        }
    }

    fn prefs_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_prefs;
        egui::Window::new("Preferences").open(&mut open).resizable(false).default_width(480.0).show(ctx, |ui| {
            egui::Grid::new("prefs-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label(RichText::new("APPEARANCE").strong());
                ui.end_row();
                ui.label("Theme:");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.prefs.theme, ThemeMode::Dark, "Dark");
                    ui.radio_value(&mut self.prefs.theme, ThemeMode::Light, "Light");
                });
                ui.end_row();
                ui.label("Font size:");
                ui.add(egui::Slider::new(&mut self.prefs.font_size, 10.0..=18.0));
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut self.prefs.relative_time, "Show relative time in history");
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut self.prefs.show_tags_in_graph, "Show tags in commit graph");
                ui.end_row();

                ui.label(RichText::new("GENERAL").strong());
                ui.end_row();
                ui.label("Max history commits:");
                ui.add(egui::DragValue::new(&mut self.prefs.max_history_commits).range(100..=100000).speed(100));
                ui.end_row();
                ui.label("Default clone dir:");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.prefs.default_clone_dir).desired_width(260.0));
                    if ui.button("…").clicked() {
                        if let Some(d) = rfd::FileDialog::new().pick_folder() {
                            self.prefs.default_clone_dir = d.display().to_string();
                        }
                    }
                });
                ui.end_row();

                ui.label(RichText::new("GIT").strong());
                ui.end_row();
                ui.label("Git version:");
                ui.label(RichText::new(&self.git_version).monospace());
                ui.end_row();
                ui.label("User name (global):");
                let n = ui.add(egui::TextEdit::singleline(&mut self.global_name).desired_width(260.0));
                ui.end_row();
                ui.label("User email (global):");
                let e = ui.add(egui::TextEdit::singleline(&mut self.global_email).desired_width(260.0));
                ui.end_row();
                if n.lost_focus() {
                    ops::set_config(None, "user.name", self.global_name.trim(), true);
                }
                if e.lost_focus() {
                    ops::set_config(None, "user.email", self.global_email.trim(), true);
                }
                ui.label("");
                ui.checkbox(&mut self.prefs.fetch_prune, "Prune on fetch by default");
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut self.prefs.pull_rebase, "Use rebase on pull by default");
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut self.prefs.auto_stash, "Auto-stash local changes on pull/rebase");
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut self.prefs.show_untracked, "Show untracked files");
                ui.end_row();
            });
            ui.separator();
            ui.label(RichText::new(format!("Settings file: {}", crate::prefs::config_dir().join("preference.json").display())).size(11.0));
        });
        if self.show_prefs && !open {
            self.prefs.save();
        }
        self.show_prefs = open;
    }

    fn log_window(&mut self, ctx: &egui::Context, pal: &Palette) {
        let mut open = self.show_log;
        egui::Window::new("Git Command Log").open(&mut open).default_size([820.0, 480.0]).resizable(true).show(ctx, |ui| {
            let log = git::command_log();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} command(s)", log.len())).color(pal.fg2));
                if ui.small_button("Clear").clicked() {
                    git::clear_command_log();
                    self.log_selected = None;
                }
            });
            ui.separator();
            egui::SidePanel::left("log-list").default_width(420.0).resizable(true).show_inside(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (i, e) in log.iter().enumerate().rev() {
                        let r = widgets::row(ui, self.log_selected == Some(i), 22.0, pal, |ui| {
                            ui.label(RichText::new(if e.ok { "✔" } else { "✖" }).color(if e.ok { theme::GRAPH_COLORS[1] } else { egui::Color32::LIGHT_RED }));
                            ui.label(RichText::new(e.time.format("%H:%M:%S").to_string()).monospace().color(pal.fg2));
                            ui.label(RichText::new(&e.args).monospace());
                        });
                        if r.clicked() {
                            self.log_selected = Some(i);
                        }
                    }
                });
            });
            egui::CentralPanel::default().show_inside(ui, |ui| {
                if let Some(e) = self.log_selected.and_then(|i| log.get(i)) {
                    ui.label(RichText::new(&e.args).monospace().strong());
                    ui.label(RichText::new(format!("{} — {} ms", e.repo, e.millis)).color(pal.fg2));
                    ui.separator();
                    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(&e.output).monospace()).selectable(true));
                    });
                }
            });
        });
        self.show_log = open;
    }

    fn errors_ui(&mut self, ctx: &egui::Context, pal: &Palette) {
        if self.errors.is_empty() && self.task.is_none() {
            return;
        }
        let mut remove = None;
        egui::Area::new(egui::Id::new("app-errors")).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -16.0)).show(ctx, |ui| {
            if let Some((label, _)) = &self.task {
                egui::Frame::new().fill(pal.popup).stroke(egui::Stroke::new(1.0_f32, pal.border1)).corner_radius(4).inner_margin(10).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(label);
                    });
                });
            }
            for (i, e) in self.errors.iter().enumerate() {
                egui::Frame::new().fill(egui::Color32::from_rgb(0x8B, 0x2A, 0x2A)).corner_radius(4).inner_margin(10).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(e).color(egui::Color32::WHITE));
                        if ui.small_button("✖").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
        });
        if let Some(i) = remove {
            self.errors.remove(i);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let (new_tab, close, next, refresh, p1, p2, p3, find) = ctx.input(|i| {
            let c = i.modifiers.command;
            (
                c && i.key_pressed(egui::Key::T),
                c && i.key_pressed(egui::Key::W),
                c && i.key_pressed(egui::Key::Tab),
                i.key_pressed(egui::Key::F5),
                c && i.key_pressed(egui::Key::Num1),
                c && i.key_pressed(egui::Key::Num2),
                c && i.key_pressed(egui::Key::Num3),
                c && i.key_pressed(egui::Key::F),
            )
        });
        if new_tab {
            self.new_tab();
        }
        if close {
            self.close_tab(self.active);
        }
        if next && !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
        if let Some(Tab::Repo(r)) = self.tabs.get_mut(self.active) {
            if refresh {
                r.refresh(&self.prefs);
            }
            if p1 {
                r.page = Page::Histories;
            }
            if p2 {
                r.page = Page::WorkingCopy;
            }
            if p3 {
                r.page = Page::Stashes;
            }
            if find {
                r.page = Page::Histories;
                r.search.open = true;
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame += 1;
        if self.applied_theme != Some((self.prefs.theme, self.prefs.font_size)) {
            theme::apply(ctx, self.prefs.theme);
            let size = self.prefs.font_size;
            ctx.style_mut(|s| {
                for (style, font) in s.text_styles.iter_mut() {
                    font.size = match style {
                        egui::TextStyle::Heading => size + 5.0,
                        egui::TextStyle::Small => size - 3.0,
                        _ => size,
                    };
                }
            });
            self.applied_theme = Some((self.prefs.theme, self.prefs.font_size));
        }
        let pal = theme::palette(self.prefs.theme);
        self.poll_task(ctx);
        self.shortcuts(ctx);
        self.tab_bar(ctx, &pal);

        let mut requests = Vec::new();
        match self.tabs.get_mut(self.active) {
            Some(Tab::Repo(r)) => requests = r.ui(ctx, &pal, &mut self.prefs),
            _ => self.welcome(ctx, &pal),
        }
        for r in requests {
            match r {
                AppRequest::OpenRepo(p) => {
                    let prev = self.active;
                    self.tabs.push(Tab::Welcome);
                    self.active = self.tabs.len() - 1;
                    self.open_repo(p);
                    if matches!(self.tabs.get(self.active), Some(Tab::Welcome)) {
                        self.tabs.pop();
                        self.active = prev;
                    }
                }
            }
        }
        self.app_popup(ctx);
        self.prefs_window(ctx);
        self.log_window(ctx, &pal);
        self.errors_ui(ctx, &pal);

        // --screenshot support
        if let Some(s) = &mut self.screenshot {
            if let Some(Tab::Repo(r)) = self.tabs.get_mut(self.active) {
                if let Some(pg) = s.page.take() {
                    r.page = pg;
                }
            }
            if !s.requested && self.frame as u32 >= s.frames {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                s.requested = true;
            }
            let shot = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None })
            });
            if let Some(img) = shot {
                let path = s.path.clone();
                if let Err(e) = crate::save_png(&path, &img) {
                    eprintln!("screenshot failed: {e}");
                    std::process::exit(1);
                }
                eprintln!("screenshot saved to {}", path.display());
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.screenshot.is_none() {
            self.save_session();
        }
    }
}
