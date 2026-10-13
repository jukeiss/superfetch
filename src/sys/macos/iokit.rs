//! A minimal binding to the I/O Registry, where macOS keeps the hardware
//! details that sysctl does not expose.
//!
//! Only the handful of calls superfetch needs are declared. Everything
//! unsafe stays inside this file; callers get owned Rust values.

use std::ffi::{CStr, c_char, c_void};
use std::ptr;

type CfRef = *const c_void;
type IoObject = u32;

const UTF8: u32 = 0x0800_0100;
const SEARCH_PARENTS: u32 = 0x3; // kIORegistryIterateRecursively | kIORegistryIterateParents

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(allocator: CfRef, text: *const c_char, encoding: u32) -> CfRef;
    fn CFStringGetCString(string: CfRef, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
    fn CFGetTypeID(object: CfRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFDataGetTypeID() -> usize;
    fn CFDataGetBytePtr(data: CfRef) -> *const u8;
    fn CFDataGetLength(data: CfRef) -> isize;
    fn CFRelease(object: CfRef);
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IORegistryEntryFromPath(port: u32, path: *const c_char) -> IoObject;
    fn IOServiceMatching(class: *const c_char) -> CfRef;
    fn IOServiceNameMatching(name: *const c_char) -> CfRef;
    fn IOServiceGetMatchingService(port: u32, matching: CfRef) -> IoObject;
    fn IORegistryEntrySearchCFProperty(
        entry: IoObject,
        plane: *const c_char,
        key: CfRef,
        allocator: CfRef,
        options: u32,
    ) -> CfRef;
    fn IORegistryEntryCreateCFProperty(
        entry: IoObject,
        key: CfRef,
        allocator: CfRef,
        options: u32,
    ) -> CfRef;
    fn IOObjectRelease(object: IoObject) -> i32;
}

/// A registry property, copied out of CoreFoundation.
pub enum Property {
    Text(String),
    Data(Vec<u8>),
}

impl Property {
    /// The property as text. Device-tree strings are stored as raw
    /// NUL-terminated bytes rather than as CFStrings.
    pub fn into_text(self) -> Option<String> {
        let text = match self {
            Property::Text(text) => text,
            Property::Data(bytes) => {
                let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
                String::from_utf8(bytes[..end].to_vec()).ok()?
            }
        };
        Some(text.trim().to_string()).filter(|text| !text.is_empty())
    }
}

/// An owned reference to a registry entry.
pub struct Entry(IoObject);

impl Entry {
    /// Looks up an entry by path, e.g. `IODeviceTree:/product`.
    pub fn from_path(path: &CStr) -> Option<Entry> {
        // SAFETY: `path` is NUL-terminated; 0 selects the default main port.
        let object = unsafe { IORegistryEntryFromPath(0, path.as_ptr()) };
        (object != 0).then_some(Entry(object))
    }

    /// The first service of the given class, e.g. `IOAccelerator`.
    pub fn from_class(class: &CStr) -> Option<Entry> {
        // SAFETY: IOServiceGetMatchingService consumes the dictionary, and
        // treats a null one as "no match".
        let object = unsafe { IOServiceGetMatchingService(0, IOServiceMatching(class.as_ptr())) };
        (object != 0).then_some(Entry(object))
    }

    /// The first service with the given registry name, e.g. `pmgr`.
    pub fn from_name(name: &CStr) -> Option<Entry> {
        // SAFETY: as in `from_class`.
        let object =
            unsafe { IOServiceGetMatchingService(0, IOServiceNameMatching(name.as_ptr())) };
        (object != 0).then_some(Entry(object))
    }

    /// Reads a property of this entry.
    pub fn property(&self, key: &CStr) -> Option<Property> {
        self.read(key, false)
    }

    /// Reads a property of this entry or, failing that, of its closest
    /// ancestor that has one.
    pub fn inherited_property(&self, key: &CStr) -> Option<Property> {
        self.read(key, true)
    }

    fn read(&self, key: &CStr, search_parents: bool) -> Option<Property> {
        // SAFETY: every CoreFoundation object created here is released
        // before returning, and its contents are copied while it is alive.
        unsafe {
            let key = CFStringCreateWithCString(ptr::null(), key.as_ptr(), UTF8);
            if key.is_null() {
                return None;
            }
            let value = if search_parents {
                let plane = c"IOService".as_ptr();
                IORegistryEntrySearchCFProperty(self.0, plane, key, ptr::null(), SEARCH_PARENTS)
            } else {
                IORegistryEntryCreateCFProperty(self.0, key, ptr::null(), 0)
            };
            CFRelease(key);
            if value.is_null() {
                return None;
            }
            let property = copy_out(value);
            CFRelease(value);
            property
        }
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by this value and released only here.
        unsafe { IOObjectRelease(self.0) };
    }
}

/// # Safety
/// `value` must be a live CoreFoundation object.
unsafe fn copy_out(value: CfRef) -> Option<Property> {
    unsafe {
        let kind = CFGetTypeID(value);
        if kind == CFStringGetTypeID() {
            let mut buffer = [0 as c_char; 256];
            let copied =
                CFStringGetCString(value, buffer.as_mut_ptr(), buffer.len() as isize, UTF8);
            (copied != 0).then(|| {
                let text = CStr::from_ptr(buffer.as_ptr());
                Property::Text(text.to_string_lossy().into_owned())
            })
        } else if kind == CFDataGetTypeID() {
            let length = usize::try_from(CFDataGetLength(value)).ok()?;
            let bytes = CFDataGetBytePtr(value);
            if bytes.is_null() {
                return None;
            }
            Some(Property::Data(
                std::slice::from_raw_parts(bytes, length).to_vec(),
            ))
        } else {
            None
        }
    }
}
