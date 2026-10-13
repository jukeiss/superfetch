use super::{Fields, Kind, Module, Request};
use crate::sys;

pub const MODULE: Module = Module {
    name: "host",
    kind: Kind::Info,
    label: "Host",
    format: "{model}",
    about: "Model of the machine or its motherboard.",
    fields: &["model", "vendor"],
    settings: "",
    // On macOS this is a registry lookup, which is not free.
    threaded: cfg!(target_os = "macos"),
    fetch,
};

fn fetch(_: &Request) -> Fields {
    match sys::host() {
        Some(host) => vec![("model", host.model), ("vendor", host.vendor)],
        None => Vec::new(),
    }
}
