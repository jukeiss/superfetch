//! The interactive editor behind `superfetch --config`.
//!
//! A single screen: a list of modules to switch on, off and reorder, a few
//! appearance settings, and a live preview of the result. It is drawn with
//! plain escape sequences; there is not enough here to justify a TUI
//! library.

use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;

use crate::config::{self, Align, Config, Position};
use crate::logo::{self, Logo};
use crate::modules::{self, MODULES};
use crate::style::{Color, RESET, Style};
use crate::theme::Theme;
use crate::{render, term, text};

const ENTER_SCREEN: &str = "\x1b[?1049h\x1b[?25l";
const LEAVE_SCREEN: &str = "\x1b[?25h\x1b[?1049l";
const HELP: &str = "↑↓ move   space toggle   ←→ change   J/K reorder   s save   q quit";

/// Bar styles to cycle through, as (filled, empty).
const BAR_STYLES: [(&str, &str); 5] = [("━", "─"), ("█", "░"), ("■", "□"), ("#", "-"), ("=", " ")];
const KEY_SEPARATORS: [&str; 5] = [": ", " ", " - ", " → ", " │ "];

#[derive(Clone, Copy, PartialEq)]
enum Setting {
    Accent,
    Logo,
    LogoPosition,
    KeyAlign,
    KeySeparator,
    BarStyle,
    BarWidth,
    Units,
}

const SETTINGS: [(Setting, &str); 8] = [
    (Setting::Accent, "Accent color"),
    (Setting::Logo, "Logo"),
    (Setting::LogoPosition, "Logo position"),
    (Setting::KeyAlign, "Align labels"),
    (Setting::KeySeparator, "Label separator"),
    (Setting::BarStyle, "Bar style"),
    (Setting::BarWidth, "Bar width"),
    (Setting::Units, "Size units"),
];

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Heading(&'static str),
    Module(usize),
    Setting(usize),
}

enum Key {
    Up,
    Down,
    Left,
    Right,
    MoveUp,
    MoveDown,
    Toggle,
    Save,
    Quit,
    Other,
}

struct Editor {
    config: Config,
    /// The state on disk, to tell what a save has to write.
    saved: Config,
    /// Every module that can be shown, in display order, and whether it is on.
    modules: Vec<(String, bool)>,
    rows: Vec<Row>,
    cursor: usize,
    status: String,
    quit_armed: bool,
}

/// Restores the terminal however the editor ends, including by panic.
struct Screen {
    _raw: term::RawMode,
}

impl Screen {
    fn enter() -> io::Result<Screen> {
        let raw = term::RawMode::enter()?;
        term::enable_escape_sequences();
        write_all(ENTER_SCREEN)?;
        Ok(Screen { _raw: raw })
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = write_all(LEAVE_SCREEN);
    }
}

fn write_all(text: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(text.as_bytes())?;
    stdout.flush()
}

pub fn run(path: &Path, config: Config) -> Result<(), String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("--config needs an interactive terminal".into());
    }
    let mut editor = Editor::new(config);
    let mut saved_once = false;

    // Warnings would land in the middle of the screen.
    crate::set_quiet(true);
    let result = (|| -> io::Result<()> {
        let _screen = Screen::enter()?;
        let mut input = io::stdin().lock();
        let mut buffer = [0u8; 32];
        loop {
            write_all(&editor.frame(path))?;
            let read = input.read(&mut buffer)?;
            if read == 0 {
                return Ok(());
            }
            match decode(&buffer[..read]) {
                Key::Quit if editor.is_dirty() && !editor.quit_armed => {
                    editor.quit_armed = true;
                    editor.status =
                        "Unsaved changes. Press s to save, or q again to discard.".into();
                    continue;
                }
                Key::Quit => return Ok(()),
                Key::Save => match config::save(path, &editor.saved, &editor.config, &[]) {
                    Ok(()) => {
                        editor.saved = editor.config.clone();
                        editor.status = format!("Saved to {}", shorten(path));
                        saved_once = true;
                    }
                    Err(err) => editor.status = err,
                },
                key => {
                    editor.status.clear();
                    editor.handle(key);
                }
            }
            editor.quit_armed = false;
        }
    })();
    crate::set_quiet(false);

    result.map_err(|err| format!("terminal error: {err}"))?;
    if saved_once {
        println!("Saved to {}", shorten(path));
    }
    Ok(())
}

