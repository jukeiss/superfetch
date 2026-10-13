//! The Win32 API, as far as superfetch uses it.
//!
//! Declared by hand so the crate needs nothing but the standard library.
//! Names and layouts follow the Windows SDK headers. Structures that are
//! only ever read through a pointer the system hands out are declared up
//! to the last field superfetch looks at.
//!
//! `raw-dylib` makes the compiler generate the import tables itself, so no
//! import libraries are required at link time.

#![allow(non_snake_case, non_camel_case_types, clippy::upper_case_acronyms)]

use std::ffi::c_void;

pub type BOOL = i32;
pub type HANDLE = *mut c_void;
pub type HKEY = *mut c_void;

pub const ERROR_SUCCESS: u32 = 0;
pub const ERROR_BUFFER_OVERFLOW: u32 = 111;
pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
pub const HKEY_LOCAL_MACHINE: HKEY = 0x8000_0002u32 as i32 as isize as HKEY;
pub const RRF_RT_REG_SZ: u32 = 0x2;
pub const RRF_RT_REG_DWORD: u32 = 0x10;

pub const STD_INPUT_HANDLE: u32 = -10i32 as u32;
pub const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
pub const ENABLE_PROCESSED_INPUT: u32 = 0x1;
pub const ENABLE_LINE_INPUT: u32 = 0x2;
pub const ENABLE_ECHO_INPUT: u32 = 0x4;
pub const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x200;
pub const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x4;

pub const TH32CS_SNAPPROCESS: u32 = 0x2;
pub const DISPLAY_DEVICE_PRIMARY_DEVICE: u32 = 0x4;

