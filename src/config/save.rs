//! Writing settings back to the config file.
//!
//! The file is edited in place, one value at a time, so comments, ordering
//! and any keys superfetch does not touch survive a save.

use std::path::Path;
use std::{fs, io};

use super::{Config, default_file, override_value, split_key};
use crate::toml::{self, Value};

/// The settings that flags and the interactive editor can change, as
/// key paths into the file.
fn editable(config: &Config) -> Vec<(&'static [&'static str], Value)> {
    let text = |text: &str| Value::String(text.to_string());
    vec![
        (
            &["modules"],
            Value::Array(config.modules.iter().map(|name| text(name)).collect()),
        ),
        (&["logo", "source"], text(&config.logo.source)),
        (&["logo", "position"], text(config.logo.position.name())),
        (&["theme", "accent"], text(&config.theme.accent)),
        (
            &["layout", "key_separator"],
            text(&config.layout.key_separator),
        ),
        (
            &["layout", "key_align"],
            text(config.layout.key_align.name()),
        ),
        (&["bar", "width"], Value::Integer(config.bar.width as i64)),
        (&["bar", "filled"], text(&config.bar.filled)),
        (&["bar", "empty"], text(&config.bar.empty)),
        (&["units", "binary"], Value::Boolean(config.units.binary)),
    ]
}

/// Updates the file at `path` with every editable setting that differs
/// between `before` and `after`, plus the raw `--set` pairs in `overrides`.
pub fn save(
    path: &Path,
    before: &Config,
    after: &Config,
    overrides: &[(String, String)],
) -> Result<(), String> {
    let mut text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => default_file(),
        Err(err) => return Err(format!("cannot read {}: {err}", path.display())),
    };
    let located = |err: toml::Error| format!("{}: {err}", path.display());

    let changed = editable(before)
        .into_iter()
        .zip(editable(after))
        .filter(|((_, old), (_, new))| old != new)
        .map(|(_, new)| new);
    for (key, value) in changed {
        text = toml::set(&text, key, &value).map_err(located)?;
    }
    for (key, raw) in overrides {
        text = toml::set(&text, &split_key(key), &override_value(raw)).map_err(located)?;
    }

    write_atomically(path, &text).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

fn write_atomically(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let staging = path.with_extension("toml.tmp");
    fs::write(&staging, contents)?;
    fs::rename(&staging, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("superfetch-{}-{name}", std::process::id()));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn save_preserves_comments_and_untouched_keys() {
        let original = "\
# above the list
modules = [\"os\"] # beside the list

# mine
[theme]
# above the key
accent = \"auto\" # follow the logo
keys = \"bold red\"
";
        let path = temp_file("comments.toml", original);
        let before = Config {
            modules: vec!["os".into()],
            ..Config::default()
        };
        let mut after = before.clone();
        after.theme.accent = "blue".into();
        after.modules.push("cpu".into());
        save(&path, &before, &after, &[]).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        fs::remove_file(&path).unwrap();
        let expected = original
            .replace("\"auto\"", "\"blue\"")
            .replace("[\"os\"]", "[\"os\", \"cpu\"]");
        assert_eq!(text, expected);
    }

    #[test]
    fn save_round_trips_through_load() {
        let path = temp_file("roundtrip.toml", &default_file());
        let before = Config::default();
        let mut after = before.clone();
        after.modules = vec!["os".into(), "memory".into()];
        after.bar.width = 20;
        after.units.binary = false;
        let overrides = [("module.os.label".to_string(), "System".to_string())];
        save(&path, &before, &after, &overrides).unwrap();

        let loaded = crate::config::load(Some(&path), &[]).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(loaded.modules, after.modules);
        assert_eq!(loaded.bar.width, 20);
        assert!(!loaded.units.binary);
        assert_eq!(loaded.module("os").label.as_deref(), Some("System"));
    }

    #[test]
    fn save_refuses_to_touch_a_broken_file() {
        let path = temp_file("broken.toml", "modules = [\n");
        let before = Config::default();
        let mut after = before.clone();
        after.bar.width = 3;
        assert!(save(&path, &before, &after, &[]).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "modules = [\n");
        fs::remove_file(&path).unwrap();
    }
}
