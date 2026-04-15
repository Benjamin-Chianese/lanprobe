//! Thin wrappers around `std::process::Command` and `tokio::process::Command`
//! that hide the console window on Windows (CREATE_NO_WINDOW).
//!
//! Without this flag every subprocess call (netsh, ipconfig, ping, …) flashes
//! a cmd.exe window and steals focus from the WebView2 host, which makes the
//! UI flicker and feel laggy.

use std::ffi::OsStr;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn sync_cmd<S: AsRef<OsStr>>(program: S) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn async_cmd<S: AsRef<OsStr>>(program: S) -> tokio::process::Command {
    #[allow(unused_mut)]
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
