//! The few things superfetch needs from the terminal itself: its size, and
//! unbuffered key input for the interactive editor.

/// Columns and rows of the terminal on standard output, if it is one.
pub fn size() -> Option<(usize, usize)> {
    imp::size()
}

/// Makes sure escape sequences are interpreted. Only the legacy Windows
/// console needs to be told; everywhere else this does nothing.
pub fn enable_escape_sequences() {
    imp::enable_escape_sequences();
}

/// Puts the terminal into raw mode: keys arrive one at a time, unechoed.
/// Dropping the guard restores the previous state.
pub struct RawMode(imp::Saved);

impl RawMode {
    pub fn enter() -> std::io::Result<RawMode> {
        imp::enter_raw().map(RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        imp::restore(&self.0);
    }
}

#[cfg(unix)]
mod imp {
    use std::io;

    use crate::sys::c;

    pub type Saved = c::Termios;

    pub fn size() -> Option<(usize, usize)> {
        let mut size = c::Winsize::default();
        // SAFETY: TIOCGWINSZ writes a winsize through the pointer.
        let status = unsafe { c::ioctl(c::STDOUT, c::TIOCGWINSZ, &mut size) };
        (status == 0 && size.columns > 0).then_some((size.columns.into(), size.rows.into()))
    }

    pub fn enable_escape_sequences() {}

    pub fn enter_raw() -> io::Result<Saved> {
        let mut saved = c::Termios::zeroed();
        // SAFETY: the buffer is larger than any system's termios, and the
        // copy handed to cfmakeraw and tcsetattr was filled in by tcgetattr.
        unsafe {
            if c::tcgetattr(c::STDIN, &mut saved) != 0 {
                return Err(io::Error::last_os_error());
            }
            // Raw mode: no line buffering, no echo, and Ctrl-C arrives as
            // an ordinary byte. It also stops the terminal from turning
            // "\n" into a full line break, so the editor writes "\r\n".
            let mut raw = saved;
            c::cfmakeraw(&mut raw);
            if c::tcsetattr(c::STDIN, c::TCSAFLUSH, &raw) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(saved)
        }
    }

    pub fn restore(saved: &Saved) {
        // SAFETY: `saved` came from tcgetattr.
        unsafe { c::tcsetattr(c::STDIN, c::TCSAFLUSH, saved) };
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::mem::zeroed;

    use crate::sys::win32::*;

    pub type Saved = u32;

    pub fn size() -> Option<(usize, usize)> {
        // SAFETY: the info struct is plain data the call fills in.
        unsafe {
            let mut info: CONSOLE_SCREEN_BUFFER_INFO = zeroed();
            if GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &mut info) == 0 {
                return None;
            }
            let columns = info.srWindow.Right - info.srWindow.Left + 1;
            let rows = info.srWindow.Bottom - info.srWindow.Top + 1;
            Some((usize::try_from(columns).ok()?, usize::try_from(rows).ok()?))
        }
    }

    pub fn enable_escape_sequences() {
        // SAFETY: both calls only read or write the mode of a handle, and
        // fail harmlessly when it is not a console.
        unsafe {
            let output = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut mode = 0;
            if GetConsoleMode(output, &mut mode) != 0 {
                SetConsoleMode(output, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            }
        }
    }

    pub fn enter_raw() -> io::Result<Saved> {
        // SAFETY: as above.
        unsafe {
            let input = GetStdHandle(STD_INPUT_HANDLE);
            let mut saved = 0;
            if GetConsoleMode(input, &mut saved) == 0 {
                return Err(io::Error::last_os_error());
            }
            // With virtual terminal input the arrow keys arrive as the same
            // escape sequences a Unix terminal sends.
            let raw = (saved & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT))
                | ENABLE_VIRTUAL_TERMINAL_INPUT;
            if SetConsoleMode(input, raw) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(saved)
        }
    }

    pub fn restore(saved: &Saved) {
        // SAFETY: restores a mode previously read from the same handle.
        unsafe { SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), *saved) };
    }
}
