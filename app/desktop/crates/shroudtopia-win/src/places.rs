//! Where things are on this PC: Steam's folder (HKCU\Software\Valve\Steam `SteamPath`, else the default), the
//! "Saved Games" known folder, and the app's data folder (`%APPDATA%\Shroudtopia`).

use std::path::PathBuf;

use shroudtopia_core::config::DATA_FOLDER_NAME;

pub const STEAM_DEFAULT: &str = r"C:\Program Files (x86)\Steam";

/// Steam's install folder, when it is there.
pub fn steam_root() -> Option<PathBuf> {
    let from_registry = crate::registry::read_text(r"Software\Valve\Steam", "SteamPath").map(|p| PathBuf::from(p.replace('/', "\\")));
    from_registry.into_iter().chain(std::iter::once(PathBuf::from(STEAM_DEFAULT))).find(|p| p.join("config").is_dir())
}

/// `%APPDATA%\Shroudtopia` (the temp folder when there is no APPDATA, as on Linux).
pub fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join(DATA_FOLDER_NAME)
}

/// The "Saved Games" folder (it can be moved by the user, so the shell is asked), else `%USERPROFILE%\Saved Games`.
pub fn saved_games() -> Option<PathBuf> {
    known_saved_games().or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Saved Games")))
}

#[cfg(windows)]
fn known_saved_games() -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_SavedGames, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    let mut out: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: the out pointer is freed with CoTaskMemFree, as the API requires, whether the call worked or not.
    unsafe {
        let hr = SHGetKnownFolderPath(&FOLDERID_SavedGames, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut out);
        let path = if hr >= 0 && !out.is_null() {
            let len = (0..).take_while(|&i| *out.add(i) != 0).count();
            Some(PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(out, len))))
        } else {
            None
        };
        CoTaskMemFree(out as *const core::ffi::c_void);
        path
    }
}

#[cfg(not(windows))]
fn known_saved_games() -> Option<PathBuf> {
    None
}
