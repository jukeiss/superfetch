//! macOS: sysctl for most things, Mach for memory and CPU counters, and the
//! I/O Registry for hardware names.

mod iokit;

use std::ffi::{CStr, c_void};
use std::mem::{MaybeUninit, size_of};
use std::net::{Ipv4Addr, UdpSocket};
use std::os::fd::AsRawFd;
use std::os::unix::process::parent_id;
use std::ptr;
use std::time::{SystemTime, UNIX_EPOCH};

use super::c::{self, c_char, c_int};
use super::{CpuTicks, Host, Kernel, Network, Os, Usage, unix};

pub use unix::{cpu_cores, default_disk, disk, hostname, load_average, username};

const HOST_CPU_LOAD_INFO: c_int = 3;
const HOST_VM_INFO64: c_int = 4;

unsafe extern "C" {
    fn sysctlbyname(
        name: *const c_char,
        old: *mut c_void,
        old_size: *mut usize,
        new: *mut c_void,
        new_size: usize,
    ) -> c_int;
    fn proc_name(pid: c_int, buffer: *mut c_void, size: u32) -> c_int;
    fn getpagesize() -> c_int;
    fn mach_host_self() -> u32;
    fn host_statistics(host: u32, flavor: c_int, info: *mut c_int, count: *mut u32) -> c_int;
    fn host_statistics64(host: u32, flavor: c_int, info: *mut c_int, count: *mut u32) -> c_int;
}

/// `struct timeval`, the type of `kern.boottime`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Timeval {
    seconds: i64,
    microseconds: i32,
}

/// `struct xsw_usage`, the type of `vm.swapusage`.
#[repr(C)]
#[derive(Clone, Copy)]
struct SwapUsage {
    total: u64,
    available: u64,
    used: u64,
    page_size: u32,
    encrypted: u32,
}

/// `struct vm_statistics64` from `<mach/vm_statistics.h>`. Counts are in
/// pages.
#[repr(C)]
#[derive(Clone, Copy)]
struct VmStatistics {
    free: u32,
    active: u32,
    inactive: u32,
    wired: u32,
    zero_fill: u64,
    reactivations: u64,
    pageins: u64,
    pageouts: u64,
    faults: u64,
    cow_faults: u64,
    lookups: u64,
    hits: u64,
    purges: u64,
    purgeable: u32,
    speculative: u32,
    decompressions: u64,
    compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor: u32,
    throttled: u32,
    /// File-backed pages: the cache.
    external: u32,
    internal: u32,
    uncompressed_in_compressor: u64,
}

/// Reads a fixed-size sysctl value.
fn sysctl<T: Copy>(name: &CStr) -> Option<T> {
    let mut value = MaybeUninit::<T>::zeroed();
    let mut size = size_of::<T>();
    // SAFETY: the kernel writes at most `size` bytes, and the value is only
    // used if it wrote exactly a `T`.
    unsafe {
        let status = sysctlbyname(
            name.as_ptr(),
            value.as_mut_ptr().cast(),
            &mut size,
            ptr::null_mut(),
            0,
        );
        (status == 0 && size == size_of::<T>()).then(|| value.assume_init())
    }
}

fn sysctl_string(name: &CStr) -> Option<String> {
    let mut size = 0;
    // SAFETY: the first call only asks for the length; the second writes at
    // most that many bytes into a buffer of that size.
    let buffer = unsafe {
        if sysctlbyname(
            name.as_ptr(),
            ptr::null_mut(),
            &mut size,
            ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        let mut buffer = vec![0u8; size];
        let status = sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut size,
            ptr::null_mut(),
            0,
        );
        if status != 0 {
            return None;
        }
        buffer.truncate(size);
        buffer
    };
    let end = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
    let text = String::from_utf8_lossy(&buffer[..end]).trim().to_string();
    (!text.is_empty()).then_some(text)
}

