//! Command-line arguments.
//!
//! Parsed by hand: the grammar is small, and an argument parsing crate
//! would cost more start-up time than the rest of the program combined.

use std::path::PathBuf;

use crate::config::{Config, Position};
use crate::modules::{self, MODULES};
use crate::style::Color;
use crate::text;

pub const HELP: &str = "\
A fast, configurable system information tool.

Usage: superfetch [OPTIONS]

Display:
  -c, --color <COLOR>        Accent color of the title and labels: a name,
                             0-255, #rrggbb, or auto to follow the logo
  -l, --logo <NAME|PATH>     Logo to draw: auto, none, a built-in name or a file
      --logo-position <POS>  Where the logo goes: left, right or top
  -m, --modules <LIST>       Show exactly these modules, in this order
  -t, --toggle <LIST>        Flip modules on or off
      --show <LIST>          Turn modules on
      --hide <LIST>          Turn modules off
      --set <KEY=VALUE>      Override any config key, e.g. --set bar.width=20
      --no-color             Print plain text
      --force-color          Use color even when the output is not a terminal

Configuration:
      --config               Open the interactive editor
      --config-file <PATH>   Use PATH instead of the default config file
      --save                 Write the display options given to the config file
      --default-config       Print the default config file

Other:
      --list                 List the available modules and logos
      --timings              Report how long each module took
  -h, --help                 Print this help
  -V, --version              Print the version

LIST is one or more module names separated by commas.
";

#[derive(Debug, Default, PartialEq)]
pub enum Action {
    #[default]
    Fetch,
    Configure,
    Help,
    Version,
    List,
    DefaultConfig,
}

#[derive(Debug, PartialEq)]
pub enum Change {
    Toggle(String),
    Show(String),
    Hide(String),
}

#[derive(Debug, Default, PartialEq)]
pub struct Args {
    pub action: Action,
    pub config_file: Option<PathBuf>,
    pub color: Option<String>,
    pub logo: Option<String>,
    pub logo_position: Option<Position>,
    pub modules: Option<Vec<String>>,
    /// `--toggle`, `--show` and `--hide`, in the order they were given.
    pub changes: Vec<Change>,
    /// `--set` pairs: a dotted config key and a TOML value.
    pub overrides: Vec<(String, String)>,
    pub no_color: bool,
    pub force_color: bool,
    pub save: bool,
    pub timings: bool,
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        // Both `--flag value` and `--flag=value` are accepted.
        let (flag, mut inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => {
                (flag.to_string(), Some(value.to_string()))
            }
            _ => (arg, None),
        };
        let mut value = || {
            inline
                .take()
                .or_else(|| args.next())
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        let list = |value: String| -> Vec<String> {
            value
                .split(',')
                .map(|name| name.trim().to_lowercase())
                .filter(|name| !name.is_empty())
                .collect()
        };

        match flag.as_str() {
            "-h" | "--help" => parsed.action = Action::Help,
            "-V" | "--version" => parsed.action = Action::Version,
            "--list" => parsed.action = Action::List,
            "--config" => parsed.action = Action::Configure,
            "--default-config" => parsed.action = Action::DefaultConfig,
            "--config-file" => parsed.config_file = Some(PathBuf::from(value()?)),
            "-c" | "--color" => parsed.color = Some(value()?),
            "-l" | "--logo" => parsed.logo = Some(value()?),
            "--logo-position" => {
                let position = value()?;
                parsed.logo_position = Some(Position::parse(&position).ok_or_else(|| {
                    format!("--logo-position: expected left, right or top, got '{position}'")
                })?);
            }
            "-m" | "--modules" => parsed.modules = Some(list(value()?)),
            "-t" | "--toggle" => parsed
                .changes
                .extend(list(value()?).into_iter().map(Change::Toggle)),
            "--show" => parsed
                .changes
                .extend(list(value()?).into_iter().map(Change::Show)),
            "--hide" => parsed
                .changes
                .extend(list(value()?).into_iter().map(Change::Hide)),
            "--set" => {
                let pair = value()?;
                let (key, value) = pair
                    .split_once('=')
                    .ok_or_else(|| format!("--set expects KEY=VALUE, got '{pair}'"))?;
                // The value is kept verbatim: its whitespace may be the point.
                parsed
                    .overrides
                    .push((key.trim().to_string(), value.to_string()));
            }
            "--no-color" => parsed.no_color = true,
            "--force-color" => parsed.force_color = true,
            "--save" => parsed.save = true,
            "--timings" => parsed.timings = true,
            _ => return Err(format!("unknown option '{flag}' (try --help)")),
        }
        if let Some(unused) = inline {
            return Err(format!("{flag} does not take a value (got '{unused}')"));
        }
    }
    Ok(parsed)
}

