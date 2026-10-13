//! Windows: the registry for names and versions, Win32 for live numbers.
//!
//! WMI would answer most of these questions too, but a single query costs
//! more than this whole program is allowed to take.

use std::mem::{size_of, size_of_val, zeroed};
use std::net::Ipv4Addr;
use std::{env, ptr, thread};

use super::win32::*;
use super::{CpuTicks, Host, Kernel, Network, Os, Usage};

const VERSION_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const BIOS_KEY: &str = r"HARDWARE\DESCRIPTION\System\BIOS";
const CPU_KEY: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";

/// A NUL-terminated UTF-16 copy of `text`, as wide Win32 calls expect.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Decodes UTF-16 up to the first NUL.
fn from_wide(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end]).trim().to_string()
}

fn registry_string(key: &str, value: &str) -> Option<String> {
    let (key, value) = (wide(key), wide(value));
    let mut buffer = [0u16; 256];
    let mut size = size_of_val(&buffer) as u32;
    // SAFETY: both names are NUL-terminated and `size` is the buffer's size
    // in bytes, which the call never exceeds.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    Some(from_wide(&buffer)).filter(|text| !text.is_empty())
}

fn registry_number(key: &str, value: &str) -> Option<u32> {
    let (key, value) = (wide(key), wide(value));
    let mut number = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: as in `registry_string`, with a buffer of exactly one DWORD.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            ptr::null_mut(),
            ptr::from_mut(&mut number).cast(),
            &mut size,
        )
    };
    (status == ERROR_SUCCESS).then_some(number)
}

fn build_number() -> u32 {
    registry_string(VERSION_KEY, "CurrentBuildNumber")
        .and_then(|build| build.parse().ok())
        .unwrap_or(0)
}

pub fn os() -> Os {
    let build = build_number();
    let mut name =
        registry_string(VERSION_KEY, "ProductName").unwrap_or_else(|| "Windows".to_string());
    // Windows 11 still calls itself Windows 10 in the registry; the build
    // number is the documented way to tell them apart.
    if build >= 22000 {
        name = name.replacen("Windows 10", "Windows 11", 1);
    }
    let version = registry_string(VERSION_KEY, "DisplayVersion").unwrap_or_default();
    let revision = registry_number(VERSION_KEY, "UBR");
    Os {
        pretty: format!("{name} {version}").trim().to_string(),
        name,
        version,
        codename: String::new(),
        build: match revision {
            Some(revision) => format!("{build}.{revision}"),
            None => build.to_string(),
        },
        arch: arch(),
    }
}

fn arch() -> String {
    // The WOW64 variable is set, and holds the real architecture, when a
    // 32-bit build runs on a 64-bit system.
    let reported = env::var("PROCESSOR_ARCHITEW6432")
        .or_else(|_| env::var("PROCESSOR_ARCHITECTURE"))
        .unwrap_or_default();
    match reported.as_str() {
        "AMD64" => "x86_64".into(),
        "ARM64" => "aarch64".into(),
        "x86" => "x86".into(),
        _ => env::consts::ARCH.into(),
    }
}

pub fn os_ids() -> Vec<String> {
    vec!["windows".into()]
}

pub fn kernel() -> Kernel {
    let major = registry_number(VERSION_KEY, "CurrentMajorVersionNumber").unwrap_or(10);
    let minor = registry_number(VERSION_KEY, "CurrentMinorVersionNumber").unwrap_or(0);
    Kernel {
        name: "Windows NT".into(),
        release: format!("{major}.{minor}.{}", build_number()),
    }
}

pub fn host() -> Option<Host> {
    const PLACEHOLDERS: [&str; 4] = [
        "to be filled by o.e.m.",
        "default string",
        "system product name",
        "system manufacturer",
    ];
    let bios = |value: &str| {
        registry_string(BIOS_KEY, value)
            .filter(|text| !PLACEHOLDERS.contains(&text.to_lowercase().as_str()))
    };
    let vendor = bios("SystemManufacturer").unwrap_or_default();
    let model = bios("SystemProductName").or_else(|| bios("BaseBoardProduct"))?;
    Some(Host { model, vendor })
}

