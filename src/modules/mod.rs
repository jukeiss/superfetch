//! Modules: the units of output. Each one produces a line.
//!
//! A module is a `Module` constant plus a `fetch` function that returns
//! named fields. What ends up on screen is decided by the module's format
//! string, so `fetch` only has to gather data, and only the fields the
//! format actually mentions (see [`Request::wants`]).
//!
//! # Adding a module
//!
//! 1. Create `src/modules/<name>.rs` with a `pub const MODULE` and a
//!    `fetch` function; `uptime.rs` is a good one to copy.
//! 2. Declare the file below and add the constant to [`MODULES`].
//!
//! The config table, `--list`, the flags and the interactive editor all
//! read from that registry, so there is nothing else to wire up.

mod cpu;
mod custom;
mod disk;
mod gpu;
mod host;
mod kernel;
mod layout;
mod memory;
mod network;
mod os;
mod shell;
mod swap;
mod terminal;
mod title;
mod uptime;

use std::env;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{Config, ModuleConfig};
use crate::format;
use crate::style::Style;
use crate::sys::Usage;
use crate::template::{Context, Template};
use crate::theme::Theme;

/// Named values a module hands to its format string.
pub type Fields = Vec<(&'static str, String)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A regular `label: value` line.
    Info,
    /// A rule as wide as the line above it.
    Separator,
    /// An empty line.
    Break,
    /// The terminal's color palette.
    Colors,
}

pub struct Module {
    pub name: &'static str,
    pub kind: Kind,
    /// Default label; empty for a line without one.
    pub label: &'static str,
    /// Default format string.
    pub format: &'static str,
    /// One line of help, shown by `--list` and written to the config file.
    pub about: &'static str,
    /// The fields `fetch` can return.
    pub fields: &'static [&'static str],
    /// Extra settings, as the TOML lines to put in the default config file.
    pub settings: &'static str,
    /// Run `fetch` on its own thread. Worth it for anything that may take
    /// more than a fraction of a millisecond.
    pub threaded: bool,
    pub fetch: fn(&Request) -> Fields,
}

/// Every built-in module, in the order they are listed and offered.
pub static MODULES: &[Module] = &[
    title::MODULE,
    layout::SEPARATOR,
    os::MODULE,
    kernel::MODULE,
    host::MODULE,
    uptime::MODULE,
    shell::MODULE,
    terminal::MODULE,
    cpu::MODULE,
    gpu::MODULE,
    memory::MODULE,
    swap::MODULE,
    disk::MODULE,
    network::MODULE,
    layout::BREAK,
    layout::COLORS,
];

/// The modules shown when the config file does not say otherwise.
pub const DEFAULT_LAYOUT: &[&str] = &[
    "title",
    "separator",
    "os",
    "kernel",
    "host",
    "uptime",
    "cpu",
    "gpu",
    "memory",
    "disk",
    "network",
    "break",
    "colors",
];

pub fn find(name: &str) -> Option<&'static Module> {
    MODULES.iter().find(|module| module.name == name)
}

/// What a module gets to work with.
pub struct Request<'a> {
    template: &'a Template,
    pub settings: &'a ModuleConfig,
    pub config: &'a Config,
    pub theme: &'a Theme,
}

impl Request<'_> {
    /// Whether the format string uses `field`. Check this before doing
    /// anything expensive to produce it.
    pub fn wants(&self, field: &str) -> bool {
        self.template.wants(field)
    }

    /// The fields common to everything measured as "used out of total".
    pub fn usage_fields(&self, usage: Usage) -> Fields {
        if usage.total == 0 {
            return Vec::new();
        }
        let units = &self.config.units;
        let percent = usage.used as f64 / usage.total as f64 * 100.0;
        vec![
            ("used", format::size(usage.used, units)),
            ("total", format::size(usage.total, units)),
            (
                "free",
                format::size(usage.total.saturating_sub(usage.used), units),
            ),
            ("percent", self.theme.percent(percent)),
            ("bar", self.theme.bar(percent, &self.config.bar)),
        ]
    }
}

