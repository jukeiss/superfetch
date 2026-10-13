//! What Linux and macOS have in common: the POSIX calls.

use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::net::Ipv4Addr;
use std::{env, ptr, thread};

use super::Usage;
use super::c::{self, c_char};

pub struct Uname {
    pub sysname: String,
    pub release: String,
    pub machine: String,
}

pub fn uname() -> Option<Uname> {
    let mut raw = MaybeUninit::<c::Utsname>::zeroed();
    // SAFETY: the buffer is at least as large as the real structure, and
    // all-zero bytes are a valid value for it.
    let raw = unsafe {
        if c::uname(raw.as_mut_ptr()) != 0 {
            return None;
        }
        raw.assume_init()
    };
    Some(Uname {
        sysname: c_chars(&raw.sysname),
        release: c_chars(&raw.release),
        machine: c_chars(&raw.machine),
    })
}

/// Converts a NUL-terminated C character array.
// `c_char` is signed on some targets and unsigned on others.
#[allow(clippy::unnecessary_cast)]
pub fn c_chars(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| *c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn username() -> String {
    ["USER", "LOGNAME"]
        .iter()
        .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_default()
}

pub fn hostname() -> String {
    let mut buffer = [0 as c_char; 256];
    // SAFETY: the length passed leaves room for a terminating NUL.
    let status = unsafe { c::gethostname(buffer.as_mut_ptr(), buffer.len() - 1) };
    if status != 0 {
        return String::new();
    }
    c_chars(&buffer)
}

pub fn cpu_cores() -> usize {
    thread::available_parallelism().map_or(1, |cores| cores.get())
}

/// The one-minute load average.
pub fn load_average() -> Option<f64> {
    let mut load = [0f64; 1];
    // SAFETY: the buffer holds the one sample that is requested.
    let samples = unsafe { c::getloadavg(load.as_mut_ptr(), 1) };
    (samples == 1).then_some(load[0])
}

pub fn default_disk() -> String {
    "/".into()
}

pub fn disk(path: &str) -> Option<Usage> {
    let path = CString::new(path).ok()?;
    let (blocks, free, block_size) = filesystem(&path)?;
    Some(Usage {
        used: blocks.saturating_sub(free) * block_size,
        total: blocks * block_size,
    })
}

/// Total blocks, free blocks and the block size of the filesystem at `path`.
#[cfg(target_os = "macos")]
fn filesystem(path: &CStr) -> Option<(u64, u64, u64)> {
    // statfs rather than statvfs: on macOS the latter counts blocks in
    // 32 bits and wraps on large volumes.
    let mut stat = MaybeUninit::<c::StatFs>::zeroed();
    // SAFETY: `path` is NUL-terminated, the buffer is larger than the real
    // structure, and all-zero bytes are a valid value for it.
    let stat = unsafe {
        if c::statfs(path.as_ptr(), stat.as_mut_ptr()) != 0 {
            return None;
        }
        stat.assume_init()
    };
    Some((stat.blocks, stat.blocks_free, u64::from(stat.block_size)))
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn filesystem(path: &CStr) -> Option<(u64, u64, u64)> {
    let mut stat = MaybeUninit::<c::StatVfs>::zeroed();
    // SAFETY: as for the macOS variant above.
    let stat = unsafe {
        if c::statvfs(path.as_ptr(), stat.as_mut_ptr()) != 0 {
            return None;
        }
        stat.assume_init()
    };
    Some((stat.blocks, stat.blocks_free, stat.fragment_size))
}

/// 32-bit Linux is not covered: its `statvfs` layout depends on the C
/// library in use, so the disk module stays empty there.
#[cfg(all(target_os = "linux", not(target_pointer_width = "64")))]
fn filesystem(_: &CStr) -> Option<(u64, u64, u64)> {
    None
}

/// Finds the interface that owns `wanted`. Without an address to look for
/// (the machine has no default route), settles for the first interface
/// that is up and not the loopback.
pub fn find_interface(wanted: Option<Ipv4Addr>) -> Option<(Ipv4Addr, String)> {
    let mut list = ptr::null_mut();
    // SAFETY: on success getifaddrs stores a list that stays valid until
    // freeifaddrs; every pointer read below belongs to that list.
    unsafe {
        if c::getifaddrs(&mut list) != 0 {
            return None;
        }
        let mut found = None;
        let mut cursor = list;
        while !cursor.is_null() {
            let entry = &*cursor;
            cursor = entry.next;
            if entry.address.is_null() || (*entry.address).family != c::AF_INET {
                continue;
            }
            let ip = Ipv4Addr::from(u32::from_be((*entry.address).address));
            let usable = entry.flags & c::IFF_UP != 0
                && entry.flags & c::IFF_RUNNING != 0
                && entry.flags & c::IFF_LOOPBACK == 0;
            if wanted.map_or(usable, |wanted| wanted == ip) {
                let name = CStr::from_ptr(entry.name).to_string_lossy().into_owned();
                found = Some((ip, name));
                break;
            }
        }
        c::freeifaddrs(list);
        found
    }
}
