use super::{Fields, Kind, Module, Request};
use crate::sys;

pub const MODULE: Module = Module {
    name: "os",
    kind: Kind::Info,
    label: "OS",
    format: "{pretty} {arch}",
    about: "Operating system name, version and architecture.",
    fields: &["pretty", "name", "version", "codename", "build", "arch"],
    settings: "",
    threaded: false,
    fetch,
};

fn fetch(_: &Request) -> Fields {
    let os = sys::os();
    vec![
        ("pretty", os.pretty),
        ("name", os.name),
        ("version", os.version),
        ("codename", os.codename),
        ("build", os.build),
        ("arch", os.arch),
    ]
}
