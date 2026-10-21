//! What the app leaves behind when something goes wrong on a PC nobody here can look at: `crash.log` (a panic, which
//! the release build turns into an abort with no window) and `shroudtopia.log` (one line per start: the exe, the game
//! folder, whether it runs as administrator). Both in `%APPDATA%\Shroudtopia`, both small and plain text, so a friend
//! can send them over a chat. Also the guard for an exe opened from inside the downloaded .zip.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The logs are cut back to their last part past this size.
const MAX_LOG: u64 = 256 * 1024;

fn append(name: &str, text: &str) {
    let dir = crate::places::data_dir();
    let path = dir.join(name);
    let _ = fs::create_dir_all(&dir);
    if fs::metadata(&path).map(|m| m.len() > MAX_LOG).unwrap_or(false) {
        if let Ok(old) = fs::read(&path) {
            let _ = fs::write(&path, &old[old.len() - (MAX_LOG as usize / 2)..]);
        }
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(text.as_bytes());
    }
}

fn stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z").to_string()
}

#[cfg(windows)]
pub fn elevated() -> bool {
    // SAFETY: no arguments, reads this process's token.
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}
#[cfg(not(windows))]
pub fn elevated() -> bool {
    false
}

fn exe() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|e| format!("? ({e})"))
}

/// Every panic goes to `crash.log` before the process ends.
pub fn install_panic_log() {
    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current().name().unwrap_or("?").to_string();
        let text = format!(
            "\n=== {} · Shroudtopia {} · crash\nexe: {}\nadmin: {}\nthread: {thread}\n{info}\n{}\n",
            stamp(),
            shroudtopia_core::APP_VERSION,
            exe(),
            elevated(),
            std::backtrace::Backtrace::force_capture()
        );
        append("crash.log", &text);
    }));
}

/// One line per start, and anything else worth a line (`note`).
pub fn note(text: &str) {
    append("shroudtopia.log", &format!("{} · {text}\n", stamp()));
}

pub fn note_start(game_folder: Option<&Path>) {
    note(&format!(
        "start {} · exe {} · admin {} · game {}",
        shroudtopia_core::APP_VERSION,
        exe(),
        elevated(),
        game_folder.map(|g| g.display().to_string()).unwrap_or_else(|| "não achada".into())
    ));
}

/// The exe runs from the temp folder: Explorer opened it straight from inside the .zip. Updates and the Steam helper
/// need it to live in a real folder, so the app says so and stops.
pub fn running_from_temp() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let temp = std::env::temp_dir();
    let lower = |p: &Path| p.to_string_lossy().to_lowercase();
    lower(&exe).starts_with(&lower(&temp)).then_some(exe)
}

#[cfg(windows)]
pub fn message(title: &str, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (t, m) = (w(title), w(text));
    // SAFETY: both strings are NUL-terminated and live until the call returns.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONWARNING);
    }
}
#[cfg(not(windows))]
pub fn message(_: &str, _: &str) {}
