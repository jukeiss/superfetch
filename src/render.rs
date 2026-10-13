//! Layout: puts the logo and the module lines together.

use crate::config::{Align, Config, Position};
use crate::logo::Logo;
use crate::modules::Line;
use crate::text;
use crate::theme::Theme;

/// Columns the text needs next to a logo. In a narrower terminal the logo
/// is left out rather than squeezing the text into a sliver.
const MIN_TEXT_WIDTH: usize = 24;

/// Builds the complete output. `width` is the width of the terminal, when
/// there is one.
pub fn render(
    lines: &[Line],
    logo: Option<&Logo>,
    config: &Config,
    theme: &Theme,
    width: Option<usize>,
) -> String {
    let info = info_lines(lines, config, theme);
    let (gap, indent) = (config.logo.gap, config.logo.indent);
    let logo = logo.filter(|logo| {
        config.logo.position == Position::Top
            || width.is_none_or(|width| width >= indent + logo.width + gap + MIN_TEXT_WIDTH)
    });

    let mut rows: Vec<String> = Vec::new();
    match (logo, config.logo.position) {
        (None, _) => rows = info,
        (Some(logo), Position::Top) => {
            for index in 0..logo.height() {
                let mut row = " ".repeat(indent);
                logo.write_line(index, theme.color, &mut row);
                rows.push(row);
            }
            rows.push(String::new());
            rows.extend(info);
        }
        (Some(logo), Position::Left) => {
            for index in 0..logo.height().max(info.len()) {
                let mut row = " ".repeat(indent);
                logo.write_line(index, theme.color, &mut row);
                if let Some(line) = info.get(index).filter(|line| !line.is_empty()) {
                    text::pad(&mut row, gap);
                    row.push_str(line);
                }
                rows.push(row);
            }
        }
        (Some(logo), Position::Right) => {
            let column = info.iter().map(|line| text::width(line)).max().unwrap_or(0);
            for index in 0..logo.height().max(info.len()) {
                let mut row = info.get(index).cloned().unwrap_or_default();
                if index < logo.height() {
                    let padding = column - text::width(&row) + gap;
                    text::pad(&mut row, padding);
                    logo.write_line(index, theme.color, &mut row);
                }
                rows.push(row);
            }
        }
    }

    let mut out = String::new();
    for row in rows {
        let row = row.trim_end_matches(' ');
        match width.filter(|_| config.layout.truncate) {
            Some(width) => out.push_str(&text::truncate(row, width)),
            None => out.push_str(row),
        }
        out.push('\n');
    }
    out
}

/// Turns module output into finished lines of text.
fn info_lines(lines: &[Line], config: &Config, theme: &Theme) -> Vec<String> {
    let layout = &config.layout;
    let key_column = lines
        .iter()
        .filter_map(|line| match line {
            Line::Entry { key, .. } => Some(text::width(key)),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        out.push(match line {
            Line::Blank => String::new(),
            Line::Text(text) => text.clone(),
            Line::Separator(symbol) => {
                // As wide as the line it underlines.
                let above = out.last().map_or(0, |line| text::width(line));
                let rule: String = symbol
                    .chars()
                    .cycle()
                    .take(if symbol.is_empty() { 0 } else { above })
                    .collect();
                theme.muted.paint(&rule)
            }
            Line::Entry { key, value } => {
                let padding = key_column - text::width(key);
                let mut row = String::new();
                if layout.key_align == Align::Right {
                    text::pad(&mut row, padding);
                }
                row.push_str(&theme.keys.paint(key));
                row.push_str(&layout.key_separator);
                if layout.key_align == Align::Left {
                    text::pad(&mut row, padding);
                }
                row.push_str(&theme.text.paint(value));
                row
            }
        });
    }
    // A break before a module that printed nothing would leave a dangling
    // empty line at the end.
    while out.last().is_some_and(|line| line.is_empty()) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LogoConfig;

    fn entry(key: &str, value: &str) -> Line {
        Line::Entry {
            key: key.into(),
            value: value.into(),
        }
    }

    fn plain(lines: &[Line], config: &Config, width: Option<usize>) -> String {
        let logo = Logo::load(&config.logo);
        let theme = Theme::new(config, logo.as_ref(), false);
        render(lines, logo.as_ref(), config, &theme, width)
    }

    fn without_logo() -> Config {
        Config {
            logo: LogoConfig {
                source: "none".into(),
                ..LogoConfig::default()
            },
            ..Config::default()
        }
    }

    #[test]
    fn separator_matches_the_line_above() {
        let lines = [
            Line::Text("user@host".into()),
            Line::Separator("-".into()),
            entry("OS", "Linux"),
            Line::Blank,
        ];
        assert_eq!(
            plain(&lines, &without_logo(), None),
            "user@host\n---------\nOS: Linux\n"
        );
    }

    #[test]
    fn keys_can_be_aligned() {
        let lines = [entry("OS", "Linux"), entry("Kernel", "6.11")];
        let mut config = without_logo();
        config.layout.key_align = Align::Left;
        assert_eq!(
            plain(&lines, &config, None),
            "OS:     Linux\nKernel: 6.11\n"
        );
        config.layout.key_align = Align::Right;
        assert_eq!(
            plain(&lines, &config, None),
            "    OS: Linux\nKernel: 6.11\n"
        );
    }

    #[test]
    fn long_lines_are_cut_at_the_terminal_edge() {
        let lines = [entry("GPU", "A graphics card with a very long name")];
        let mut config = without_logo();
        assert_eq!(plain(&lines, &config, Some(12)), "GPU: A grap…\n");
        config.layout.truncate = false;
        assert!(plain(&lines, &config, Some(12)).ends_with("long name\n"));
    }

    #[test]
    fn logo_sits_beside_the_text() {
        let lines = [entry("OS", "Linux")];
        let mut config = Config::default();
        config.logo.source = "macos_small".into();
        let left = plain(&lines, &config, None);
        let first = left.lines().next().unwrap();
        assert!(first.starts_with("        .:'") && first.ends_with("   OS: Linux"));
        assert_eq!(left.lines().count(), 7);

        config.logo.position = Position::Right;
        let right = plain(&lines, &config, None);
        assert!(right.starts_with("OS: Linux           .:'"));

        config.logo.position = Position::Top;
        let top = plain(&lines, &config, None);
        assert_eq!(top.lines().count(), 9);
        assert_eq!(top.lines().last(), Some("OS: Linux"));
    }

    #[test]
    fn logo_is_dropped_when_the_terminal_is_narrow() {
        let lines = [entry("OS", "Linux")];
        let mut config = Config::default();
        config.logo.source = "macos_small".into();
        assert_eq!(plain(&lines, &config, Some(30)), "OS: Linux\n");
        assert_eq!(plain(&lines, &config, Some(80)).lines().count(), 7);
    }
}