pub fn uptime() -> Option<u64> {
    // SAFETY: GetTickCount64 has no preconditions.
    Some(unsafe { GetTickCount64() } / 1000)
}

pub fn username() -> String {
    env::var("USERNAME").unwrap_or_default()
}

pub fn hostname() -> String {
    env::var("COMPUTERNAME").unwrap_or_default()
}

/// The name of the process that started this one, without `.exe`.
pub fn parent_process() -> Option<String> {
    // SAFETY: the snapshot handle is checked, used only while open, and
    // closed on every path; `dwSize` is set as the API requires.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let find = |wanted: u32| {
            let mut entry: PROCESSENTRY32W = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut more = Process32FirstW(snapshot, &mut entry);
            while more != 0 {
                if entry.th32ProcessID == wanted {
                    return Some(entry);
                }
                more = Process32NextW(snapshot, &mut entry);
            }
            None
        };
        let parent = find(GetCurrentProcessId()).and_then(|me| find(me.th32ParentProcessID));
        CloseHandle(snapshot);

        let name = from_wide(&parent?.szExeFile);
        let name = name
            .strip_suffix(".exe")
            .or_else(|| name.strip_suffix(".EXE"))
            .unwrap_or(&name);
        Some(name.to_string())
    }
}

pub fn cpu_model() -> Option<String> {
    registry_string(CPU_KEY, "ProcessorNameString").map(|raw| {
        let name = raw.split(" @ ").next().unwrap_or(&raw);
        let mut name = name.to_string();
        for noise in ["(R)", "(TM)", "(tm)", " CPU", " Processor"] {
            name = name.replace(noise, "");
        }
        name.split_whitespace().collect::<Vec<_>>().join(" ")
    })
}

pub fn cpu_cores() -> usize {
    thread::available_parallelism().map_or(1, |n| n.get())
}

pub fn cpu_mhz() -> Option<u32> {
    registry_number(CPU_KEY, "~MHz").filter(|mhz| *mhz > 0)
}

pub fn cpu_ticks() -> Option<CpuTicks> {
    let as_u64 =
        |time: FILETIME| u64::from(time.dwHighDateTime) << 32 | u64::from(time.dwLowDateTime);
    // SAFETY: the three out-pointers refer to live FILETIME values.
    unsafe {
        let (mut idle, mut kernel, mut user): (FILETIME, FILETIME, FILETIME) =
            (zeroed(), zeroed(), zeroed());
        if GetSystemTimes(&mut idle, &mut kernel, &mut user) == 0 {
            return None;
        }
        // Kernel time includes idle time.
        let total = as_u64(kernel) + as_u64(user);
        Some(CpuTicks {
            busy: total.saturating_sub(as_u64(idle)),
            total,
        })
    }
}

/// Windows keeps no load average.
pub fn load_average() -> Option<f64> {
    None
}

/// The adapter driving the primary display.
pub fn gpu() -> Option<String> {
    // SAFETY: `cb` is set as the API requires and the struct outlives the call.
    unsafe {
        let mut device: DISPLAY_DEVICEW = zeroed();
        device.cb = size_of::<DISPLAY_DEVICEW>() as u32;
        let mut first = None;
        let mut index = 0;
        while EnumDisplayDevicesW(ptr::null(), index, &mut device, 0) != 0 {
            let name = from_wide(&device.DeviceString);
            if device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0 {
                return Some(name);
            }
            first.get_or_insert(name);
            index += 1;
        }
        first
    }
}

fn memory_status() -> Option<MEMORYSTATUSEX> {
    // SAFETY: `dwLength` is set as the API requires.
    unsafe {
        let mut status: MEMORYSTATUSEX = zeroed();
        status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        (GlobalMemoryStatusEx(&mut status) != 0).then_some(status)
    }
}

