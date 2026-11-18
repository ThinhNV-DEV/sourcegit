//! SourceGit-rs — a Rust/egui rebuild of the SourceGit Git GUI client.
//!
//! Usage:
//!   sourcegit-rs [REPO_PATH...]
//!   sourcegit-rs --screenshot out.png [--page histories|changes|stashes] [--theme dark|light] [REPO_PATH]
//!   sourcegit-rs --self-test out.json

mod consts;
mod git;
mod graph;
mod prefs;
mod repo;
mod selftest;
mod theme;
mod ui;

use std::path::{Path, PathBuf};

pub fn save_png(path: &Path, img: &eframe::egui::ColorImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let w = std::io::BufWriter::new(file);
    let mut enc = png::Encoder::new(w, img.size[0] as u32, img.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    let data: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
    writer.write_image_data(&data).map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut repos: Vec<PathBuf> = Vec::new();
    let mut screenshot: Option<PathBuf> = None;
    let mut page = None;
    let mut theme_override = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--self-test" => {
                let out = args.get(i + 1).cloned().unwrap_or_else(|| "self-test.json".into());
                let ok = selftest::run(Path::new(&out));
                std::process::exit(if ok { 0 } else { 1 });
            }
            "--screenshot" => {
                screenshot = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--page" => {
                page = match args.get(i + 1).map(String::as_str) {
                    Some("changes") => Some(ui::repo_view::Page::WorkingCopy),
                    Some("stashes") => Some(ui::repo_view::Page::Stashes),
                    _ => Some(ui::repo_view::Page::Histories),
                };
                i += 1;
            }
            "--theme" => {
                theme_override = match args.get(i + 1).map(String::as_str) {
                    Some("light") => Some(theme::ThemeMode::Light),
                    _ => Some(theme::ThemeMode::Dark),
                };
                i += 1;
            }
            "-h" | "--help" => {
                println!("sourcegit-rs [REPO_PATH...] | --screenshot out.png [--page histories|changes|stashes] [--theme dark|light] [REPO] | --self-test out.json");
                return;
            }
            a => repos.push(PathBuf::from(a)),
        }
        i += 1;
    }

    if git::version().is_none() {
        eprintln!("git executable not found in PATH. Please install Git >= 2.25.1.");
    }

    let mut prefs = if screenshot.is_some() {
        // Do not read or write the user's session in screenshot mode.
        prefs::Preferences { open_tabs: vec![], ..Default::default() }
    } else {
        prefs::Preferences::load()
    };
    if let Some(t) = theme_override {
        prefs.theme = t;
    }

    let mut app = ui::app::App::new(prefs, repos);
    if let Some(p) = screenshot {
        app.screenshot = Some(ui::app::ScreenshotCfg { path: p, frames: 120, requested: false, page });
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_title("SourceGit-rs").with_inner_size([1350.0, 800.0]).with_min_inner_size([900.0, 560.0]).with_drag_and_drop(true),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native("SourceGit-rs", options, Box::new(|_cc| Ok(Box::new(app)))) {
        eprintln!("failed to start UI: {e}");
        std::process::exit(1);
    }
}
