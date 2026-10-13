//! Modules defined in the config file rather than in code.

use std::process::Command;

use super::{Fields, Request};

/// Runs the module's `command`, if it has one, and returns the first line
/// it prints as `{output}`. A custom module without a command is just its
/// format string, which can still use `{env:NAME}`.
pub fn fetch(req: &Request) -> Fields {
    let Some(command) = req.settings.command.as_deref() else {
        return Vec::new();
    };
    if !req.wants("output") {
        return Vec::new();
    }
    let (shell, flag) = if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    let Ok(output) = Command::new(shell).args([flag, command]).output() else {
        return Vec::new();
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    match stdout.lines().map(str::trim).find(|line| !line.is_empty()) {
        Some(line) => vec![("output", line.to_string())],
        None => Vec::new(),
    }
}