pub fn os() -> Os {
    let version = sysctl_string(c"kern.osproductversion").unwrap_or_default();
    let codename = codename(&version).to_string();
    let pretty = ["macOS", &codename, &version]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Os {
        name: "macOS".into(),
        build: sysctl_string(c"kern.osversion").unwrap_or_default(),
        arch: unix::uname().map(|u| u.machine).unwrap_or_default(),
        version,
        codename,
        pretty,
    }
}

fn codename(version: &str) -> &'static str {
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0));
    match (parts.next().unwrap_or(0), parts.next().unwrap_or(0)) {
        (26, _) => "Tahoe",
        (15, _) => "Sequoia",
        (14, _) => "Sonoma",
        (13, _) => "Ventura",
        (12, _) => "Monterey",
        (11, _) => "Big Sur",
        (10, 15) => "Catalina",
        (10, 14) => "Mojave",
        (10, 13) => "High Sierra",
        (10, 12) => "Sierra",
        _ => "",
    }
}

pub fn os_ids() -> Vec<String> {
    vec!["macos".into()]
}

pub fn kernel() -> Kernel {
    let uname = unix::uname();
    Kernel {
        name: uname
            .as_ref()
            .map_or("Darwin".into(), |u| u.sysname.clone()),
        release: uname.map(|u| u.release).unwrap_or_default(),
    }
}

pub fn host() -> Option<Host> {
    // Apple Silicon publishes the marketing name ("MacBook Air (13-inch,
    // M4, 2025)") in the device tree. Intel Macs only have the identifier.
    let model = iokit::Entry::from_path(c"IODeviceTree:/product")
        .and_then(|product| product.property(c"product-name"))
        .and_then(iokit::Property::into_text)
        .or_else(|| sysctl_string(c"hw.model"))?;
    Some(Host {
        model,
        vendor: "Apple".into(),
    })
}

pub fn uptime() -> Option<u64> {
    let boot: Timeval = sysctl(c"kern.boottime")?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    now.checked_sub(u64::try_from(boot.seconds).ok()?)
}

pub fn parent_process() -> Option<String> {
    let mut name = [0 as c_char; 64];
    let pid = c_int::try_from(parent_id()).ok()?;
    // SAFETY: proc_name writes at most the given number of bytes.
    let length = unsafe { proc_name(pid, name.as_mut_ptr().cast(), name.len() as u32) };
    (length > 0).then(|| unix::c_chars(&name))
}

pub fn cpu_model() -> Option<String> {
    sysctl_string(c"machdep.cpu.brand_string")
}

/// The highest clock speed of the performance cores.
pub fn cpu_mhz() -> Option<u32> {
    if let Some(hz) = sysctl::<u64>(c"hw.cpufrequency_max").filter(|hz| *hz > 0) {
        return u32::try_from(hz / 1_000_000).ok();
    }
    // Apple Silicon has no frequency sysctl. The power manager's table of
    // voltage states for the performance cluster holds (frequency, voltage)
    // pairs of 32-bit integers; the largest frequency is the top speed.
    let Some(iokit::Property::Data(states)) =
        iokit::Entry::from_name(c"pmgr")?.property(c"voltage-states5-sram")
    else {
        return None;
    };
    let top = states
        .as_chunks::<8>()
        .0
        .iter()
        .map(|pair| u32::from_le_bytes([pair[0], pair[1], pair[2], pair[3]]))
        .max()
        .filter(|frequency| *frequency > 0)?;
    // The unit changed from Hz to kHz with the M4 generation.
    Some(if top > 100_000_000 {
        top / 1_000_000
    } else {
        top / 1000
    })
}

pub fn cpu_ticks() -> Option<CpuTicks> {
    // user, system, idle, nice
    let mut ticks = [0u32; 4];
    let mut count = ticks.len() as u32;
    // SAFETY: `count` tells the kernel how many integers the buffer holds.
    let status = unsafe {
        host_statistics(
            mach_host_self(),
            HOST_CPU_LOAD_INFO,
            ticks.as_mut_ptr().cast(),
            &mut count,
        )
    };
    if status != 0 {
        return None;
    }
    let [user, system, idle, nice] = ticks.map(u64::from);
    Some(CpuTicks {
        busy: user + system + nice,
        total: user + system + nice + idle,
    })
}

