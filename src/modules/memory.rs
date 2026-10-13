use super::{Fields, Kind, Module, Request};
use crate::sys;

pub const MODULE: Module = Module {
    name: "memory",
    kind: Kind::Info,
    label: "Memory",
    format: "{used} / {total} {bar} {percent}",
    about: "RAM in use.",
    fields: &["used", "total", "free", "percent", "bar"],
    settings: "",
    threaded: false,
    fetch,
};

fn fetch(req: &Request) -> Fields {
    sys::memory().map_or_else(Vec::new, |usage| req.usage_fields(usage))
}