pub const AF_INET: u16 = 2;
pub const GAA_FLAG_SKIP_ANYCAST: u32 = 0x2;
pub const GAA_FLAG_SKIP_MULTICAST: u32 = 0x4;
pub const GAA_FLAG_SKIP_DNS_SERVER: u32 = 0x8;
pub const IF_OPER_STATUS_UP: i32 = 1;
pub const IF_TYPE_ETHERNET: u32 = 6;
pub const IF_TYPE_PPP: u32 = 23;
pub const IF_TYPE_LOOPBACK: u32 = 24;
pub const IF_TYPE_PROP_VIRTUAL: u32 = 53;
pub const IF_TYPE_WIFI: u32 = 71;
pub const IF_TYPE_TUNNEL: u32 = 131;
pub const IF_TYPE_WWAN: u32 = 243;
pub const IF_TYPE_WWAN2: u32 = 244;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FILETIME {
    pub dwLowDateTime: u32,
    pub dwHighDateTime: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MEMORYSTATUSEX {
    pub dwLength: u32,
    pub dwMemoryLoad: u32,
    pub ullTotalPhys: u64,
    pub ullAvailPhys: u64,
    pub ullTotalPageFile: u64,
    pub ullAvailPageFile: u64,
    pub ullTotalVirtual: u64,
    pub ullAvailVirtual: u64,
    pub ullAvailExtendedVirtual: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PROCESSENTRY32W {
    pub dwSize: u32,
    pub cntUsage: u32,
    pub th32ProcessID: u32,
    pub th32DefaultHeapID: usize,
    pub th32ModuleID: u32,
    pub cntThreads: u32,
    pub th32ParentProcessID: u32,
    pub pcPriClassBase: i32,
    pub dwFlags: u32,
    pub szExeFile: [u16; 260],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DISPLAY_DEVICEW {
    pub cb: u32,
    pub DeviceName: [u16; 32],
    pub DeviceString: [u16; 128],
    pub StateFlags: u32,
    pub DeviceID: [u16; 128],
    pub DeviceKey: [u16; 128],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct COORD {
    pub X: i16,
    pub Y: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SMALL_RECT {
    pub Left: i16,
    pub Top: i16,
    pub Right: i16,
    pub Bottom: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CONSOLE_SCREEN_BUFFER_INFO {
    pub dwSize: COORD,
    pub dwCursorPosition: COORD,
    pub wAttributes: u16,
    pub srWindow: SMALL_RECT,
    pub dwMaximumWindowSize: COORD,
}

/// `SOCKADDR_IN` up to the address, which is in network byte order.
#[repr(C)]
pub struct SOCKADDR_IN {
    pub sin_family: u16,
    pub sin_port: u16,
    pub sin_addr: u32,
}

#[repr(C)]
pub struct SOCKET_ADDRESS {
    pub lpSockaddr: *const SOCKADDR_IN,
    pub iSockaddrLength: i32,
}

/// The leading fields of `IP_ADAPTER_UNICAST_ADDRESS_LH`.
#[repr(C)]
pub struct IP_ADAPTER_UNICAST_ADDRESS {
    pub Length: u32,
    pub Flags: u32,
    pub Next: *const IP_ADAPTER_UNICAST_ADDRESS,
    pub Address: SOCKET_ADDRESS,
}

/// The leading fields of `IP_ADAPTER_ADDRESSES_LH`.
#[repr(C)]
pub struct IP_ADAPTER_ADDRESSES {
    pub Length: u32,
    pub IfIndex: u32,
    pub Next: *const IP_ADAPTER_ADDRESSES,
    pub AdapterName: *const u8,
    pub FirstUnicastAddress: *const IP_ADAPTER_UNICAST_ADDRESS,
    pub FirstAnycastAddress: *const c_void,
    pub FirstMulticastAddress: *const c_void,
    pub FirstDnsServerAddress: *const c_void,
    pub DnsSuffix: *const u16,
    pub Description: *const u16,
    pub FriendlyName: *const u16,
    pub PhysicalAddress: [u8; 8],
    pub PhysicalAddressLength: u32,
    pub Flags: u32,
    pub Mtu: u32,
    pub IfType: u32,
    pub OperStatus: i32,
}

#[link(name = "kernel32", kind = "raw-dylib")]
unsafe extern "system" {
    pub fn CloseHandle(handle: HANDLE) -> BOOL;
    pub fn GetTickCount64() -> u64;
    pub fn GetCurrentProcessId() -> u32;
    pub fn GlobalMemoryStatusEx(status: *mut MEMORYSTATUSEX) -> BOOL;
    pub fn GetSystemTimes(idle: *mut FILETIME, kernel: *mut FILETIME, user: *mut FILETIME) -> BOOL;
    pub fn GetDiskFreeSpaceExW(
        directory: *const u16,
        free_to_caller: *mut u64,
        total: *mut u64,
        free: *mut u64,
    ) -> BOOL;
    pub fn CreateToolhelp32Snapshot(flags: u32, process: u32) -> HANDLE;
    pub fn Process32FirstW(snapshot: HANDLE, entry: *mut PROCESSENTRY32W) -> BOOL;
    pub fn Process32NextW(snapshot: HANDLE, entry: *mut PROCESSENTRY32W) -> BOOL;
    pub fn GetStdHandle(which: u32) -> HANDLE;
    pub fn GetConsoleMode(console: HANDLE, mode: *mut u32) -> BOOL;
    pub fn SetConsoleMode(console: HANDLE, mode: u32) -> BOOL;
    pub fn GetConsoleScreenBufferInfo(
        console: HANDLE,
        info: *mut CONSOLE_SCREEN_BUFFER_INFO,
    ) -> BOOL;
}

#[link(name = "advapi32", kind = "raw-dylib")]
unsafe extern "system" {
    pub fn RegGetValueW(
        key: HKEY,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> u32;
}

#[link(name = "user32", kind = "raw-dylib")]
unsafe extern "system" {
    pub fn EnumDisplayDevicesW(
        device: *const u16,
        index: u32,
        display: *mut DISPLAY_DEVICEW,
        flags: u32,
    ) -> BOOL;
}

#[link(name = "iphlpapi", kind = "raw-dylib")]
unsafe extern "system" {
    pub fn GetAdaptersAddresses(
        family: u32,
        flags: u32,
        reserved: *const c_void,
        addresses: *mut IP_ADAPTER_ADDRESSES,
        size: *mut u32,
    ) -> u32;
}
