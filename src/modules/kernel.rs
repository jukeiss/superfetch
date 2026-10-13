use super::{Fields, Kind, Module, Request};
use crate::sys;

pub const MODULE: Module = Module {
    name: "kernel",
    kind: Kind::Info,
    label: "Kernel",
    format: "{name} {release}",
    about: "Kernel name and release.",
    fields: &["name", "release"],
    settings: "",
    threaded: false,
    fetch,
};

fn fetch(_: &Request) -> Fields {
    let kernel = sys::kernel();
    vec![("name", kernel.name), ("release", kernel.release)]
}
