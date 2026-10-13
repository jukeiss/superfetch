//! The platform layer: everything that asks the operating system a question.
//!
//! Each supported OS has one file implementing the same set of functions;
//! the rest of the program only uses the re-exports below and never contains
//! a `cfg`. Porting to another system means adding one more file here.
//!
//! Functions return `None` when a value is unavailable. They never print
//! and never panic on unexpected data: a fetch tool should degrade by
//! leaving out a line, not by failing.

use std::net::{IpAddr, Ipv4Addr, UdpSocket};

#[cfg(unix)]
pub mod c;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
pub mod win32;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("superfetch supports Linux, macOS and Windows; see src/sys/mod.rs to add a port");

pub use platform::{
    cpu_cores, cpu_mhz, cpu_model, cpu_ticks, default_disk, disk, gpu, host, hostname, kernel,
    load_average, memory, os, os_ids, parent_process, swap, uptime, username,
};

#[derive(Debug)]
pub struct Os {
    /// Product name without a version, e.g. `macOS` or `Fedora Linux`.
    pub name: String,
    pub version: String,
    pub codename: String,
    pub build: String,
    /// The name as the OS presents itself, e.g. `macOS Sequoia 15.6.1`.
    pub pretty: String,
    pub arch: String,
}

#[derive(Debug)]
pub struct Kernel {
    pub name: String,
    pub release: String,
}

#[derive(Debug)]
pub struct Host {
    pub model: String,
    pub vendor: String,
}

/// An amount in use out of a total, in bytes.
#[derive(Clone, Copy, Debug)]
pub struct Usage {
    pub used: u64,
    pub total: u64,
}

/// Cumulative CPU time since boot, summed over all cores, in the kernel's
/// own tick unit. Only differences between two samples are meaningful.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuTicks {
    pub busy: u64,
    pub total: u64,
}

#[derive(Debug)]
pub struct Network {
    pub ip: Ipv4Addr,
    pub interface: String,
    /// `Wi-Fi`, `Ethernet`, `VPN`, ... Empty when the kind is unknown.
    pub kind: &'static str,
}

/// The interface that carries traffic to the internet.
pub fn network() -> Option<Network> {
    platform::interface(outbound_ip())
}

/// The local address the routing table selects for outbound traffic.
///
/// Connecting a UDP socket sends nothing; it only makes the kernel pick a
/// route, after which the socket's local address is the answer. The target
/// is a documentation-only address (RFC 5737), reachable through any default
/// route.
fn outbound_ip() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip),
        _ => None,
    }
}
