use super::{Fields, Kind, Module, Request};
use crate::sys;

pub const MODULE: Module = Module {
    name: "swap",
    kind: Kind::Info,
    label: "Swap",
    format: "{used} / {total} {bar} {percent}",
    about: "Swap space in use. Hidden on systems without swap.",
    fields: &["used", "total", "free", "percent", "bar"],
    settings: "",
    threaded: false,
    fetch,
};

fn fetch(req: &Request) -> Fields {
    sys::swap().map_or_else(Vec::new, |usage| req.usage_fields(usage))
}
