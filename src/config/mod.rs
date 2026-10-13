//! The configuration file: its schema, defaults, and where it lives.
//!
//! Defaults are defined once, in the `Default` impls below. The file written
//! on first run is `default.toml` followed by one table per built-in module,
//! generated from the module registry; a test keeps that file and the
//! `Default` impls in agreement.

mod save;

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use crate::modules::{self, Kind};
use crate::text;
use crate::toml::{self, Value};

pub use save::save;

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Names of the modules to print, in order.
    pub modules: Vec<String>,
    pub logo: LogoConfig,
    pub theme: ThemeConfig,
    pub thresholds: Thresholds,
    pub layout: LayoutConfig,
    pub bar: BarConfig,
    pub units: UnitsConfig,
    /// Per-module overrides, and definitions of custom modules.
    pub module: BTreeMap<String, ModuleConfig>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogoConfig {
    pub source: String,
    pub position: Position,
    pub gap: usize,
    pub indent: usize,
    pub colors: Vec<String>,
    pub bold: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    Left,
    Right,
    Top,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeConfig {
    pub accent: String,
    pub title: String,
    pub keys: String,
    pub text: String,
    pub muted: String,
    pub good: String,
    pub warn: String,
    pub critical: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Thresholds {
    pub warn: f64,
    pub critical: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutConfig {
    pub key_separator: String,
    pub key_align: Align,
    pub truncate: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BarConfig {
    pub width: usize,
    pub filled: String,
    pub empty: String,
    pub left: String,
    pub right: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnitsConfig {
    pub binary: bool,
    pub precision: usize,
}

/// Settings of one module. `label` and `format` apply to every module; the
/// rest are read only by the modules that document them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModuleConfig {
    pub label: Option<String>,
    pub format: Option<String>,
    /// `separator`, `colors`: the character(s) to repeat.
    pub symbol: Option<String>,
    /// `disk`: the mount point to report on.
    pub path: Option<String>,
    /// `cpu`: how long to measure utilisation for, in milliseconds.
    pub sample_ms: Option<u64>,
    /// Custom modules: a shell command whose first output line is `{output}`.
    pub command: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            modules: modules::DEFAULT_LAYOUT
                .iter()
                .map(|s| s.to_string())
                .collect(),
            logo: LogoConfig::default(),
            theme: ThemeConfig::default(),
            thresholds: Thresholds::default(),
            layout: LayoutConfig::default(),
            bar: BarConfig::default(),
            units: UnitsConfig::default(),
            module: BTreeMap::new(),
        }
    }
}

impl Default for LogoConfig {
    fn default() -> Self {
        LogoConfig {
            source: "auto".into(),
            position: Position::Left,
            gap: 3,
            indent: 0,
            colors: Vec::new(),
            bold: true,
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        ThemeConfig {
            accent: "auto".into(),
            title: "bold accent".into(),
            keys: "bold accent".into(),
            text: "default".into(),
            muted: "dim".into(),
            good: "green".into(),
            warn: "yellow".into(),
            critical: "red".into(),
        }
    }
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            warn: 50.0,
            critical: 80.0,
        }
    }
}

impl Default for LayoutConfig {
    fn default() -> Self {
        LayoutConfig {
            key_separator: ": ".into(),
            key_align: Align::None,
            truncate: true,
        }
    }
}

impl Default for BarConfig {
    fn default() -> Self {
        BarConfig {
            width: 10,
            filled: "━".into(),
            empty: "─".into(),
            left: String::new(),
            right: String::new(),
        }
    }
}

impl Default for UnitsConfig {
    fn default() -> Self {
        UnitsConfig {
            binary: true,
            precision: 2,
        }
    }
}

impl Position {
    pub const ALL: [Position; 3] = [Position::Left, Position::Right, Position::Top];

    pub fn name(self) -> &'static str {
        match self {
            Position::Left => "left",
            Position::Right => "right",
            Position::Top => "top",
        }
    }

    pub fn parse(s: &str) -> Option<Position> {
        Position::ALL.into_iter().find(|p| p.name() == s)
    }
}

impl Align {
    pub const ALL: [Align; 3] = [Align::None, Align::Left, Align::Right];

