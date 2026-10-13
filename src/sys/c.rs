//! The C library, as far as superfetch uses it.
//!
//! These declarations are written by hand so the crate needs nothing but
//! the standard library, which already links the C library on Unix.
//!
//! Structures are declared only up to the last field superfetch reads.
//! Where the real structure is longer, or differs between systems beyond
//! that point, the remainder is padding sized generously enough for every
//! supported target; the kernel and libc only ever see a pointer.

use std::ffi::c_ulong;
pub use std::ffi::{c_char, c_int, c_uint};

pub const AF_INET: Family = 2;
pub const IFF_UP: c_uint = 0x1;
pub const IFF_LOOPBACK: c_uint = 0x8;
pub const IFF_RUNNING: c_uint = 0x40;
pub const STDIN: c_int = 0;
pub const STDOUT: c_int = 1;
pub const TCSAFLUSH: c_int = 2;

/// The type of an `ioctl` request number.
#[cfg(target_env = "musl")]
pub type IoctlRequest = c_int;
#[cfg(not(target_env = "musl"))]
pub type IoctlRequest = c_ulong;

/// "Get window size". Most Linux architectures number their ioctls one
/// way; the rest share the BSD encoding that macOS uses.
#[cfg(all(
    target_os = "linux",
    not(any(
        target_arch = "mips",
        target_arch = "mips64",
        target_arch = "powerpc",
        target_arch = "powerpc64",
        target_arch = "sparc64"
    ))
))]
pub const TIOCGWINSZ: IoctlRequest = 0x5413;
#[cfg(not(all(
    target_os = "linux",
    not(any(
        target_arch = "mips",
        target_arch = "mips64",
        target_arch = "powerpc",
        target_arch = "powerpc64",
        target_arch = "sparc64"
    ))
)))]
pub const TIOCGWINSZ: IoctlRequest = 0x4008_7468;

/// Length of each field of `struct utsname`.
#[cfg(target_os = "macos")]
const UTSNAME_FIELD: usize = 256;
#[cfg(not(target_os = "macos"))]
const UTSNAME_FIELD: usize = 65;

/// `struct utsname`. Linux has a sixth field that macOS lacks; declaring
/// it everywhere only makes the buffer larger than macOS needs.
#[repr(C)]
pub struct Utsname {
    pub sysname: [c_char; UTSNAME_FIELD],
    pub nodename: [c_char; UTSNAME_FIELD],
    pub release: [c_char; UTSNAME_FIELD],
    pub version: [c_char; UTSNAME_FIELD],
    pub machine: [c_char; UTSNAME_FIELD],
    pub domainname: [c_char; UTSNAME_FIELD],
}

/// `struct winsize`.
#[repr(C)]
#[derive(Default)]
pub struct Winsize {
    pub rows: u16,
    pub columns: u16,
    pub x_pixels: u16,
    pub y_pixels: u16,
}

/// `struct termios`, treated as an opaque block. Its layout varies a lot
/// between systems, and it never needs to be looked into: the terminal
/// state is read, handed to `cfmakeraw`, and later written back.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
pub struct Termios([u8; 256]);

impl Termios {
    pub const fn zeroed() -> Termios {
        Termios([0; 256])
    }
}

/// The leading fields of `struct ifaddrs`, which are the same everywhere.
#[repr(C)]
pub struct IfAddrs {
    pub next: *mut IfAddrs,
    pub name: *const c_char,
    pub flags: c_uint,
    pub address: *const SockAddrIn,
}

/// The address family field of a socket address. The BSDs spend the first
/// byte of the structure on its length; Linux gives the family both bytes.
#[cfg(target_os = "macos")]
pub type Family = u8;
#[cfg(not(target_os = "macos"))]
pub type Family = u16;

/// `struct sockaddr_in` up to the address.
#[repr(C)]
pub struct SockAddrIn {
    #[cfg(target_os = "macos")]
    pub length: u8,
    pub family: Family,
    pub port: u16,
    /// In network byte order.
    pub address: u32,
}

/// The leading fields of macOS's `struct statfs`.
#[cfg(target_os = "macos")]
#[repr(C)]
pub struct StatFs {
    pub block_size: u32,
    pub io_size: i32,
    pub blocks: u64,
    pub blocks_free: u64,
    padding: [u64; 320],
}

/// The leading fields of `struct statvfs` on 64-bit Linux, where glibc and
/// musl agree. Their 32-bit layouts differ from this and from each other.
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
#[repr(C)]
pub struct StatVfs {
    pub block_size: c_ulong,
    pub fragment_size: c_ulong,
    pub blocks: u64,
    pub blocks_free: u64,
    padding: [u64; 28],
}

unsafe extern "C" {
    pub fn uname(buffer: *mut Utsname) -> c_int;
    pub fn gethostname(name: *mut c_char, length: usize) -> c_int;
    pub fn getloadavg(averages: *mut f64, count: c_int) -> c_int;
    pub fn getifaddrs(list: *mut *mut IfAddrs) -> c_int;
    pub fn freeifaddrs(list: *mut IfAddrs);
    pub fn ioctl(fd: c_int, request: IoctlRequest, ...) -> c_int;
    pub fn tcgetattr(fd: c_int, termios: *mut Termios) -> c_int;
    pub fn tcsetattr(fd: c_int, actions: c_int, termios: *const Termios) -> c_int;
    pub fn cfmakeraw(termios: *mut Termios);

    // On Intel Macs the plain symbol is the legacy 32-bit-inode variant.
    #[cfg(target_os = "macos")]
    #[cfg_attr(target_arch = "x86_64", link_name = "statfs$INODE64")]
    pub fn statfs(path: *const c_char, buffer: *mut StatFs) -> c_int;

    #[cfg(all(target_os = "linux", target_pointer_width = "64"))]
    pub fn statvfs(path: *const c_char, buffer: *mut StatVfs) -> c_int;
}
