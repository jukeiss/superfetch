//! Linux: everything comes from procfs and sysfs, read as plain text.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{fs, os::unix::process::parent_id};

use super::{CpuTicks, Host, Kernel, Network, Os, Usage, unix};

pub use unix::{cpu_cores, default_disk, disk, hostname, load_average, username};

/// Reads a file and trims it; `None` if it is missing or blank.
fn read(path: impl AsRef<Path>) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    (!text.is_empty()).then(|| text.to_string())
}

/// The value of `key` in a file of `key<separator>value` lines.
fn lookup<'a>(text: &'a str, key: &str, separator: char) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once(separator)?;
        (name.trim() == key).then(|| value.trim())
    })
}

/// `/etc/os-release`, read once: both the OS module and logo detection
/// need it.
fn os_release() -> &'static str {
    static CONTENTS: OnceLock<String> = OnceLock::new();
    CONTENTS.get_or_init(|| {
        fs::read_to_string("/etc/os-release")
            .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
            .unwrap_or_default()
    })
}

fn os_release_value(key: &str) -> String {
    lookup(os_release(), key, '=')
        .map(|value| value.trim_matches(['"', '\'']).to_string())
        .unwrap_or_default()
}

pub fn os() -> Os {
    let name = Some(os_release_value("NAME"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Linux".into());
    let version = os_release_value("VERSION_ID");
    let pretty = Some(os_release_value("PRETTY_NAME"))
        .filter(|pretty| !pretty.is_empty())
        .unwrap_or_else(|| format!("{name} {version}").trim().to_string());
    Os {
        name,
        version,
        codename: os_release_value("VERSION_CODENAME"),
        build: os_release_value("BUILD_ID"),
        pretty,
        arch: unix::uname().map(|u| u.machine).unwrap_or_default(),
    }
}

/// The distribution's `ID`, then the distributions it derives from, then
/// the generic fallback.
pub fn os_ids() -> Vec<String> {
    let mut ids = vec![os_release_value("ID")];
    ids.extend(
        os_release_value("ID_LIKE")
            .split_whitespace()
            .map(String::from),
    );
    ids.push("linux".into());
    ids.retain(|id| !id.is_empty());
    ids
}

pub fn kernel() -> Kernel {
    let uname = unix::uname();
    Kernel {
        name: uname.as_ref().map_or("Linux".into(), |u| u.sysname.clone()),
        release: uname.map(|u| u.release).unwrap_or_default(),
    }
}

pub fn host() -> Option<Host> {
    // Boards without a real name ship one of these in the DMI tables.
    const PLACEHOLDERS: [&str; 9] = [
        "to be filled by o.e.m.",
        "default string",
        "system product name",
        "system version",
        "system manufacturer",
        "not specified",
        "not applicable",
        "none",
        "unknown",
    ];
    let dmi = |field: &str| {
        read(format!("/sys/devices/virtual/dmi/id/{field}"))
            .filter(|value| !PLACEHOLDERS.contains(&value.to_lowercase().as_str()))
    };

    let vendor = dmi("sys_vendor").unwrap_or_default();
    // Lenovo puts a machine type code in the name and the name people know
    // ("ThinkPad X1 Carbon Gen 9") in the version.
    let friendly = if vendor.eq_ignore_ascii_case("lenovo") {
        dmi("product_version")
    } else {
        None
    };
    let model = friendly
        .or_else(|| dmi("product_name"))
        .or_else(|| dmi("board_name"))
        // ARM boards such as the Raspberry Pi describe themselves here.
        .or_else(|| read("/sys/firmware/devicetree/base/model"))?;
    Some(Host { model, vendor })
}

pub fn uptime() -> Option<u64> {
    let text = read("/proc/uptime")?;
    let seconds: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some(seconds as u64)
}

pub fn parent_process() -> Option<String> {
    read(format!("/proc/{}/comm", parent_id()))
}

pub fn cpu_model() -> Option<String> {
    let info = fs::read_to_string("/proc/cpuinfo").ok()?;
    // x86 has "model name"; the other keys cover ARM, MIPS and POWER.
    let raw = ["model name", "Hardware", "cpu model", "cpu"]
        .iter()
        .find_map(|key| lookup(&info, key, ':'))?;
    Some(tidy_cpu_name(raw))
}

/// Drops the marketing noise from a CPU name, and the clock speed, which
/// is reported separately.
fn tidy_cpu_name(raw: &str) -> String {
    let name = raw.split(" @ ").next().unwrap_or(raw);
    let mut name = name.to_string();
    for noise in ["(R)", "(r)", "(TM)", "(tm)", " CPU", " Processor"] {
        name = name.replace(noise, "");
    }
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn cpu_mhz() -> Option<u32> {
    if let Some(khz) = read("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")
        .and_then(|text| text.parse::<u32>().ok())
    {
        return Some(khz / 1000);
    }
    let info = fs::read_to_string("/proc/cpuinfo").ok()?;
    let mhz: f64 = lookup(&info, "cpu MHz", ':')?.parse().ok()?;
    Some(mhz as u32)
}

pub fn cpu_ticks() -> Option<CpuTicks> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    // cpu  user nice system idle iowait irq softirq steal ...
    let fields: Vec<u64> = stat
        .lines()
        .next()?
        .split_whitespace()
        .skip(1)
        .take(8)
        .filter_map(|field| field.parse().ok())
        .collect();
    if fields.len() < 5 {
        return None;
    }
    let total: u64 = fields.iter().sum();
    let idle = fields[3] + fields[4];
    Some(CpuTicks {
        busy: total - idle,
        total,
    })
}

pub fn gpu() -> Option<String> {
    let mut cards: Vec<PathBuf> = fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            // "card0", but not its connectors ("card0-HDMI-A-1").
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix("card"))
                .is_some_and(|index| index.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(|path| path.join("device"))
        .collect();
    cards.sort();

    // With several cards, prefer the one the firmware booted the display on.
    let device = cards
        .iter()
        .find(|device| read(device.join("boot_vga")).as_deref() == Some("1"))
        .or(cards.first())?;

    let id = |file: &str| {
        let text = read(device.join(file))?;
        u16::from_str_radix(text.trim_start_matches("0x"), 16).ok()
    };
    match (id("vendor"), id("device")) {
        (Some(vendor), Some(model)) => Some(pci_name(vendor, model)),
        // Not a PCI device (most ARM boards): the driver name is all there is.
        _ => lookup(&read(device.join("uevent"))?, "DRIVER", '=').map(String::from),
    }
}

/// Names a PCI device using the system's copy of the PCI ID database.
fn pci_name(vendor: u16, device: u16) -> String {
    let short_vendor = match vendor {
        0x10de => Some("NVIDIA"),
        0x1002 | 0x1022 => Some("AMD"),
        0x8086 => Some("Intel"),
        _ => None,
    };
    let database = [
        "/usr/share/hwdata/pci.ids",
        "/usr/share/misc/pci.ids",
        "/usr/share/pci.ids",
    ]
    .iter()
    .find_map(|path| fs::read(path).ok())
    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    .unwrap_or_default();

    let (vendor_name, device_name) = search_pci_ids(&database, vendor, device);
    let vendor_name = short_vendor
        .map(String::from)
        .or(vendor_name)
        .unwrap_or_else(|| format!("{vendor:04x}"));
    match device_name {
        Some(name) => format!("{vendor_name} {name}"),
        None => format!("{vendor_name} {device:04x}"),
    }
}

/// Finds a vendor and one of its devices in `pci.ids`:
///
/// ```text
/// 10de  NVIDIA Corporation
/// <tab>2184  TU116 [GeForce GTX 1660]
/// ```
fn search_pci_ids(database: &str, vendor: u16, device: u16) -> (Option<String>, Option<String>) {
    let vendor_key = format!("{vendor:04x}  ");
    let device_key = format!("\t{device:04x}  ");
    let mut lines = database.lines();
    let Some(vendor_name) = lines.find_map(|line| line.strip_prefix(&vendor_key)) else {
        return (None, None);
    };
    let device_name = lines
        .take_while(|line| line.starts_with(['\t', '#']) || line.is_empty())
        .find_map(|line| line.strip_prefix(&device_key))
        .map(|name| {
            // "TU116 [GeForce GTX 1660]": the bracket holds the product
            // name, the rest is the chip's code name.
            match name
                .rsplit_once('[')
                .and_then(|(_, tail)| tail.split_once(']'))
            {
                Some((product, _)) => product.to_string(),
                None => name.to_string(),
            }
        });
    (Some(vendor_name.to_string()), device_name)
}

/// A value from `/proc/meminfo`, converted from kB to bytes.
fn meminfo(key: &str) -> Option<u64> {
    static CONTENTS: OnceLock<String> = OnceLock::new();
    let text = CONTENTS.get_or_init(|| fs::read_to_string("/proc/meminfo").unwrap_or_default());
    let kilobytes: u64 = lookup(text, key, ':')?
        .strip_suffix("kB")?
        .trim()
        .parse()
        .ok()?;
    Some(kilobytes * 1024)
}

pub fn memory() -> Option<Usage> {
    let total = meminfo("MemTotal")?;
    // MemAvailable is the kernel's own estimate of what can be claimed
    // without swapping, so its complement is what is really in use.
    let available = meminfo("MemAvailable")?;
    Some(Usage {
        used: total.saturating_sub(available),
        total,
    })
}

pub fn swap() -> Option<Usage> {
    let total = meminfo("SwapTotal")?;
    Some(Usage {
        used: total.saturating_sub(meminfo("SwapFree")?),
        total,
    })
}

pub fn interface(ip: Option<Ipv4Addr>) -> Option<Network> {
    let (ip, interface) = unix::find_interface(ip)?;
    let kind = interface_kind(&interface);
    Some(Network {
        ip,
        interface,
        kind,
    })
}

fn interface_kind(name: &str) -> &'static str {
    let sysfs = Path::new("/sys/class/net").join(name);
    if sysfs.join("wireless").exists() || sysfs.join("phy80211").exists() {
        "Wi-Fi"
    } else if sysfs.join("tun_flags").exists() || name.starts_with("wg") || name.starts_with("ppp")
    {
        "VPN"
    } else if name.starts_with("ww") {
        "Cellular"
    } else if !sysfs.join("device").exists() {
        // Bridges, veth pairs and the like have no hardware behind them.
        "Virtual"
    } else if read(sysfs.join("type")).as_deref() == Some("1") {
        "Ethernet"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidies_cpu_names() {
        assert_eq!(
            tidy_cpu_name("Intel(R) Core(TM) i7-9700K CPU @ 3.60GHz"),
            "Intel Core i7-9700K"
        );
        assert_eq!(
            tidy_cpu_name("AMD Ryzen 7 5800X 8-Core Processor"),
            "AMD Ryzen 7 5800X 8-Core"
        );
    }

    #[test]
    fn searches_pci_ids() {
        let database = "\
# comment
1002  Advanced Micro Devices, Inc. [AMD/ATI]
\t73ff  Navi 23 [Radeon RX 6600/6600 XT/6600M]
10de  NVIDIA Corporation
\t2184  TU116 [GeForce GTX 1660]
\t\t1043 8706  subsystem
\t2204  GA102 [GeForce RTX 3090]
8086  Intel Corporation
\t3e9b  UHD Graphics 630
";
        let name = |v, d| search_pci_ids(database, v, d).1;
        assert_eq!(name(0x10de, 0x2204).as_deref(), Some("GeForce RTX 3090"));
        assert_eq!(name(0x8086, 0x3e9b).as_deref(), Some("UHD Graphics 630"));
        // A device ID that only exists under another vendor must not match.
        assert_eq!(name(0x1002, 0x2184), None);
        assert_eq!(search_pci_ids(database, 0xffff, 0x0001), (None, None));
    }

    #[test]
    fn looks_up_keys() {
        let text = "MemTotal:       16314572 kB\nMemFree:  100 kB\n";
        assert_eq!(lookup(text, "MemTotal", ':'), Some("16314572 kB"));
        assert_eq!(
            lookup("ID=arch\nNAME=\"Arch Linux\"", "NAME", '='),
            Some("\"Arch Linux\"")
        );
        assert_eq!(lookup(text, "SwapTotal", ':'), None);
    }
}