/// Maps the bytes of one key press to an action.
fn decode(bytes: &[u8]) -> Key {
    match bytes {
        b"\x1b[A" | b"\x1bOA" | b"k" => Key::Up,
        b"\x1b[B" | b"\x1bOB" | b"j" => Key::Down,
        b"\x1b[C" | b"\x1bOC" | b"l" => Key::Right,
        b"\x1b[D" | b"\x1bOD" | b"h" => Key::Left,
        b"\x1b[1;2A" | b"K" => Key::MoveUp,
        b"\x1b[1;2B" | b"J" => Key::MoveDown,
        b" " | b"\r" | b"\n" => Key::Toggle,
        b"s" | b"S" => Key::Save,
        // Escape on its own, or Ctrl-C, which raw mode delivers as a byte.
        b"q" | b"Q" | b"\x1b" | b"\x03" => Key::Quit,
        _ => Key::Other,
    }
}

impl Editor {
    fn new(config: Config) -> Editor {
        // Enabled modules in their configured order, then everything else
        // that could be enabled: the remaining built-ins and custom modules.
        let mut modules: Vec<(String, bool)> = config
            .modules
            .iter()
            .map(|name| (name.clone(), true))
            .collect();
        let builtin = MODULES.iter().map(|module| module.name.to_string());
        let custom = config
            .module
            .keys()
            .filter(|name| modules::find(name).is_none())
            .cloned();
        for name in builtin.chain(custom) {
            if !config.modules.contains(&name) {
                modules.push((name, false));
            }
        }

        let mut rows = vec![Row::Heading("Modules")];
        rows.extend((0..modules.len()).map(Row::Module));
        rows.push(Row::Heading("Appearance"));
        rows.extend((0..SETTINGS.len()).map(Row::Setting));

        Editor {
            saved: config.clone(),
            config,
            modules,
            rows,
            cursor: 1,
            status: String::new(),
            quit_armed: false,
        }
    }

    fn is_dirty(&self) -> bool {
        self.config != self.saved
    }

    fn handle(&mut self, key: Key) {
        match (key, self.rows[self.cursor]) {
            (Key::Up, _) => self.step(-1),
            (Key::Down, _) => self.step(1),
            (Key::Toggle, Row::Module(index)) => {
                self.modules[index].1 ^= true;
                self.sync_modules();
            }
            (Key::MoveUp, Row::Module(index)) if index > 0 => self.swap(index, index - 1),
            (Key::MoveDown, Row::Module(index)) if index + 1 < self.modules.len() => {
                self.swap(index, index + 1)
            }
            (Key::Left, Row::Setting(index)) => self.change(SETTINGS[index].0, -1),
            (Key::Right | Key::Toggle, Row::Setting(index)) => self.change(SETTINGS[index].0, 1),
            _ => {}
        }
    }

    /// Moves the cursor, skipping headings.
    fn step(&mut self, direction: isize) {
        let mut next = self.cursor as isize + direction;
        while let Some(row) = usize::try_from(next).ok().and_then(|i| self.rows.get(i)) {
            if !matches!(row, Row::Heading(_)) {
                self.cursor = next as usize;
                return;
            }
            next += direction;
        }
    }

    fn swap(&mut self, from: usize, to: usize) {
        self.modules.swap(from, to);
        // Module rows directly follow the first heading.
        self.cursor = to + 1;
        self.sync_modules();
    }

    fn sync_modules(&mut self) {
        self.config.modules = self
            .modules
            .iter()
            .filter(|(_, enabled)| *enabled)
            .map(|(name, _)| name.clone())
            .collect();
    }

    /// The values a setting cycles through. The current value is included
    /// even when it is something only the config file could have set.
    fn choices(&self, setting: Setting) -> Vec<String> {
        let with_current = |mut choices: Vec<String>, current: &str| {
            if !choices.iter().any(|choice| choice == current) {
                choices.push(current.to_string());
            }
            choices
        };
        match setting {
            Setting::Accent => with_current(
                std::iter::once("auto".to_string())
                    .chain(Color::names())
                    .collect(),
                &self.config.theme.accent,
            ),
            Setting::Logo => with_current(
                ["auto", "none"]
                    .into_iter()
                    .chain(logo::names())
                    .map(String::from)
                    .collect(),
                &self.config.logo.source,
            ),
            Setting::LogoPosition => Position::ALL.iter().map(|p| p.name().to_string()).collect(),
            Setting::KeyAlign => Align::ALL.iter().map(|a| a.name().to_string()).collect(),
            Setting::KeySeparator => with_current(
                KEY_SEPARATORS.iter().map(|s| s.to_string()).collect(),
                &self.config.layout.key_separator,
            ),
            Setting::BarStyle => with_current(
                BAR_STYLES
                    .iter()
                    .map(|(filled, empty)| bar_sample(filled, empty))
                    .collect(),
                &self.value(Setting::BarStyle),
            ),
            Setting::BarWidth => (4..=40).map(|width| width.to_string()).collect(),
            Setting::Units => vec!["GiB".into(), "GB".into()],
        }
    }

