use super::{Fields, Kind, Module, Request};
use crate::sys;

pub const MODULE: Module = Module {
    name: "disk",
    kind: Kind::Info,
    label: "Disk",
    format: "{used} / {total} {bar} {percent}",
    about: "Space used on one filesystem.",
    fields: &["used", "total", "free", "percent", "bar", "path"],
    settings: "# The mount point to report on. Defaults to the system drive.\n# path = \"/\"\n",
    threaded: false,
    fetch,
};

fn fetch(req: &Request) -> Fields {
    let path = req.settings.path.clone().unwrap_or_else(sys::default_disk);
    let Some(usage) = sys::disk(&path) else {
        return Vec::new();
    };
    let mut fields = req.usage_fields(usage);
    fields.push(("path", path));
    fields
}
