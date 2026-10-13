//! The look of one run: styles from the config, resolved against the logo's
//! colors and the decision whether to use color at all.

use crate::config::{BarConfig, Config};
use crate::logo::Logo;
use crate::style::{Color, Style};

pub struct Theme {
    pub color: bool,
    /// What `accent` means inside format strings.
    accent: Option<Color>,
    pub title: Style,
    pub keys: Style,
    pub text: Style,
    pub muted: Style,
    good: Style,
    warn: Style,
    critical: Style,
    thresholds: (f64, f64),
}

impl Theme {
    pub fn new(config: &Config, logo: Option<&Logo>, color: bool) -> Theme {
        let theme = &config.theme;
        // An explicit accent wins everywhere. Otherwise the title and the
        // labels each take the color the logo designates for them.
        let fixed = match theme.accent.as_str() {
            "auto" => None,
            name => Color::parse(name).or_else(|| {
                crate::warn(&format!("theme.accent: unknown color '{name}'"));
                None
            }),
        };
        let [title_accent, key_accent] = match (fixed, logo) {
            (Some(color), _) => [Some(color); 2],
            (None, Some(logo)) => logo.accents,
            (None, None) => [None; 2],
        };

        let parse = |key: &str, spec: &str, accent: Option<Color>| {
            if !color {
                return Style::default();
            }
            Style::parse(spec, accent).unwrap_or_else(|err| {
                crate::warn(&format!("theme.{key}: {err}"));
                Style::default()
            })
        };
        Theme {
            color,
            accent: key_accent,
            title: parse("title", &theme.title, title_accent),
            keys: parse("keys", &theme.keys, key_accent),
            text: parse("text", &theme.text, key_accent),
            muted: parse("muted", &theme.muted, key_accent),
            good: parse("good", &theme.good, key_accent),
            warn: parse("warn", &theme.warn, key_accent),
            critical: parse("critical", &theme.critical, key_accent),
            thresholds: (config.thresholds.warn, config.thresholds.critical),
        }
    }

    /// Resolves a `{#spec}` tag: either the name of a theme style or an
    /// ad-hoc style such as `bold red`.
    pub fn style(&self, spec: &str) -> Style {
        if !self.color {
            return Style::default();
        }
        match spec {
            "title" => self.title,
            "keys" => self.keys,
            "text" => self.text,
            "muted" => self.muted,
            "good" => self.good,
            "warn" => self.warn,
            "critical" => self.critical,
            _ => Style::parse(spec, self.accent).unwrap_or_else(|err| {
                crate::warn(&format!("format tag {{#{spec}}}: {err}"));
                Style::default()
            }),
        }
    }

    /// The style for a usage percentage, by threshold.
    pub fn level(&self, percent: f64) -> Style {
        if percent >= self.thresholds.1 {
            self.critical
        } else if percent >= self.thresholds.0 {
            self.warn
        } else {
            self.good
        }
    }

    pub fn percent(&self, percent: f64) -> String {
        self.level(percent).paint(&format!("{percent:.0}%"))
    }

    pub fn bar(&self, percent: f64, bar: &BarConfig) -> String {
        let filled = ((percent / 100.0).clamp(0.0, 1.0) * bar.width as f64).round() as usize;
        let mut out = bar.left.clone();
        out.push_str(&self.level(percent).paint(&bar.filled.repeat(filled)));
        out.push_str(&self.muted.paint(&bar.empty.repeat(bar.width - filled)));
        out.push_str(&bar.right);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_fills_proportionally() {
        let mut config = Config::default();
        config.bar.filled = "#".into();
        config.bar.empty = "-".into();
        config.bar.left = "[".into();
        config.bar.right = "]".into();
        let theme = Theme::new(&config, None, false);
        assert_eq!(theme.bar(0.0, &config.bar), "[----------]");
        assert_eq!(theme.bar(50.0, &config.bar), "[#####-----]");
        assert_eq!(theme.bar(100.0, &config.bar), "[##########]");
        assert_eq!(theme.bar(250.0, &config.bar), "[##########]");
    }

    #[test]
    fn levels_follow_thresholds() {
        let theme = Theme::new(&Config::default(), None, true);
        assert_eq!(theme.percent(12.0), "\x1b[32m12%\x1b[0m");
        assert_eq!(theme.percent(50.0), "\x1b[33m50%\x1b[0m");
        assert_eq!(theme.percent(81.4), "\x1b[31m81%\x1b[0m");
    }

    #[test]
    fn explicit_accent_overrides_the_logo() {
        let mut config = Config::default();
        config.theme.accent = "blue".into();
        let theme = Theme::new(&config, None, true);
        assert_eq!(theme.keys.sgr(), "\x1b[1;34m");
        assert_eq!(theme.style("accent").sgr(), "\x1b[34m");
        assert!(Theme::new(&config, None, false).keys.is_plain());
    }
}