    fn value(&self, setting: Setting) -> String {
        let config = &self.config;
        match setting {
            Setting::Accent => config.theme.accent.clone(),
            Setting::Logo => config.logo.source.clone(),
            Setting::LogoPosition => config.logo.position.name().into(),
            Setting::KeyAlign => config.layout.key_align.name().into(),
            Setting::KeySeparator => config.layout.key_separator.clone(),
            Setting::BarStyle => bar_sample(&config.bar.filled, &config.bar.empty),
            Setting::BarWidth => config.bar.width.clamp(4, 40).to_string(),
            Setting::Units => if config.units.binary { "GiB" } else { "GB" }.into(),
        }
    }

    fn change(&mut self, setting: Setting, direction: isize) {
        let choices = self.choices(setting);
        let current = self.value(setting);
        let index = choices
            .iter()
            .position(|choice| *choice == current)
            .unwrap_or(0);
        let next = (index as isize + direction).rem_euclid(choices.len() as isize) as usize;
        let choice = &choices[next];

        let config = &mut self.config;
        match setting {
            Setting::Accent => config.theme.accent = choice.clone(),
            Setting::Logo => config.logo.source = choice.clone(),
            Setting::LogoPosition => config.logo.position = Position::ALL[next],
            Setting::KeyAlign => config.layout.key_align = Align::ALL[next],
            Setting::KeySeparator => config.layout.key_separator = choice.clone(),
            Setting::BarStyle => {
                // A style from the config file that is not in the list sits
                // at the end of the choices and is already what is set.
                if let Some((filled, empty)) = BAR_STYLES.get(next) {
                    config.bar.filled = filled.to_string();
                    config.bar.empty = empty.to_string();
                }
            }
            Setting::BarWidth => config.bar.width = choice.parse().unwrap_or(10),
            Setting::Units => config.units.binary = choice == "GiB",
        }
    }

    /// The whole screen, ready to be written in one go.
    fn frame(&self, path: &Path) -> String {
        let (columns, rows) = term::size().unwrap_or((80, 24));
        let bold = Style {
            bold: true,
            ..Style::default()
        };
        let dim = Style {
            dim: true,
            ..Style::default()
        };

        let menu: Vec<String> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| self.row(i, *row))
            .collect();
        let preview = self.preview(columns);

        // Fixed lines: title, blank, blank, status/help.
        const CHROME: usize = 4;
        let available = rows.saturating_sub(CHROME);
        // The preview is shown when it fits below a usable slice of the menu.
        let show_preview = available >= preview.len() + 1 + 6.min(menu.len());
        let menu_height = if show_preview {
            (available - preview.len() - 1).min(menu.len())
        } else {
            available.min(menu.len())
        };
        // Scroll so the cursor stays inside the window.
        let first = (self.cursor + 1)
            .saturating_sub(menu_height)
            .min(menu.len() - menu_height);

        let mut lines = vec![
            format!(
                " {}  {}",
                bold.paint("superfetch"),
                dim.paint(&shorten(path))
            ),
            String::new(),
        ];
        lines.extend(menu.into_iter().skip(first).take(menu_height));
        lines.push(String::new());
        lines.push(if self.status.is_empty() {
            format!(" {}", dim.paint(HELP))
        } else {
            format!(" {}", self.status)
        });
        if show_preview {
            lines.push(String::new());
            lines.extend(preview);
        }
        lines.truncate(rows);

