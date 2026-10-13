use super::{Fields, Kind, Module, Request};
use crate::{format, sys};

pub const MODULE: Module = Module {
    name: "uptime",
    kind: Kind::Info,
    label: "Uptime",
    format: "{pretty}",
    about: "Time since boot.",
    fields: &["pretty", "days", "hours", "minutes"],
    settings: "",
    threaded: false,
    fetch,
};

fn fetch(_: &Request) -> Fields {
    let Some(seconds) = sys::uptime() else {
        return Vec::new();
    };
    vec![
        ("pretty", format::uptime(seconds)),
        ("days", (seconds / 86_400).to_string()),
        ("hours", (seconds / 3600 % 24).to_string()),
        ("minutes", (seconds / 60 % 60).to_string()),
    ]
}