pub fn gpu() -> Option<String> {
    // On Apple Silicon the accelerator itself carries the model name; on
    // Intel it is a property of the PCI device above it.
    iokit::Entry::from_class(c"IOAccelerator")?
        .inherited_property(c"model")?
        .into_text()
}

pub fn memory() -> Option<Usage> {
    let total: u64 = sysctl(c"hw.memsize")?;
    let mut stats = MaybeUninit::<VmStatistics>::zeroed();
    let mut count = (size_of::<VmStatistics>() / size_of::<c_int>()) as u32;
    // SAFETY: `count` is the size of the buffer in integers, as the call
    // expects, and all-zero bytes are a valid value for the structure.
    let stats = unsafe {
        let status = host_statistics64(
            mach_host_self(),
            HOST_VM_INFO64,
            stats.as_mut_ptr().cast(),
            &mut count,
        );
        if status != 0 {
            return None;
        }
        stats.assume_init()
    };
    // SAFETY: getpagesize has no preconditions.
    let page = u64::try_from(unsafe { getpagesize() }).ok()?;

    // What Activity Monitor reports as "Memory Used": app memory, wired
    // memory and the compressor. File cache and purgeable pages are left
    // out because the system hands them back on demand.
    let pages = u64::from(stats.active)
        + u64::from(stats.inactive)
        + u64::from(stats.speculative)
        + u64::from(stats.wired)
        + u64::from(stats.compressor);
    let reclaimable = u64::from(stats.purgeable) + u64::from(stats.external);
    Some(Usage {
        used: pages.saturating_sub(reclaimable) * page,
        total,
    })
}

pub fn swap() -> Option<Usage> {
    let swap: SwapUsage = sysctl(c"vm.swapusage")?;
    Some(Usage {
        used: swap.used,
        total: swap.total,
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
    const VIRTUAL: [(&str, &str); 5] = [
        ("utun", "VPN"),
        ("ipsec", "VPN"),
        ("ppp", "VPN"),
        ("pdp_ip", "Cellular"),
        ("bridge", "Bridge"),
    ];
    if let Some((_, kind)) = VIRTUAL.iter().find(|(prefix, _)| name.starts_with(prefix)) {
        return kind;
    }
    match media_type(name) {
        Some(IFM_IEEE80211) => "Wi-Fi",
        Some(IFM_ETHER) => "Ethernet",
        _ => "",
    }
}

const SIOCGIFMEDIA: c::IoctlRequest = 0xc02c_6938;
const IFM_NMASK: c_int = 0xe0;
const IFM_ETHER: c_int = 0x20;
const IFM_IEEE80211: c_int = 0x80;
const IFNAMSIZ: usize = 16;

/// `struct ifmediareq` from `<net/if.h>`, which is declared with 4-byte
/// packing.
#[repr(C, packed(4))]
struct IfMediaReq {
    name: [c_char; IFNAMSIZ],
    current: c_int,
    mask: c_int,
    status: c_int,
    active: c_int,
    count: c_int,
    list: *mut c_int,
}

/// Asks the driver what kind of link an interface is (`IFM_*`).
fn media_type(name: &str) -> Option<c_int> {
    let mut request = IfMediaReq {
        name: [0; IFNAMSIZ],
        current: 0,
        mask: 0,
        status: 0,
        active: 0,
        count: 0,
        list: ptr::null_mut(),
    };
    for (slot, byte) in request.name.iter_mut().zip(name.bytes().take(IFNAMSIZ - 1)) {
        *slot = byte as c_char;
    }
    // Any socket will do to carry the request; it is never connected.
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    // SAFETY: the request matches the layout the ioctl expects, and with a
    // zero `count` the kernel does not touch `list`.
    let status = unsafe { c::ioctl(socket.as_raw_fd(), SIOCGIFMEDIA, &mut request) };
    (status == 0).then_some(request.current & IFM_NMASK)
}