/// One line of output, before layout.
#[derive(Debug, PartialEq)]
pub enum Line {
    Entry {
        key: String,
        value: String,
    },
    Text(String),
    /// Holds the symbol to repeat.
    Separator(String),
    Blank,
}

pub struct Timing {
    pub name: String,
    pub elapsed: Duration,
    pub threaded: bool,
}

struct Job<'a> {
    name: &'a str,
    label: &'a str,
    template: Template,
    settings: &'a ModuleConfig,
    threaded: bool,
    fetch: fn(&Request) -> Fields,
}

impl Job<'_> {
    fn run(&self, config: &Config, theme: &Theme) -> (Fields, Duration) {
        let start = Instant::now();
        let fields = (self.fetch)(&Request {
            template: &self.template,
            settings: self.settings,
            config,
            theme,
        });
        (fields, start.elapsed())
    }
}

enum Slot<'a> {
    Ready(Vec<Line>),
    Pending(Job<'a>),
}

/// Runs every module named in `config.modules` and returns their lines.
pub fn collect(config: &Config, theme: &Theme) -> (Vec<Line>, Vec<Timing>) {
    let slots: Vec<Slot> = config
        .modules
        .iter()
        .filter_map(|name| plan(name, config, theme))
        .collect();

    // Slow modules get a thread each; the quick ones run on this thread in
    // the meantime, so the total is close to the slowest single module.
    let results: Vec<Option<(Fields, Duration)>> = thread::scope(|scope| {
        let threads: Vec<_> = slots
            .iter()
            .map(|slot| match slot {
                Slot::Pending(job) if job.threaded => {
                    Some(scope.spawn(move || job.run(config, theme)))
                }
                _ => None,
            })
            .collect();
        let inline: Vec<_> = slots
            .iter()
            .map(|slot| match slot {
                Slot::Pending(job) if !job.threaded => Some(job.run(config, theme)),
                _ => None,
            })
            .collect();
        threads
            .into_iter()
            .zip(inline)
            .map(|(thread, inline)| match thread {
                // A module that panics loses its line, nothing more.
                Some(thread) => thread.join().ok(),
                None => inline,
            })
            .collect()
    });

    let mut lines = Vec::new();
    let mut timings = Vec::new();
    for (slot, result) in slots.into_iter().zip(results) {
        let job = match slot {
            Slot::Ready(ready) => {
                lines.extend(ready);
                continue;
            }
            Slot::Pending(job) => job,
        };
        let Some((fields, elapsed)) = result else {
            continue;
        };
        timings.push(Timing {
            name: job.name.to_string(),
            elapsed,
            threaded: job.threaded,
        });
        let value = job.template.render(&Values {
            fields: &fields,
            theme,
        });
        if value.trim().is_empty() {
            continue;
        }
        lines.push(if job.label.is_empty() {
            Line::Text(value)
        } else {
            Line::Entry {
                key: job.label.to_string(),
                value,
            }
        });
    }
    (lines, timings)
}

/// Decides what a name in `config.modules` stands for.
fn plan<'a>(name: &'a str, config: &'a Config, theme: &Theme) -> Option<Slot<'a>> {
    let settings = config.module(name);
    let Some(module) = find(name) else {
        // Not built in, so it has to be defined in the config file.
        if !config.module.contains_key(name) {
            crate::warn(&format!(
                "unknown module '{name}' (see `superfetch --list`, or define [module.{name}])"
            ));
            return None;
        }
        return Some(Slot::Pending(Job {
            name,
            label: settings.label.as_deref().unwrap_or(name),
            template: Template::parse(settings.format.as_deref().unwrap_or("{output}")),
            settings,
            threaded: settings.command.is_some(),
            fetch: custom::fetch,
        }));
    };
    Some(match module.kind {
        Kind::Info => Slot::Pending(Job {
            name,
            label: settings.label.as_deref().unwrap_or(module.label),
            template: Template::parse(settings.format.as_deref().unwrap_or(module.format)),
            settings,
            threaded: module.threaded,
            fetch: module.fetch,
        }),
        Kind::Break => Slot::Ready(vec![Line::Blank]),
        Kind::Separator => {
            let symbol = settings
                .symbol
                .as_deref()
                .unwrap_or(layout::SEPARATOR_SYMBOL);
            Slot::Ready(vec![Line::Separator(symbol.to_string())])
        }
        Kind::Colors => {
            let symbol = settings.symbol.as_deref().unwrap_or(layout::COLORS_SYMBOL);
            Slot::Ready(layout::palette(symbol, theme))
        }
    })
}

