//! Thin wrapper around the `git` CLI plus parsers for its output.
//! Command lines mirror the ones SourceGit uses (see capture/README.md).

pub mod models;
pub mod parse;
pub mod ops;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;

pub use models::*;

#[derive(Clone, Debug)]
pub struct Output {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn into_result(self) -> Result<String, String> {
        if self.ok {
            Ok(self.stdout)
        } else {
            let msg = if self.stderr.trim().is_empty() { self.stdout } else { self.stderr };
            Err(msg.trim().to_string())
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub repo: String,
    pub args: String,
    pub ok: bool,
    pub millis: u128,
    pub output: String,
    pub time: chrono::DateTime<chrono::Local>,
}

static COMMAND_LOG: Mutex<Vec<LogEntry>> = Mutex::new(Vec::new());
const MAX_LOG: usize = 500;

pub fn command_log() -> Vec<LogEntry> {
    COMMAND_LOG.lock().map(|l| l.clone()).unwrap_or_default()
}

pub fn clear_command_log() {
    if let Ok(mut l) = COMMAND_LOG.lock() {
        l.clear();
    }
}

/// Read-only query (not recorded in the command log).
pub fn run_in(repo: Option<&Path>, args: &[&str]) -> Output {
    spawn(repo, args, None, false)
}

/// Read-only query in `repo`.
pub fn run(repo: &Path, args: &[&str]) -> Output {
    spawn(Some(repo), args, None, false)
}

/// A command that changes something; recorded in the command log.
pub fn exec(repo: &Path, args: &[&str]) -> Output {
    spawn(Some(repo), args, None, true)
}

pub fn exec_in(repo: Option<&Path>, args: &[&str]) -> Output {
    spawn(repo, args, None, true)
}

pub fn exec_with_input(repo: &Path, args: &[&str], stdin: &[u8]) -> Output {
    spawn(Some(repo), args, Some(stdin), true)
}

fn spawn(repo: Option<&Path>, args: &[&str], stdin: Option<&[u8]>, record: bool) -> Output {
    let start = Instant::now();
    let mut cmd = Command::new("git");
    no_window(&mut cmd);
    cmd.arg("--no-pager").args(["-c", "core.quotepath=off", "-c", "color.ui=false"]);
    cmd.args(args);
    if let Some(r) = repo {
        cmd.current_dir(r);
    }
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("LC_ALL", "C.UTF-8");
    cmd.env("GIT_EDITOR", "true");
    cmd.env("GIT_SEQUENCE_EDITOR", "true");
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });

    let result = match cmd.spawn() {
        Ok(mut child) => {
            if let (Some(data), Some(mut si)) = (stdin, child.stdin.take()) {
                use std::io::Write;
                let _ = si.write_all(data);
            }
            match child.wait_with_output() {
                Ok(o) => Output {
                    ok: o.status.success(),
                    stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                },
                Err(e) => Output { ok: false, stdout: String::new(), stderr: e.to_string() },
            }
        }
        Err(e) => Output { ok: false, stdout: String::new(), stderr: format!("failed to start git: {e}") },
    };

    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    if record {
        if let Ok(mut log) = COMMAND_LOG.lock() {
            let mut output = result.stdout.clone();
            output.push_str(&result.stderr);
            if output.len() > 8000 {
                output.truncate(8000);
                output.push_str("\n…");
            }
            log.push(LogEntry {
                repo: repo.map(|p| p.display().to_string()).unwrap_or_default(),
                args: format!("git {}", owned.iter().map(|a| shell_quote(a)).collect::<Vec<_>>().join(" ")),
                ok: result.ok,
                millis: start.elapsed().as_millis(),
                output,
                time: chrono::Local::now(),
            });
            if log.len() > MAX_LOG {
                let n = log.len() - MAX_LOG;
                log.drain(0..n);
            }
        }
    }
    result
}

fn shell_quote(s: &str) -> String {
    if s.is_empty() || s.chars().any(|c| c.is_whitespace() || c == '"' || c == '\'') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// On Windows, keep git from flashing a console window for every command.
pub fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn version() -> Option<String> {
    let o = run_in(None, &["--version"]);
    o.ok.then(|| o.stdout.trim().to_string())
}

/// Returns the work-tree root of `path`, if it is inside a git repository.
pub fn repo_root(path: &Path) -> Option<PathBuf> {
    let o = run(path, &["rev-parse", "--show-toplevel"]);
    if o.ok {
        let p = o.stdout.trim();
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    // Bare repository?
    let o = run(path, &["rev-parse", "--is-bare-repository"]);
    if o.ok && o.stdout.trim() == "true" {
        return Some(path.to_path_buf());
    }
    None
}

pub fn git_dir(repo: &Path) -> PathBuf {
    let o = run(repo, &["rev-parse", "--absolute-git-dir"]);
    if o.ok {
        PathBuf::from(o.stdout.trim())
    } else {
        repo.join(".git")
    }
}
