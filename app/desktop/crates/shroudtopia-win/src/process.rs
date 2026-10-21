//! Whether a process is running, by its executable's name (`enshrouded.exe`, `steam.exe`). Only names are read;
//! no process is opened. (tf3-mods' companion-win process.rs.)

pub const GAME_EXE: &str = shroudtopia_core::steam::GAME_EXE;
pub const STEAM_EXE: &str = "steam.exe";

/// `Some(true/false)`; `None` when the snapshot could not be made (the caller treats it as "running", the safe side
/// for a pull or an install).
#[cfg(windows)]
pub fn look(exe: &str) -> Option<bool> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    // SAFETY: the snapshot is this process's handle, closed below; the entry is plain data with its size set.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry);
        if more == 0 {
            CloseHandle(snapshot);
            return None;
        }
        let mut found = false;
        while more != 0 && !found {
            let len = entry.szExeFile.iter().position(|u| *u == 0).unwrap_or(entry.szExeFile.len());
            found = String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(exe);
            more = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        Some(found)
    }
}

#[cfg(not(windows))]
pub fn look(_exe: &str) -> Option<bool> {
    Some(false)
}

/// The game's process; a look that failed counts as running.
pub fn game_running() -> bool {
    look(GAME_EXE).unwrap_or(true)
}

pub fn steam_running() -> bool {
    look(STEAM_EXE).unwrap_or(false)
}