    pub fn name(self) -> &'static str {
        match self {
            Align::None => "none",
            Align::Left => "left",
            Align::Right => "right",
        }
    }

    pub fn parse(s: &str) -> Option<Align> {
        Align::ALL.into_iter().find(|a| a.name() == s)
    }
}

impl Config {
    /// Settings of `name`, or the empty defaults when the file has no table
    /// for it.
    pub fn module(&self, name: &str) -> &ModuleConfig {
        static EMPTY: ModuleConfig = ModuleConfig {
            label: None,
            format: None,
            symbol: None,
            path: None,
            sample_ms: None,
            command: None,
        };
        self.module.get(name).unwrap_or(&EMPTY)
    }
}

/// The user's home directory, as the environment reports it.
pub fn home_dir() -> Option<PathBuf> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env::var_os(variable)
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// `$XDG_CONFIG_HOME/superfetch/config.toml`, falling back to `~/.config` on
/// Unix and `%APPDATA%` on Windows.
pub fn default_path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            if cfg!(windows) {
                env::var_os("APPDATA").map(PathBuf::from)
            } else {
                home_dir().map(|home| home.join(".config"))
            }
        })?;
    Some(base.join("superfetch").join("config.toml"))
}

/// Reads the configuration.
///
/// With an explicit `path` every problem is an error. The default file is
/// more forgiving: it is created when missing, and if it cannot be parsed
/// superfetch says so and carries on with the defaults, so a typo never
/// leaves a shell start-up without output.
///
/// `overrides` are `--set` pairs: a dotted key and a TOML value.
pub fn load(path: Option<&Path>, overrides: &[(String, String)]) -> Result<Config, String> {
    let (path, explicit) = match path {
        Some(path) => (Some(path.to_path_buf()), true),
        None => (default_path(), false),
    };

    let mut config = Config::default();
    if let Some(path) = &path {
        match fs::read_to_string(path) {
            Ok(text) => match Config::parse(&text) {
                Ok(parsed) => config = parsed,
                Err(err) if explicit => return Err(format!("{}: {err}", path.display())),
                Err(err) => crate::warn(&format!(
                    "{}: {err}\nusing the default configuration",
                    path.display()
                )),
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound && !explicit => {
                // Best effort: a read-only home directory is not a reason to fail.
                let _ = write_default(path);
            }
            Err(err) => return Err(format!("cannot read {}: {err}", path.display())),
        }
    }

    for (key, raw) in overrides {
        config
            .set(&split_key(key), override_value(raw))
            .map_err(|err| format!("--set {key}: {err}"))?;
    }
    Ok(config)
}

/// Reads the file at `path`, or returns the defaults if there is none yet.
pub fn load_or_default(path: &Path) -> Result<Config, String> {
    if path.exists() {
        load(Some(path), &[])
    } else {
        Ok(Config::default())
    }
}

/// Splits the dotted key of a `--set` flag.
pub fn split_key(key: &str) -> Vec<&str> {
    key.split('.').map(str::trim).collect()
}

/// Interprets a `--set` value as TOML, so `3`, `true` and `["a", "b"]` keep
/// their types. Anything that is not valid TOML is taken as a plain string,
/// which lets `--set theme.accent=blue` work without nested quoting.
pub fn override_value(raw: &str) -> Value {
    toml::parse_value(raw).unwrap_or_else(|| Value::from(raw))
}

/// Every key the config file understands, apart from the per-module ones.
const KEYS: &[&str] = &[
    "modules",
    "logo.source",
    "logo.position",
    "logo.gap",
    "logo.indent",
    "logo.colors",
    "logo.bold",
    "theme.accent",
    "theme.title",
    "theme.keys",
    "theme.text",
    "theme.muted",
    "theme.good",
    "theme.warn",
    "theme.critical",
    "thresholds.warn",
    "thresholds.critical",
    "layout.key_separator",
    "layout.key_align",
    "layout.truncate",
    "bar.width",
    "bar.filled",
    "bar.empty",
    "bar.left",
    "bar.right",
    "units.binary",
    "units.precision",
];

/// The keys of a `[module.<name>]` table.
const MODULE_KEYS: &[&str] = &["label", "format", "symbol", "path", "sample_ms", "command"];

/// The error for a key that does not exist. `word` is the part of the key
/// to look for among `known` when suggesting a correction.
fn unknown_key(key: &str, word: &str, known: &[&'static str]) -> String {
    match text::closest(word, known.iter().copied()) {
        Some(guess) => format!("unknown key '{key}' (did you mean '{guess}'?)"),
        None => format!("unknown key '{key}'"),
    }
}

impl Config {
    /// Builds a configuration from the text of a config file. Keys that are
    /// absent keep their defaults; keys that do not exist are errors.
    pub fn parse(source: &str) -> Result<Config, String> {
        let document = toml::parse(source).map_err(|err| err.to_string())?;
        let mut config = Config::default();

        for table in &document.tables {
            match table.path.iter().map(String::as_str).collect::<Vec<_>>()[..] {
                [] | ["logo" | "theme" | "thresholds" | "layout" | "bar" | "units" | "module"] => {}
                // A module table may be empty and still means something:
                // it defines the module.
                ["module", name] => {
                    config.module.entry(name.to_string()).or_default();
                }
                _ => {
                    return Err(format!(
                        "line {}: unknown table [{}]",
                        table.line,
                        table.path.join(".")
                    ));
                }
            }
        }
        for entry in document.entries {
            let key: Vec<&str> = entry.key.iter().map(String::as_str).collect();
            config
                .set(&key, entry.value)
                .map_err(|err| format!("line {}: {err}", entry.line))?;
        }
        Ok(config)
    }

    /// Assigns one setting, addressed the way the config file nests it.
    pub fn set(&mut self, key: &[&str], value: Value) -> Result<(), String> {
        let name = key.join(".");
        // Each helper checks the type and names the key in its error.
        let string = |value: Value| match value {
            Value::String(text) => Ok(text),
            other => Err(format!(
                "{name}: expected a string, found {}",
                other.type_name()
            )),
        };
        let boolean = |value: Value| match value {
            Value::Boolean(flag) => Ok(flag),
            other => Err(format!(
                "{name}: expected true or false, found {}",
                other.type_name()
            )),
        };
        let count = |value: Value| match value {
            Value::Integer(number) if number >= 0 => Ok(number as usize),
            other => Err(format!(
                "{name}: expected a whole number, found {}",
                match other {
                    Value::Integer(_) => "a negative one",
                    other => other.type_name(),
                }
            )),
        };
        let number = |value: Value| match value {
            Value::Integer(number) => Ok(number as f64),
            Value::Float(number) => Ok(number),
            other => Err(format!(
                "{name}: expected a number, found {}",
                other.type_name()
            )),
        };
        let strings = |value: Value| match value {
            Value::Array(items) => items.into_iter().map(string).collect::<Result<Vec<_>, _>>(),
            other => Err(format!(
                "{name}: expected an array of strings, found {}",
                other.type_name()
            )),
        };
        let choice = |value: Value, options: &str| {
            string(value).map_err(|_| format!("{name}: expected one of {options}"))
        };

        match key {
            ["modules"] => self.modules = strings(value)?,

            ["logo", "source"] => self.logo.source = string(value)?,
            ["logo", "position"] => {
                let options = "\"left\", \"right\", \"top\"";
                let text = choice(value, options)?;
                self.logo.position = Position::parse(&text).ok_or_else(|| {
                    format!("{name}: expected one of {options}, found \"{text}\"")
                })?;
            }
            ["logo", "gap"] => self.logo.gap = count(value)?,
            ["logo", "indent"] => self.logo.indent = count(value)?,
            ["logo", "colors"] => self.logo.colors = strings(value)?,
            ["logo", "bold"] => self.logo.bold = boolean(value)?,

            ["theme", "accent"] => self.theme.accent = string(value)?,
            ["theme", "title"] => self.theme.title = string(value)?,
            ["theme", "keys"] => self.theme.keys = string(value)?,
            ["theme", "text"] => self.theme.text = string(value)?,
            ["theme", "muted"] => self.theme.muted = string(value)?,
            ["theme", "good"] => self.theme.good = string(value)?,
            ["theme", "warn"] => self.theme.warn = string(value)?,
            ["theme", "critical"] => self.theme.critical = string(value)?,

            ["thresholds", "warn"] => self.thresholds.warn = number(value)?,
            ["thresholds", "critical"] => self.thresholds.critical = number(value)?,

            ["layout", "key_separator"] => self.layout.key_separator = string(value)?,
            ["layout", "key_align"] => {
                let options = "\"none\", \"left\", \"right\"";
                let text = choice(value, options)?;
                self.layout.key_align = Align::parse(&text).ok_or_else(|| {
                    format!("{name}: expected one of {options}, found \"{text}\"")
                })?;
            }
            ["layout", "truncate"] => self.layout.truncate = boolean(value)?,

            ["bar", "width"] => self.bar.width = count(value)?,
            ["bar", "filled"] => self.bar.filled = string(value)?,
            ["bar", "empty"] => self.bar.empty = string(value)?,
            ["bar", "left"] => self.bar.left = string(value)?,
            ["bar", "right"] => self.bar.right = string(value)?,

            ["units", "binary"] => self.units.binary = boolean(value)?,
            ["units", "precision"] => self.units.precision = count(value)?,

            ["module", module, field] => {
                if !MODULE_KEYS.contains(field) {
                    return Err(unknown_key(&name, field, MODULE_KEYS));
                }
                // Convert before touching the map, so a bad value does not
                // leave a half-defined module behind.
                let (text, number) = match *field {
                    "sample_ms" => (None, Some(count(value)? as u64)),
                    _ => (Some(string(value)?), None),
                };
                let settings = self.module.entry(module.to_string()).or_default();
                match *field {
                    "label" => settings.label = text,
                    "format" => settings.format = text,
                    "symbol" => settings.symbol = text,
                    "path" => settings.path = text,
                    "command" => settings.command = text,
                    _ => settings.sample_ms = number,
                }
            }

            _ => return Err(unknown_key(&name, &name, KEYS)),
        }
        Ok(())
    }
}

/// The file written on first run: every setting, at its default, with the
/// documentation next to it.
pub fn default_file() -> String {
    let mut out = String::from(include_str!("default.toml"));
    for module in modules::MODULES {
        out.push('\n');
        let _ = writeln!(out, "# {}", module.about);
        if !module.fields.is_empty() {
            let fields: Vec<String> = module.fields.iter().map(|f| format!("{{{f}}}")).collect();
            let _ = writeln!(out, "# Fields: {}", fields.join(" "));
        }
        let _ = writeln!(out, "[module.{}]", module.name);
        if module.kind == Kind::Info {
            let _ = writeln!(out, "label = {}", toml::quote(module.label));
            let _ = writeln!(out, "format = {}", toml::quote(module.format));
        }
        out.push_str(module.settings);
    }
    out
}

fn write_default(path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, default_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(overrides: &[(&str, &str)]) -> Vec<(String, String)> {
        overrides
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn with_overrides(overrides: &[(&str, &str)]) -> Result<Config, String> {
        let mut config = Config::default();
        for (key, raw) in pairs(overrides) {
            config.set(&split_key(&key), override_value(&raw))?;
        }
        Ok(config)
    }

    #[test]
    fn default_file_matches_defaults() {
        let mut parsed = Config::parse(&default_file()).expect("default file parses");
        let tables = std::mem::take(&mut parsed.module);
        assert_eq!(parsed, Config::default());

        for module in modules::MODULES {
            let table = tables.get(module.name).expect("every module has a table");
            if module.kind == Kind::Info {
                assert_eq!(table.label.as_deref(), Some(module.label));
                assert_eq!(table.format.as_deref(), Some(module.format));
            }
        }
        assert_eq!(tables.len(), modules::MODULES.len());
    }

    #[test]
    fn every_key_is_listed_and_settable() {
        // KEYS feeds the "did you mean" hints, so it has to stay in step
        // with `set`. The default file mentions every key there is.
        let document = toml::parse(&default_file()).unwrap();
        let mut seen: Vec<String> = Vec::new();
        for entry in document.entries {
            if entry.key[0] != "module" {
                seen.push(entry.key.join("."));
            }
        }
        seen.sort();
        let mut listed: Vec<String> = KEYS.iter().map(|key| key.to_string()).collect();
        listed.sort();
        assert_eq!(seen, listed);
    }

    #[test]
    fn default_layout_names_real_modules() {
        for name in modules::DEFAULT_LAYOUT {
            assert!(modules::find(name).is_some(), "unknown module {name}");
        }
    }

    #[test]
    fn overrides_keep_toml_types() {
        let config = with_overrides(&[
            ("bar.width", "24"),
            ("theme.accent", "blue"),
            ("layout.truncate", "false"),
            ("layout.key_separator", " -> "),
            ("thresholds.warn", "62.5"),
            ("modules", r#"["os", "cpu"]"#),
            ("module.cpu.label", "Processor"),
            ("module.cpu.sample_ms", "150"),
            ("module.shell.format", "{name} {version}"),
        ])
        .unwrap();
        assert_eq!(config.bar.width, 24);
        assert_eq!(config.theme.accent, "blue");
        assert!(!config.layout.truncate);
        assert_eq!(config.layout.key_separator, " -> ");
        assert_eq!(config.thresholds.warn, 62.5);
        assert_eq!(config.modules, ["os", "cpu"]);
        assert_eq!(config.module("cpu").label.as_deref(), Some("Processor"));
        assert_eq!(config.module("cpu").sample_ms, Some(150));
        assert_eq!(
            config.module("shell").format.as_deref(),
            Some("{name} {version}")
        );
    }

    #[test]
    fn rejects_unknown_keys_and_tables() {
        let error = Config::parse("[theme]\nacent = \"red\"\n").unwrap_err();
        assert_eq!(
            error,
            "line 2: unknown key 'theme.acent' (did you mean 'theme.accent'?)"
        );
        let error = Config::parse("\n[thme]\n").unwrap_err();
        assert_eq!(error, "line 2: unknown table [thme]");
        let error = with_overrides(&[("module.cpu.lable", "x")]).unwrap_err();
        assert!(error.contains("did you mean 'label'"), "{error}");
        assert!(with_overrides(&[("bar.widht", "3")]).is_err());
        assert!(with_overrides(&[("nonsense", "3")]).is_err());
    }

    #[test]
    fn rejects_values_of_the_wrong_type() {
        let cases = [
            (
                "bar.width",
                "wide",
                "expected a whole number, found a string",
            ),
            ("bar.width", "-3", "found a negative one"),
            ("logo.bold", "1", "expected true or false"),
            ("theme.accent", "7", "expected a string, found an integer"),
            ("modules", "os", "expected an array of strings"),
            ("modules", "[1, 2]", "expected a string"),
            (
                "logo.position",
                "middle",
                "expected one of \"left\", \"right\", \"top\"",
            ),
            ("layout.key_align", "true", "expected one of \"none\""),
            ("module.cpu.sample_ms", "soon", "expected a whole number"),
        ];
        for (key, value, expected) in cases {
            let error = with_overrides(&[(key, value)]).unwrap_err();
            assert!(error.contains(expected), "{key}={value}: {error}");
            assert!(error.starts_with(key), "{error}");
        }
    }

    #[test]
    fn a_bad_module_key_defines_nothing() {
        let mut config = Config::default();
        assert!(
            config
                .set(&["module", "ghost", "colour"], Value::from("x"))
                .is_err()
        );
        assert!(
            config
                .set(&["module", "ghost", "sample_ms"], Value::from("x"))
                .is_err()
        );
        assert!(!config.module.contains_key("ghost"));
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let config = Config::parse("[bar]\nwidth = 4\n").unwrap();
        assert_eq!(config.bar.width, 4);
        assert_eq!(config.bar.filled, BarConfig::default().filled);
        assert_eq!(config.modules, Config::default().modules);
    }

    #[test]
    fn an_empty_table_still_defines_a_module() {
        let config = Config::parse("[module.note]\n").unwrap();
        assert!(config.module.contains_key("note"));
    }
}