pub fn memory() -> Option<Usage> {
    let status = memory_status()?;
    Some(Usage {
        used: status.ullTotalPhys - status.ullAvailPhys,
        total: status.ullTotalPhys,
    })
}

/// The page file. Windows reports the commit limit (RAM plus page file)
/// rather than the page file itself, so both figures are derived by
/// subtracting the physical part.
pub fn swap() -> Option<Usage> {
    let status = memory_status()?;
    let total = status.ullTotalPageFile.saturating_sub(status.ullTotalPhys);
    let committed = status.ullTotalPageFile - status.ullAvailPageFile;
    let in_ram = status.ullTotalPhys - status.ullAvailPhys;
    Some(Usage {
        used: committed.saturating_sub(in_ram).min(total),
        total,
    })
}

pub fn default_disk() -> String {
    format!(
        "{}\\",
        env::var("SystemDrive").unwrap_or_else(|_| "C:".into())
    )
}

pub fn disk(path: &str) -> Option<Usage> {
    let path = wide(path);
    let (mut total, mut free) = (0u64, 0u64);
    // SAFETY: `path` is NUL-terminated and the out-pointers are live.
    let ok = unsafe { GetDiskFreeSpaceExW(path.as_ptr(), ptr::null_mut(), &mut total, &mut free) };
    (ok != 0).then(|| Usage {
        used: total.saturating_sub(free),
        total,
    })
}

/// Finds the adapter that owns `wanted`. Without an address to look for,
/// settles for the first adapter that is up and not the loopback.
pub fn interface(wanted: Option<Ipv4Addr>) -> Option<Network> {
    const FLAGS: u32 = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    // u64 elements keep the buffer aligned for the structs written into it.
    let mut buffer: Vec<u64> = Vec::new();
    let mut size = 16 * 1024u32;
    // SAFETY: the buffer is at least `size` bytes; the list written into it
    // is only walked while the buffer is alive, and every pointer followed
    // is one the API produced.
    unsafe {
        loop {
            buffer.resize(size as usize / size_of::<u64>() + 1, 0);
            let status = GetAdaptersAddresses(
                u32::from(AF_INET),
                FLAGS,
                ptr::null(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            );
            match status {
                ERROR_SUCCESS => break,
                // `size` now holds the length that is needed.
                ERROR_BUFFER_OVERFLOW => continue,
                _ => return None,
            }
        }

        let mut adapter = buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES>();
        while !adapter.is_null() {
            let current = &*adapter;
            adapter = current.Next;
            let usable =
                current.OperStatus == IF_OPER_STATUS_UP && current.IfType != IF_TYPE_LOOPBACK;

            let mut unicast = current.FirstUnicastAddress;
            while !unicast.is_null() {
                let address = (*unicast).Address.lpSockaddr;
                unicast = (*unicast).Next;
                if address.is_null() || (*address).sin_family != AF_INET {
                    continue;
                }
                let ip = Ipv4Addr::from(u32::from_be((*address).sin_addr));
                if wanted.map_or(usable, |wanted| wanted == ip) {
                    return Some(Network {
                        ip,
                        interface: wide_ptr_to_string(current.FriendlyName),
                        kind: interface_kind(current.IfType),
                    });
                }
            }
        }
        None
    }
}

/// # Safety
/// `text` must be null or point to a NUL-terminated UTF-16 string.
unsafe fn wide_ptr_to_string(text: *const u16) -> String {
    if text.is_null() {
        return String::new();
    }
    unsafe {
        let mut length = 0;
        while *text.add(length) != 0 {
            length += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(text, length))
    }
}

fn interface_kind(if_type: u32) -> &'static str {
    match if_type {
        IF_TYPE_ETHERNET => "Ethernet",
        IF_TYPE_WIFI => "Wi-Fi",
        IF_TYPE_PPP | IF_TYPE_TUNNEL | IF_TYPE_PROP_VIRTUAL => "VPN",
        IF_TYPE_WWAN | IF_TYPE_WWAN2 => "Cellular",
        _ => "",
    }
}
