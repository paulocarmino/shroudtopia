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

/// The last time (unix seconds) the game was seen running, or a look failed.
static LAST_SEEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// How long the game must have been closed before the app touches the game folder or starts Steam as the game:
/// right after the exit Steam is still syncing the game's cloud files and the game's files may still be held.
pub const SETTLE_SECS: u64 = 60;

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The game's process; a look that failed counts as running.
pub fn game_running() -> bool {
    let running = look(GAME_EXE).unwrap_or(true);
    if running {
        LAST_SEEN.store(now_secs(), std::sync::atomic::Ordering::Relaxed);
    }
    running
}

/// The game is closed and has been for `SETTLE_SECS`: the only state in which the app installs mods, swaps its own
/// exe or runs the Steam storage helper. Looked at right before each of those steps, never cached.
pub fn game_settled() -> bool {
    !game_running() && now_secs().saturating_sub(LAST_SEEN.load(std::sync::atomic::Ordering::Relaxed)) >= SETTLE_SECS
}

pub fn steam_running() -> bool {
    look(STEAM_EXE).unwrap_or(false)
}
