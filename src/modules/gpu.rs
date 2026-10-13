use super::{Fields, Kind, Module, Request};
use crate::sys;

pub const MODULE: Module = Module {
    name: "gpu",
    kind: Kind::Info,
    label: "GPU",
    format: "{name}",
    about: "The graphics card driving the display.",
    fields: &["name"],
    settings: "",
    threaded: true,
    fetch,
};

fn fetch(_: &Request) -> Fields {
    sys::gpu().map(|name| ("name", name)).into_iter().collect()
}
