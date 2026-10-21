//! Shroudtopia's Windows glue: what the core needs from Windows and cannot read from files. Every module compiles on
//! Linux too (the parts that call Win32 are `cfg(windows)`, with a stand-in elsewhere), so `cargo test` of the
//! workspace runs in the Linux container.

pub mod places;
pub mod process;
pub mod registry;
pub mod steam_remote;

pub use places::*;

/// "Iniciar com o Windows": the value in HKCU's Run key, `"<exe>" --minimized`.
pub mod autostart {
    use crate::registry;
    use std::io;

    pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    pub const VALUE: &str = "Shroudtopia";
    pub const MINIMIZED: &str = "--minimized";

    pub fn command() -> io::Result<String> {
        let exe = std::env::current_exe()?;
        let exe = exe.to_str().ok_or_else(|| io::Error::other("the executable's path is not text"))?;
        if exe.contains('"') {
            return Err(io::Error::other("a quote in the executable's path"));
        }
        Ok(format!("\"{exe}\" {MINIMIZED}"))
    }

    /// On only when the value is there and names this executable.
    pub fn is_on() -> bool {
        match (registry::read_text(RUN_KEY, VALUE), command()) {
            (Some(stored), Ok(ours)) => stored.eq_ignore_ascii_case(&ours),
            _ => false,
        }
    }

    pub fn set(on: bool) -> io::Result<()> {
        if on {
            registry::write_text(RUN_KEY, VALUE, &command()?)
        } else {
            registry::delete_value(RUN_KEY, VALUE)
        }
    }
}
