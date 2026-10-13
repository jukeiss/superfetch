use super::{Fields, Kind, Module, Request};
use crate::sys;

pub const MODULE: Module = Module {
    name: "title",
    kind: Kind::Info,
    label: "",
    format: "{#title}{user}{#}@{#title}{host}{#}",
    about: "The user and host name.",
    fields: &["user", "host", "fqdn"],
    settings: "",
    threaded: false,
    fetch,
};

fn fetch(_: &Request) -> Fields {
    let fqdn = sys::hostname();
    // "mac.local" and "web1.example.com" are both better known by their
    // first label.
    let host = fqdn.split('.').next().unwrap_or_default().to_string();
    vec![("user", sys::username()), ("host", host), ("fqdn", fqdn)]
}
