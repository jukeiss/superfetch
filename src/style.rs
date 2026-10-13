//! Colors and text attributes, and how they are spelled in config files.

use std::fmt::Write;

pub const RESET: &str = "\x1b[0m";

const NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// One of the 16 colors the terminal theme defines: 0-7 are the normal
    /// variants, 8-15 the bright ones.
    Ansi(u8),
    /// An index into the 256-color palette.
    Fixed(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    /// Accepts a name (`red`, `bright_blue`, `gray`), a palette index
    /// (`0`-`255`) or a hex triplet (`#ff8800`).
    pub fn parse(s: &str) -> Option<Color> {
        let s = s.trim().to_ascii_lowercase();
        if let Some(hex) = s.strip_prefix('#') {
            if hex.len() != 6 || !hex.is_ascii() {
                return None;
            }
            let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
            return Some(Color::Rgb(channel(0)?, channel(2)?, channel(4)?));
        }
        if let Ok(index) = s.parse::<u8>() {
            return Some(Color::Fixed(index));
        }
        if s == "gray" || s == "grey" {
            return Some(Color::Ansi(8));
        }
        let (name, offset) = match s.strip_prefix("bright") {
            Some(rest) => (rest.trim_start_matches(['_', '-']), 8),
            None => (s.as_str(), 0),
        };
        let index = NAMES.iter().position(|n| *n == name)?;
        Some(Color::Ansi(index as u8 + offset))
    }

    /// The names offered by the interactive editor.
    pub fn names() -> impl Iterator<Item = String> {
        NAMES
            .iter()
            .map(|n| n.to_string())
            .chain(NAMES.iter().map(|n| format!("bright_{n}")))
    }

    fn write_sgr(self, background: bool, out: &mut String) {
        let base = if background { 40 } else { 30 };
        let _ = match self {
            Color::Ansi(n @ 0..=7) => write!(out, "{}", base + n),
            Color::Ansi(n) => write!(out, "{}", base + 60 + (n & 7)),
            Color::Fixed(n) => write!(out, "{};5;{n}", base + 8),
            Color::Rgb(r, g, b) => write!(out, "{};2;{r};{g};{b}", base + 8),
        };
    }

    /// Escape sequence that switches the background to this color.
    pub fn background(self) -> String {
        let mut out = String::from("\x1b[");
        self.write_sgr(true, &mut out);
        out.push('m');
        out
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Style {
    pub fn fg(color: Color) -> Style {
        Style {
            fg: Some(color),
            ..Style::default()
        }
    }

    /// Parses a space-separated spec such as `"bold cyan"` or
    /// `"#ff8800 italic"`. The word `accent` resolves to `accent`.
    pub fn parse(spec: &str, accent: Option<Color>) -> Result<Style, String> {
        let mut style = Style::default();
        for word in spec.split_whitespace() {
            match word.to_ascii_lowercase().as_str() {
                "bold" => style.bold = true,
                "dim" => style.dim = true,
                "italic" => style.italic = true,
                "underline" => style.underline = true,
                "default" | "none" => style.fg = None,
                "accent" => style.fg = accent,
                other => {
                    style.fg = Some(
                        Color::parse(other)
                            .ok_or_else(|| format!("unknown color or attribute '{word}'"))?,
                    )
                }
            }
        }
        Ok(style)
    }

    pub fn is_plain(&self) -> bool {
        *self == Style::default()
    }

    /// The escape sequence that turns this style on; empty for a plain style.
    pub fn sgr(&self) -> String {
        if self.is_plain() {
            return String::new();
        }
        let mut out = String::from("\x1b[");
        for (enabled, code) in [
            (self.bold, "1;"),
            (self.dim, "2;"),
            (self.italic, "3;"),
            (self.underline, "4;"),
        ] {
            if enabled {
                out.push_str(code);
            }
        }
        match self.fg {
            Some(color) => color.write_sgr(false, &mut out),
            None => {
                out.pop();
            }
        }
        out.push('m');
        out
    }

    pub fn paint(&self, text: &str) -> String {
        if self.is_plain() || text.is_empty() {
            return text.to_string();
        }
        format!("{}{text}{RESET}", self.sgr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_color_spellings() {
        assert_eq!(Color::parse("red"), Some(Color::Ansi(1)));
        assert_eq!(Color::parse("Bright_Blue"), Some(Color::Ansi(12)));
        assert_eq!(Color::parse("bright-cyan"), Some(Color::Ansi(14)));
        assert_eq!(Color::parse("grey"), Some(Color::Ansi(8)));
        assert_eq!(Color::parse("208"), Some(Color::Fixed(208)));
        assert_eq!(Color::parse("#ff8800"), Some(Color::Rgb(255, 136, 0)));
        assert_eq!(Color::parse("#ff88"), None);
        assert_eq!(Color::parse("mauve"), None);
    }

    #[test]
    fn builds_escape_sequences() {
        let accent = Some(Color::Ansi(4));
        assert_eq!(
            Style::parse("bold accent", accent).unwrap().sgr(),
            "\x1b[1;34m"
        );
        assert_eq!(Style::parse("bright_red", None).unwrap().sgr(), "\x1b[91m");
        assert_eq!(Style::parse("dim", None).unwrap().sgr(), "\x1b[2m");
        assert_eq!(
            Style::parse("#010203", None).unwrap().sgr(),
            "\x1b[38;2;1;2;3m"
        );
        assert_eq!(Style::parse("default", None).unwrap().sgr(), "");
        assert!(Style::parse("bold sparkly", None).is_err());
    }

    #[test]
    fn plain_style_adds_nothing() {
        assert_eq!(Style::default().paint("text"), "text");
        assert_eq!(Style::fg(Color::Ansi(2)).paint("ok"), "\x1b[32mok\x1b[0m");
    }
}
