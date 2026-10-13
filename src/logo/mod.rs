//! ASCII logos: the built-in set and files supplied by the user.
//!
//! Art is plain text. `$1` to `$9` switch to that entry of the logo's color
//! list and `$$` is a literal dollar sign. neofetch's `${c1}` spelling is
//! accepted too, so its art files can be used unchanged.
//!
//! To add a built-in logo, drop the art into `art/` and add an entry to
//! `BUILTINS`.

use std::fs;
use std::path::PathBuf;

use crate::config::{self, LogoConfig};
use crate::style::{Color, RESET, Style};
use crate::{sys, text};

struct Builtin {
    name: &'static str,
    /// OS identifiers this logo is the automatic choice for: `ID` and
    /// `ID_LIKE` values from os-release, or `macos` / `windows`.
    ids: &'static [&'static str],
    art: &'static str,
    colors: &'static [&'static str],
    /// Zero-based indices into `colors` for the title and for the labels.
    accents: [usize; 2],
}

macro_rules! art {
    ($name:literal) => {
        include_str!(concat!("art/", $name, ".txt"))
    };
}

#[rustfmt::skip]
const BUILTINS: &[Builtin] = &[
    Builtin { name: "macos", ids: &["macos"], art: art!("macos"), colors: &["green", "yellow", "bright_red", "red", "magenta", "blue"], accents: [0, 1] },
    Builtin { name: "macos_small", ids: &[], art: art!("macos_small"), colors: &["green", "yellow", "red", "magenta", "blue"], accents: [0, 1] },
    Builtin { name: "windows", ids: &["windows"], art: art!("windows"), colors: &["blue"], accents: [0, 0] },
    Builtin { name: "linux", ids: &["linux"], art: art!("linux"), colors: &["white", "bright_black", "yellow"], accents: [2, 2] },
    Builtin { name: "alpine", ids: &["alpine"], art: art!("alpine"), colors: &["blue"], accents: [0, 0] },
    Builtin { name: "android", ids: &["android"], art: art!("android"), colors: &["green", "white"], accents: [0, 0] },
    Builtin { name: "arch", ids: &["arch", "archarm"], art: art!("arch"), colors: &["cyan"], accents: [0, 0] },
    Builtin { name: "cachyos", ids: &["cachyos"], art: art!("cachyos"), colors: &["green", "black", "cyan"], accents: [2, 0] },
    Builtin { name: "debian", ids: &["debian"], art: art!("debian"), colors: &["red", "white"], accents: [0, 0] },
    Builtin { name: "endeavouros", ids: &["endeavouros"], art: art!("endeavouros"), colors: &["magenta", "red", "blue"], accents: [0, 2] },
    Builtin { name: "fedora", ids: &["fedora"], art: art!("fedora"), colors: &["blue", "white"], accents: [0, 0] },
    Builtin { name: "freebsd", ids: &["freebsd"], art: art!("freebsd"), colors: &["white", "red"], accents: [1, 1] },
    Builtin { name: "gentoo", ids: &["gentoo"], art: art!("gentoo"), colors: &["magenta", "white"], accents: [0, 0] },
    Builtin { name: "kali", ids: &["kali"], art: art!("kali"), colors: &["blue", "bright_black"], accents: [0, 0] },
    Builtin { name: "manjaro", ids: &["manjaro", "manjaro-arm"], art: art!("manjaro"), colors: &["green"], accents: [0, 0] },
    Builtin { name: "mint", ids: &["linuxmint"], art: art!("mint"), colors: &["green", "white"], accents: [0, 0] },
    Builtin { name: "nixos", ids: &["nixos"], art: art!("nixos"), colors: &["blue", "cyan"], accents: [0, 1] },
    Builtin { name: "opensuse", ids: &["opensuse", "opensuse-tumbleweed", "opensuse-leap", "suse"], art: art!("opensuse"), colors: &["green"], accents: [0, 0] },
    Builtin { name: "pop", ids: &["pop"], art: art!("pop"), colors: &["cyan", "white"], accents: [0, 0] },
    Builtin { name: "raspbian", ids: &["raspbian"], art: art!("raspbian"), colors: &["red", "green"], accents: [0, 1] },
    Builtin { name: "rocky", ids: &["rocky"], art: art!("rocky"), colors: &["green"], accents: [0, 0] },
    Builtin { name: "ubuntu", ids: &["ubuntu"], art: art!("ubuntu"), colors: &["red"], accents: [0, 0] },
    Builtin { name: "void", ids: &["void"], art: art!("void"), colors: &["green", "bright_black"], accents: [0, 0] },
];