impl Args {
    /// Applies the display options on top of what the config file says.
    pub fn apply(&self, config: &mut Config) -> Result<(), String> {
        let check = |name: &str| {
            if modules::find(name).is_some() || config.module.contains_key(name) {
                return Ok(());
            }
            let known = MODULES.iter().map(|module| module.name);
            let custom = config.module.keys().map(String::as_str);
            Err(match text::closest(name, known.chain(custom)) {
                Some(guess) => format!("unknown module '{name}' (did you mean '{guess}'?)"),
                None => format!("unknown module '{name}' (see --list)"),
            })
        };

        if let Some(names) = &self.modules {
            names.iter().try_for_each(|name| check(name))?;
        }
        for change in &self.changes {
            let (Change::Toggle(name) | Change::Show(name) | Change::Hide(name)) = change;
            check(name)?;
        }

        if let Some(names) = &self.modules {
            config.modules = names.clone();
        }
        for change in &self.changes {
            match change {
                Change::Show(name) => show(&mut config.modules, name),
                Change::Hide(name) => config.modules.retain(|module| module != name),
                Change::Toggle(name) if config.modules.contains(name) => {
                    config.modules.retain(|module| module != name);
                }
                Change::Toggle(name) => show(&mut config.modules, name),
            }
        }

        if let Some(color) = &self.color {
            if color != "auto" && Color::parse(color).is_none() {
                return Err(format!(
                    "--color: '{color}' is not a color (use a name such as blue, 0-255, or #rrggbb)"
                ));
            }
            config.theme.accent = color.clone();
        }
        if let Some(logo) = &self.logo {
            config.logo.source = logo.clone();
        }
        if let Some(position) = self.logo_position {
            config.logo.position = position;
        }
        Ok(())
    }
}

/// Adds `name` to the layout where it would sit by default, so a module
/// that is switched on lands among its neighbours rather than at the end.
pub fn show(layout: &mut Vec<String>, name: &str) {
    if layout.iter().any(|module| module == name) {
        return;
    }
    let rank = |name: &str| MODULES.iter().position(|module| module.name == name);
    let position = rank(name).and_then(|own| {
        layout
            .iter()
            .position(|other| rank(other).is_some_and(|rank| rank > own))
    });
    layout.insert(position.unwrap_or(layout.len()), name.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(line: &str) -> Result<Args, String> {
        parse(line.split_whitespace().map(String::from))
    }

    fn applied(line: &str) -> Result<Config, String> {
        let mut config = Config::default();
        parse_str(line)?.apply(&mut config)?;
        Ok(config)
    }

    #[test]
    fn parses_both_value_styles() {
        let spaced = parse_str("--color blue --logo arch -t gpu,disk").unwrap();
        let inline = parse_str("--color=blue --logo=arch --toggle=gpu,disk").unwrap();
        assert_eq!(spaced, inline);
        assert_eq!(spaced.color.as_deref(), Some("blue"));
        assert_eq!(
            spaced.changes,
            [Change::Toggle("gpu".into()), Change::Toggle("disk".into())]
        );
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(parse_str("--frobnicate").is_err());
        assert!(parse_str("--color").is_err());
        assert!(parse_str("--no-color=yes").is_err());
        assert!(parse_str("--set bar.width").is_err());
        assert!(parse_str("--logo-position middle").is_err());
    }

    #[test]
    fn set_keeps_everything_after_the_first_equals() {
        let args = parse_str("--set layout.key_separator==").unwrap();
        assert_eq!(
            args.overrides,
            [("layout.key_separator".into(), "=".into())]
        );
        let args = parse_str("--set=bar.width=20").unwrap();
        assert_eq!(args.overrides, [("bar.width".into(), "20".into())]);
    }

    #[test]
    fn toggle_flips_modules() {
        let config = applied("--toggle gpu,shell").unwrap();
        assert!(!config.modules.contains(&"gpu".to_string()));
        // Switched on between uptime and cpu, where it is listed.
        let shell = config.modules.iter().position(|m| m == "shell").unwrap();
        assert_eq!(config.modules[shell - 1], "uptime");
        assert_eq!(config.modules[shell + 1], "cpu");
    }

    #[test]
    fn show_and_hide_are_idempotent() {
        let config = applied("--show os --hide swap --hide os --show os").unwrap();
        assert_eq!(config.modules.iter().filter(|m| *m == "os").count(), 1);
        assert_eq!(config.modules.len(), Config::default().modules.len());
    }

    #[test]
    fn modules_replaces_the_layout() {
        let config = applied("-m os,cpu,memory").unwrap();
        assert_eq!(config.modules, ["os", "cpu", "memory"]);
    }

    #[test]
    fn unknown_modules_get_a_suggestion() {
        let error = applied("--toggle gnu").unwrap_err();
        assert!(error.contains("did you mean 'gpu'"), "{error}");
        let error = applied("--hide xyzzy").unwrap_err();
        assert!(error.contains("--list"), "{error}");
    }

    #[test]
    fn color_is_validated() {
        assert_eq!(applied("--color blue").unwrap().theme.accent, "blue");
        assert_eq!(applied("-c #ff8800").unwrap().theme.accent, "#ff8800");
        assert!(applied("--color blurple").is_err());
    }
}