        // Redraw in place: home, overwrite each line, clear what is left.
        let mut frame = String::from("\x1b[H");
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                frame.push_str("\r\n");
            }
            frame.push_str(&text::truncate(line, columns));
            frame.push_str("\x1b[K");
        }
        frame.push_str("\x1b[J");
        frame
    }

    fn row(&self, index: usize, row: Row) -> String {
        let selected = index == self.cursor;
        let pointer = if selected { "›" } else { " " };
        let highlight = |text: &str| {
            if selected {
                format!("\x1b[1m{text}{RESET}")
            } else {
                text.to_string()
            }
        };
        let dim = Style {
            dim: true,
            ..Style::default()
        };
        match row {
            Row::Heading(title) => {
                let rule = Style {
                    bold: true,
                    underline: true,
                    ..Style::default()
                };
                format!(" {}", rule.paint(title))
            }
            Row::Module(module) => {
                let (name, enabled) = &self.modules[module];
                let mark = if *enabled { "x" } else { " " };
                let about = modules::find(name).map_or("Custom module.", |m| m.about);
                format!(
                    " {pointer} [{mark}] {} {}",
                    highlight(&format!("{name:<10}")),
                    dim.paint(about)
                )
            }
            Row::Setting(setting) => {
                let (setting, label) = SETTINGS[setting];
                let value = self.value(setting);
                format!(
                    " {pointer} {} ‹ {} ›",
                    highlight(&format!("{label:<16}")),
                    // Leading and trailing spaces would be invisible.
                    if value != value.trim() {
                        format!("\"{value}\"")
                    } else {
                        value
                    }
                )
            }
        }
    }

    /// What superfetch would print with the settings as they are now.
    fn preview(&self, columns: usize) -> Vec<String> {
        let logo = Logo::load(&self.config.logo);
        let theme = Theme::new(&self.config, logo.as_ref(), true);
        let (lines, _) = modules::collect(&self.config, &theme);
        render::render(&lines, logo.as_ref(), &self.config, &theme, Some(columns))
            .lines()
            .map(String::from)
            .collect()
    }
}

/// `path` for display, with the home directory abbreviated.
fn shorten(path: &Path) -> String {
    match config::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => path.display().to_string(),
    }
}

fn bar_sample(filled: &str, empty: &str) -> String {
    format!("{}{}", filled.repeat(4), empty.repeat(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_keys() {
        assert!(matches!(decode(b"\x1b[A"), Key::Up));
        assert!(matches!(decode(b"j"), Key::Down));
        assert!(matches!(decode(b"\x1b[1;2B"), Key::MoveDown));
        assert!(matches!(decode(b" "), Key::Toggle));
        assert!(matches!(decode(b"\x03"), Key::Quit));
        assert!(matches!(decode(b"\x1b[5~"), Key::Other));
    }

    #[test]
    fn toggling_and_reordering_update_the_layout() {
        let mut editor = Editor::new(Config::default());
        assert!(!editor.is_dirty());

        // The cursor starts on the first module, "title".
        editor.handle(Key::Toggle);
        assert_eq!(editor.config.modules[0], "separator");
        editor.handle(Key::Toggle);
        assert_eq!(editor.config, editor.saved);

        editor.handle(Key::MoveDown);
        assert_eq!(&editor.config.modules[..2], ["separator", "title"]);
        assert!(matches!(editor.rows[editor.cursor], Row::Module(1)));
        editor.handle(Key::MoveUp);
        assert_eq!(editor.config, editor.saved);
    }

    #[test]
    fn disabled_modules_are_offered() {
        let editor = Editor::new(Config::default());
        let shell = editor.modules.iter().find(|(name, _)| name == "shell");
        assert_eq!(shell, Some(&("shell".to_string(), false)));
        assert_eq!(editor.modules.len(), MODULES.len());
    }

    #[test]
    fn settings_cycle_in_both_directions() {
        let mut editor = Editor::new(Config::default());
        for (setting, _) in SETTINGS {
            let before = editor.value(setting);
            editor.change(setting, 1);
            assert_ne!(editor.value(setting), before);
            editor.change(setting, -1);
            assert_eq!(editor.value(setting), before);
        }
        assert!(!editor.is_dirty());

        editor.change(Setting::Accent, -1);
        assert_eq!(editor.config.theme.accent, "bright_white");
    }

    #[test]
    fn cursor_skips_headings() {
        let mut editor = Editor::new(Config::default());
        editor.handle(Key::Up);
        assert_eq!(editor.cursor, 1);
        for _ in 0..editor.modules.len() {
            editor.handle(Key::Down);
        }
        assert!(matches!(editor.rows[editor.cursor], Row::Setting(0)));
    }
}
