//! The entries of the module list that arrange output rather than report
//! anything: the separator, the blank line and the palette.

use super::{Fields, Kind, Line, Module, Request};
use crate::style::{Color, RESET};
use crate::theme::Theme;

pub const SEPARATOR_SYMBOL: &str = "─";
pub const COLORS_SYMBOL: &str = "   ";

pub const SEPARATOR: Module = Module {
    name: "separator",
    kind: Kind::Separator,
    label: "",
    format: "",
    about: "A rule as wide as the line above it.",
    fields: &[],
    settings: "symbol = \"─\"\n",
    threaded: false,
    fetch: nothing,
};

pub const BREAK: Module = Module {
    name: "break",
    kind: Kind::Break,
    label: "",
    format: "",
    about: "An empty line.",
    fields: &[],
    settings: "",
    threaded: false,
    fetch: nothing,
};

pub const COLORS: Module = Module {
    name: "colors",
    kind: Kind::Colors,
    label: "",
    format: "",
    about: "The terminal's 16-color palette. Hidden when color is off.",
    fields: &[],
    settings: "# Blank symbols are drawn as blocks; anything else, such as \"● \", is drawn\n\
               # in each color.\n\
               symbol = \"   \"\n",
    threaded: false,
    fetch: nothing,
};

fn nothing(_: &Request) -> Fields {
    Vec::new()
}

/// Two rows of swatches: the normal colors, then the bright ones.
pub fn palette(symbol: &str, theme: &Theme) -> Vec<Line> {
    if !theme.color || symbol.is_empty() {
        return Vec::new();
    }
    let blocks = symbol.trim().is_empty();
    [0..8u8, 8..16]
        .into_iter()
        .map(|row| {
            let mut line = String::new();
            for index in row {
                let color = Color::Ansi(index);
                line.push_str(&if blocks {
                    color.background()
                } else {
                    crate::style::Style::fg(color).sgr()
                });
                line.push_str(symbol);
            }
            line.push_str(RESET);
            Line::Text(line)
        })
        .collect()
}
