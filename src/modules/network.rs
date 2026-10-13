use super::{Fields, Kind, Module, Request};
use crate::sys;

pub const MODULE: Module = Module {
    name: "network",
    kind: Kind::Info,
    label: "Network",
    format: "{ip}{? ({type})}",
    about: "Local IP address and the kind of connection it belongs to.",
    fields: &["ip", "type", "interface"],
    settings: "",
    threaded: true,
    fetch,
};

fn fetch(_: &Request) -> Fields {
    let Some(network) = sys::network() else {
        return Vec::new();
    };
    vec![
        ("ip", network.ip.to_string()),
        ("type", network.kind.to_string()),
        ("interface", network.interface),
    ]
}
