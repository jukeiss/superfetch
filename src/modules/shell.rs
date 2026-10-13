use std::env;
use std::path::Path;
use std::process::Command;

use super::{Fields, Kind, Module, Request};
use crate::sys;

pub const MODULE: Module = Module {
    name: "shell",
    kind: Kind::Info,
    label: "Shell",
    format: "{name}",
    about: "The shell superfetch was started from.",
    fields: &["name", "version"],
    settings: "# {version} is available too, but finding it out means running the shell,\n\
               # which costs a few milliseconds.\n",
    threaded: true,
    fetch,
};

const SHELLS: [&str; 16] = [
    "bash",
    "zsh",
    "fish",
    "sh",
    "dash",
    "ksh",
    "tcsh",
    "csh",
    "nu",
    "elvish",
    "xonsh",
    "ion",
    "osh",
    "pwsh",
    "powershell",
    "cmd",
];

fn fetch(req: &Request) -> Fields {
    let login_shell = env::var("SHELL").unwrap_or_default();
    let login_name = Path::new(&login_shell)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());

    // The parent process is the shell actually in use, unless superfetch was
    // started by something else (a script, `watch`), in which case the
    // login shell is the best answer. Login shells are named like "-zsh".
    let parent = sys::parent_process()
        .map(|name| name.trim_start_matches('-').to_string())
        .filter(|name| SHELLS.contains(&name.to_lowercase().as_str()));
    let Some(name) = parent.or(login_name.clone()) else {
        return Vec::new();
    };

    let mut fields = Vec::new();
    if req.wants("version") {
        let program = if login_name.as_deref() == Some(&name) {
            login_shell.as_str()
        } else {
            name.as_str()
        };
        if let Some(version) = version(program) {
            fields.push(("version", version));
        }
    }
    fields.push(("name", name));
    fields
}

fn version(program: &str) -> Option<String> {
    // Neither understands --version.
    if program.ends_with("cmd") || program.ends_with("powershell") {
        return None;
    }
    let output = Command::new(program).arg("--version").output().ok()?;
    parse_version(&String::from_utf8_lossy(&output.stdout))
}

/// Picks the version number out of a banner such as
/// `GNU bash, version 5.2.37(1)-release (aarch64-apple-darwin24.0.0)`.
fn parse_version(banner: &str) -> Option<String> {
    let word = banner
        .lines()
        .next()?
        .split([' ', ','])
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()))?;
    let end = word.find(['(', '-']).unwrap_or(word.len());
    Some(word[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn parses_version_banners() {
        let cases = [
            ("zsh 5.9 (arm64-apple-darwin24.0)", "5.9"),
            (
                "GNU bash, version 5.2.37(1)-release (x86_64-pc-linux-gnu)",
                "5.2.37",
            ),
            ("fish, version 3.7.1", "3.7.1"),
            ("PowerShell 7.4.6", "7.4.6"),
            ("0.99.1", "0.99.1"),
        ];
        for (banner, expected) in cases {
            assert_eq!(parse_version(banner).as_deref(), Some(expected));
        }
        assert_eq!(parse_version("no digits here"), None);
        assert_eq!(parse_version(""), None);
    }
}
