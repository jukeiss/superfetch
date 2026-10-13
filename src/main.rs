//! superfetch: a fast, configurable system information tool.
//!
//! How a run flows through the code:
//!
//! 1. `cli` parses the arguments.
//! 2. `config` loads the TOML file and the flags are applied on top.
//! 3. `logo` picks the art; `theme` resolves colors against it.
//! 4. `modules` gathers the data, in parallel where that pays off, calling
//!    into `sys` for anything platform-specific.
//! 5. `render` lays the logo and the lines out and the result is written
//!    in a single call.

mod cli;
mod config;
mod format;
mod logo;
mod modules;
mod render;
mod style;
mod sys;
mod template;
mod term;
mod text;
mod theme;
mod toml;
mod tui;

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use std::{env, fmt::Write as _};

use cli::{Action, Args};
use logo::Logo;
use theme::Theme;

static QUIET: AtomicBool = AtomicBool::new(false);

/// Reports a problem that does not stop the run, such as a misspelt color.
pub fn warn(message: &str) {
    if !QUIET.load(Ordering::Relaxed) {
        eprintln!("superfetch: {message}");
    }
}

/// Silences `warn`, for when the screen belongs to the interactive editor.
pub fn set_quiet(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("superfetch: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let started = Instant::now();
    let args = cli::parse(env::args().skip(1))?;
    match args.action {
        Action::Fetch => fetch(&args, started),
        Action::Configure => {
            let path = config_path(&args)?;
            let config = config::load_or_default(&path)?;
            tui::run(&path, config)
        }
        Action::Help => print(cli::HELP),
        Action::Version => print(&format!("superfetch {}\n", env!("CARGO_PKG_VERSION"))),
        Action::DefaultConfig => print(&config::default_file()),
        Action::List => print(&listing()),
    }
}

fn fetch(args: &Args, started: Instant) -> Result<(), String> {
    let mut config = config::load(args.config_file.as_deref(), &args.overrides)?;
    args.apply(&mut config)?;
    if args.save {
        let path = config_path(args)?;
        let before = config::load_or_default(&path)?;
        config::save(&path, &before, &config, &args.overrides)?;
    }
    let configured = started.elapsed();

    let stdout = io::stdout();
    let color = args.force_color
        || (!args.no_color
            && env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
            && stdout.is_terminal());
    if color {
        term::enable_escape_sequences();
    }

    let logo = Logo::load(&config.logo);
    let theme = Theme::new(&config, logo.as_ref(), color);
    let (lines, timings) = modules::collect(&config, &theme);
    let fetched = started.elapsed();

    let width = term::size().map(|(columns, _)| columns);
    let output = render::render(&lines, logo.as_ref(), &config, &theme, width);
    print(&output)?;

    if args.timings {
        let total = started.elapsed();
        let mut report = String::from("\n");
        for timing in &timings {
            let note = if timing.threaded {
                "  (own thread)"
            } else {
                ""
            };
            let _ = writeln!(report, "{:>10.1?}  {}{note}", timing.elapsed, timing.name);
        }
        let _ = writeln!(report, "{:>10.1?}  arguments and config", configured);
        let _ = writeln!(
            report,
            "{:>10.1?}  all modules, wall clock",
            fetched - configured
        );
        let _ = writeln!(report, "{:>10.1?}  layout and output", total - fetched);
        let _ = writeln!(report, "{total:>10.1?}  total");
        eprint!("{report}");
    }
    Ok(())
}

/// The config file this run reads and writes.
fn config_path(args: &Args) -> Result<PathBuf, String> {
    args.config_file
        .clone()
        .or_else(config::default_path)
        .ok_or_else(|| "cannot locate a config directory (is HOME set?)".to_string())
}

/// Writes to standard output in one call. A closed pipe (`superfetch |
/// head -1`) is not an error.
fn print(text: &str) -> Result<(), String> {
    match io::stdout().lock().write_all(text.as_bytes()) {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => {
            Err(format!("cannot write output: {err}"))
        }
        _ => Ok(()),
    }
}

fn listing() -> String {
    let mut out = String::from("Modules (* shown by default):\n");
    for module in modules::MODULES {
        let mark = if modules::DEFAULT_LAYOUT.contains(&module.name) {
            '*'
        } else {
            ' '
        };
        let _ = writeln!(out, "  {mark} {:<10} {}", module.name, module.about);
    }
    out.push_str("\nLogos:\n  auto, none");
    for name in logo::names() {
        let _ = write!(out, ", {name}");
    }
    out.push_str("\n  or the path of a text file\n");
    out
}