/// A run of text in one color; `None` means the terminal's default.
type Span = (Option<Color>, String);

pub struct Logo {
    lines: Vec<Vec<Span>>,
    /// Columns of the widest line.
    pub width: usize,
    /// Colors the theme uses for the title and the labels when
    /// `theme.accent` is "auto".
    pub accents: [Option<Color>; 2],
    bold: bool,
}

/// Names of the built-in logos.
pub fn names() -> impl Iterator<Item = &'static str> {
    BUILTINS.iter().map(|b| b.name)
}

impl Logo {
    /// Resolves `logo.source`. Returns `None` when no logo should be drawn.
    pub fn load(config: &LogoConfig) -> Option<Logo> {
        let overrides = parse_colors(&config.colors);
        match config.source.as_str() {
            "none" | "off" | "" => None,
            "auto" => Some(Logo::builtin(detect(), &overrides, config.bold)),
            source => {
                if let Some(builtin) = BUILTINS.iter().find(|b| b.name == source) {
                    return Some(Logo::builtin(builtin, &overrides, config.bold));
                }
                match fs::read_to_string(expand_home(source)) {
                    Ok(art) => {
                        let accents = [0, usize::from(overrides.len() > 1)];
                        Some(Logo::parse(&art, &overrides, accents, config.bold))
                    }
                    Err(err) => {
                        crate::warn(&format!(
                            "logo '{source}' is neither a built-in logo nor a readable file ({err})"
                        ));
                        Some(Logo::builtin(detect(), &overrides, config.bold))
                    }
                }
            }
        }
    }

    fn builtin(builtin: &Builtin, overrides: &[Color], bold: bool) -> Logo {
        let mut colors = parse_colors(builtin.colors);
        for (slot, color) in colors.iter_mut().zip(overrides) {
            *slot = *color;
        }
        Logo::parse(builtin.art, &colors, builtin.accents, bold)
    }

    fn parse(art: &str, colors: &[Color], accents: [usize; 2], bold: bool) -> Logo {
        let mut current = colors.first().copied();
        let mut lines = Vec::new();
        for line in art.trim_end().lines() {
            let mut spans: Vec<Span> = Vec::new();
            let mut text = String::new();
            let mut rest = line;
            while let Some(c) = rest.chars().next() {
                let (marker, tail) = match split_marker(rest) {
                    Some(found) => found,
                    None => {
                        text.push(c);
                        rest = &rest[c.len_utf8()..];
                        continue;
                    }
                };
                rest = tail;
                match marker {
                    None => text.push('$'),
                    Some(index) => {
                        if !text.is_empty() {
                            spans.push((current, std::mem::take(&mut text)));
                        }
                        current = colors.get(index - 1).copied();
                    }
                }
            }
            if !text.is_empty() {
                spans.push((current, text));
            }
            lines.push(spans);
        }

        let width = lines
            .iter()
            .map(|spans| span_width(spans))
            .max()
            .unwrap_or(0);
        Logo {
            lines,
            width,
            accents: accents.map(|index| colors.get(index).copied()),
            bold,
        }
    }

    pub fn height(&self) -> usize {
        self.lines.len()
    }