/// Connects a module's fields to its format string.
struct Values<'a> {
    fields: &'a Fields,
    theme: &'a Theme,
}

impl Context for Values<'_> {
    fn field(&self, name: &str) -> Option<String> {
        if let Some(variable) = name.strip_prefix("env:") {
            return env::var(variable).ok();
        }
        self.fields
            .iter()
            .find(|(field, _)| *field == name)
            .map(|(_, value)| value.clone())
    }

    fn style(&self, spec: &str) -> Style {
        self.theme.style(spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn showing(names: &[&str]) -> Config {
        Config {
            modules: names.iter().map(|name| name.to_string()).collect(),
            ..Config::default()
        }
    }

    fn lines(config: &Config) -> Vec<Line> {
        collect(config, &Theme::new(config, None, false)).0
    }

    #[test]
    fn registry_is_consistent() {
        for (i, module) in MODULES.iter().enumerate() {
            assert!(MODULES[..i].iter().all(|m| m.name != module.name));
            assert!(!module.about.is_empty(), "{}", module.name);
            let template = Template::parse(module.format);
            for field in module.fields {
                assert_eq!(field.trim(), *field);
            }
            if module.kind == Kind::Info {
                assert!(
                    module.fields.iter().any(|field| template.wants(field)),
                    "{}: default format uses none of its fields",
                    module.name
                );
            }
        }
    }

    #[test]
    fn modules_only_return_declared_fields() {
        let mut config = Config::default();
        // Measure directly, so the test leaves nothing in the cache directory.
        config.module.insert(
            "cpu".into(),
            ModuleConfig {
                sample_ms: Some(1),
                ..ModuleConfig::default()
            },
        );
        let theme = Theme::new(&config, None, false);
        for module in MODULES.iter().filter(|m| m.kind == Kind::Info) {
            // A format that asks for everything, to exercise every branch.
            let everything: String = module.fields.iter().map(|f| format!("{{{f}}}")).collect();
            let template = Template::parse(&everything);
            let fields = (module.fetch)(&Request {
                template: &template,
                settings: config.module(module.name),
                config: &config,
                theme: &theme,
            });
            for (name, _) in fields {
                assert!(module.fields.contains(&name), "{}: {name}", module.name);
            }
        }
    }

    #[test]
    fn layout_follows_the_module_list() {
        let config = showing(&["break", "separator", "os"]);
        let lines = lines(&config);
        assert_eq!(lines[0], Line::Blank);
        assert_eq!(lines[1], Line::Separator("─".into()));
        assert!(matches!(&lines[2], Line::Entry { key, .. } if key == "OS"));
    }

    #[test]
    fn label_and_format_can_be_overridden() {
        let mut config = showing(&["os", "note"]);
        config.module.insert(
            "os".into(),
            ModuleConfig {
                label: Some("System".into()),
                format: Some("fixed".into()),
                ..ModuleConfig::default()
            },
        );
        config.module.insert(
            "note".into(),
            ModuleConfig {
                label: Some(String::new()),
                format: Some("{{literal}}".into()),
                ..ModuleConfig::default()
            },
        );
        assert_eq!(
            lines(&config),
            [
                Line::Entry {
                    key: "System".into(),
                    value: "fixed".into()
                },
                Line::Text("{literal}".into()),
            ]
        );
    }

    #[test]
    fn empty_values_drop_the_line() {
        let mut config = showing(&["os"]);
        config.module.insert(
            "os".into(),
            ModuleConfig {
                format: Some("{? {no_such_field}}".into()),
                ..ModuleConfig::default()
            },
        );
        assert!(lines(&config).is_empty());
    }
}
