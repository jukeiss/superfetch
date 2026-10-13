use std::env;

use super::{Fields, Kind, Module, Request};

pub const MODULE: Module = Module {
    name: "terminal",
    kind: Kind::Info,
    label: "Terminal",
    format: "{name}{? {version}}",
    about: "The terminal emulator.",
    fields: &["name", "version"],
    settings: "",
    threaded: false,
    fetch,
};

/// Friendlier names for `$TERM_PROGRAM` values.
const PROGRAMS: [(&str, &str); 6] = [
    ("Apple_Terminal", "Apple Terminal"),
    ("iTerm.app", "iTerm2"),
    ("vscode", "VS Code"),
    ("ghostty", "Ghostty"),
    ("WarpTerminal", "Warp"),
    ("rio", "Rio"),
];

/// Variables that terminals without `$TERM_PROGRAM` set for their children.
const MARKERS: [(&str, &str); 9] = [
    ("WT_SESSION", "Windows Terminal"),
    ("KITTY_WINDOW_ID", "kitty"),
    ("ALACRITTY_WINDOW_ID", "Alacritty"),
    ("WEZTERM_EXECUTABLE", "WezTerm"),
    ("KONSOLE_VERSION", "Konsole"),
    ("GNOME_TERMINAL_SCREEN", "GNOME Terminal"),
    ("TILIX_ID", "Tilix"),
    ("TERMINATOR_UUID", "Terminator"),
    ("TMUX", "tmux"),
];

fn fetch(_: &Request) -> Fields {
    let var = |name: &str| env::var(name).ok().filter(|value| !value.is_empty());

    // Terminals identify themselves through the environment, which makes
    // this free: no walking up the process tree.
    if let Some(program) = var("TERM_PROGRAM") {
        let name = PROGRAMS
            .iter()
            .find(|(id, _)| *id == program)
            .map_or(program, |(_, name)| name.to_string());
        let mut fields = vec![("name", name)];
        fields.extend(var("TERM_PROGRAM_VERSION").map(|version| ("version", version)));
        return fields;
    }
    if let Some((_, name)) = MARKERS.iter().find(|(marker, _)| var(marker).is_some()) {
        return vec![("name", name.to_string())];
    }
    let name = match var("TERM").as_deref() {
        Some("linux") => "Linux console".to_string(),
        Some(term) => term.trim_start_matches("xterm-").to_string(),
        None if cfg!(windows) => "Windows Console".to_string(),
        None => return Vec::new(),
    };
    vec![("name", name)]
}