    /// Appends line `index` to `out`, padded with spaces to the logo's full
    /// width. Lines past the end of the art are all padding.
    pub fn write_line(&self, index: usize, color: bool, out: &mut String) {
        let Some(spans) = self.lines.get(index) else {
            text::pad(out, self.width);
            return;
        };
        for (i, (fg, span)) in spans.iter().enumerate() {
            if color {
                let style = Style {
                    fg: *fg,
                    bold: self.bold,
                    ..Style::default()
                };
                // Going back to the default color needs a reset; setting a
                // new one simply replaces the old.
                if fg.is_none() && i > 0 {
                    out.push_str(RESET);
                }
                out.push_str(&style.sgr());
            }
            out.push_str(span);
        }
        if color {
            out.push_str(RESET);
        }
        text::pad(out, self.width - span_width(spans));
    }
}

fn span_width(spans: &[Span]) -> usize {
    spans.iter().map(|(_, span)| text::width(span)).sum()
}

/// Recognises a color marker at the start of `s`. Returns the color number
/// (`None` for the `$$` escape) and the text after the marker.
fn split_marker(s: &str) -> Option<(Option<usize>, &str)> {
    let rest = s.strip_prefix('$')?;
    if let Some(tail) = rest.strip_prefix('$') {
        return Some((None, tail));
    }
    let digit = |c: char| c.to_digit(10).filter(|d| *d > 0).map(|d| d as usize);
    if let Some(index) = rest.chars().next().and_then(digit) {
        return Some((Some(index), &rest[1..]));
    }
    let (inner, tail) = rest.strip_prefix("{c")?.split_once('}')?;
    Some((Some(inner.parse().ok().filter(|i| *i > 0)?), tail))
}

fn parse_colors<S: AsRef<str>>(names: &[S]) -> Vec<Color> {
    names
        .iter()
        .filter_map(|name| {
            let name = name.as_ref();
            Color::parse(name).or_else(|| {
                crate::warn(&format!("logo.colors: unknown color '{name}'"));
                None
            })
        })
        .collect()
}

/// The logo for the running OS, or the generic one for its family.
fn detect() -> &'static Builtin {
    let ids = sys::os_ids();
    ids.iter()
        .find_map(|id| BUILTINS.iter().find(|b| b.ids.contains(&id.as_str())))
        .unwrap_or_else(|| {
            BUILTINS
                .iter()
                .find(|b| b.name == "linux")
                .expect("the generic logo is built in")
        })
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), config::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(logo: &Logo, color: bool) -> Vec<String> {
        (0..logo.height())
            .map(|i| {
                let mut line = String::new();
                logo.write_line(i, color, &mut line);
                line
            })
            .collect()
    }

    #[test]
    fn parses_markers() {
        let colors = [Color::Ansi(1), Color::Ansi(2)];
        let logo = Logo::parse("ab$2cd\n${c1}e $$5\n", &colors, [0, 1], false);
        assert_eq!(logo.width, 4);
        assert_eq!(logo.accents, [Some(Color::Ansi(1)), Some(Color::Ansi(2))]);
        assert_eq!(render(&logo, false), ["abcd", "e $5"]);
        assert_eq!(
            render(&logo, true),
            ["\x1b[31mab\x1b[32mcd\x1b[0m", "\x1b[31me $5\x1b[0m"]
        );
    }

    #[test]
    fn pads_short_and_missing_lines() {
        let logo = Logo::parse("abcd\nx\n", &[], [0, 0], false);
        let mut line = String::new();
        logo.write_line(1, false, &mut line);
        assert_eq!(line, "x   ");
        line.clear();
        logo.write_line(7, false, &mut line);
        assert_eq!(line, "    ");
    }

    #[test]
    fn builtin_art_is_well_formed() {
        for builtin in BUILTINS {
            let colors = parse_colors(builtin.colors);
            assert_eq!(colors.len(), builtin.colors.len(), "{}", builtin.name);
            let logo = Logo::parse(builtin.art, &colors, builtin.accents, true);
            assert!(logo.height() > 0 && logo.width > 0, "{}", builtin.name);
            assert!(logo.accents.iter().all(Option::is_some), "{}", builtin.name);
            let uses_known_colors = logo.lines.iter().flatten().all(|(fg, _)| fg.is_some());
            assert!(
                uses_known_colors,
                "{} uses an undefined color",
                builtin.name
            );
        }
    }
}
