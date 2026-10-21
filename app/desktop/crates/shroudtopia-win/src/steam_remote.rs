//! Writing a Steam Cloud slot (`world2.ksd`) the way the game would: through `ISteamRemoteStorage`, with the game's
//! own `steam_api64.dll`. A file copied into `userdata/<id>/1203620/remote/` by hand is invisible in-game; one
//! written with `FileWrite` shows in "Jogos salvos: Steam" (tested by the coordinator, 2026-10-21).
//!
//! It runs in a short-lived helper process, this same executable started with `--steam-remote`: while the Steam API
//! is up Steam shows the player as "playing Enshrouded", so the helper inits, does one thing and shuts down within
//! seconds, and its process ending guarantees the API is gone. Never while `enshrouded.exe` runs, and only while
//! Steam itself runs.
//!
//! ```text
//! Shroudtopia.exe --steam-remote write  <game folder> <world2.ksd> <file with the bytes>
//! Shroudtopia.exe --steam-remote delete <game folder> <world2.ksd>
//! ```
//!
//! Exit code 0 is done; anything else failed (the reason on stderr, for the log).

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use shroudtopia_core::saves::RemoteStorage;
use shroudtopia_core::steam;

pub const FLAG: &str = "--steam-remote";

/// The parent's side: a `RemoteStorage` that starts the helper.
pub struct HelperRemote {
    pub exe: PathBuf,
    pub game_dir: PathBuf,
    pub temp_dir: PathBuf,
}

impl HelperRemote {
    fn run(&self, args: &[&std::ffi::OsStr]) -> io::Result<()> {
        if crate::process::game_running() {
            return Err(io::Error::other("o Enshrouded está aberto"));
        }
        if !crate::process::steam_running() {
            return Err(io::Error::other("o Steam não está aberto; abra o Steam e tente de novo"));
        }
        if !self.game_dir.join("steam_api64.dll").is_file() {
            return Err(io::Error::other("o steam_api64.dll do jogo não foi achado na pasta do jogo"));
        }
        let mut cmd = Command::new(&self.exe);
        cmd.arg(FLAG).args(args).env("SteamAppId", steam::APP_ID).env("SteamGameId", steam::APP_ID).current_dir(&self.game_dir);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn()?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait()? {
                return if status.success() { Ok(()) } else { Err(io::Error::other(format!("o Steam recusou a gravação (código {})", status.code().unwrap_or(-1)))) };
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                return Err(io::Error::other("o Steam não respondeu a tempo"));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl RemoteStorage for HelperRemote {
    fn write(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        if steam::slot_index(name).is_none() {
            return Err(io::Error::other("only worldN.ksd"));
        }
        std::fs::create_dir_all(&self.temp_dir)?;
        let tmp = self.temp_dir.join(format!("{name}.steam-write"));
        std::fs::write(&tmp, bytes)?;
        let r = self.run(&["write".as_ref(), self.game_dir.as_os_str(), name.as_ref(), tmp.as_os_str()]);
        let _ = std::fs::remove_file(&tmp);
        r
    }

    fn delete(&self, name: &str) -> io::Result<()> {
        if steam::slot_index(name).is_none() {
            return Err(io::Error::other("only worldN.ksd"));
        }
        self.run(&["delete".as_ref(), self.game_dir.as_os_str(), name.as_ref()])
    }
}

/// The helper's side: `args` are what follows `--steam-remote`. Returns the exit code.
pub fn helper_main(args: &[String]) -> i32 {
    match helper(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("steam-remote: {e}");
            1
        }
    }
}

fn helper(args: &[String]) -> Result<(), String> {
    let (op, game, name) = match args {
        [op, game, name, ..] => (op.as_str(), Path::new(game), name.as_str()),
        _ => return Err("usage: --steam-remote write|delete <game folder> <worldN.ksd> [file]".into()),
    };
    if steam::slot_index(name).is_none() {
        return Err("only worldN.ksd".into());
    }
    let bytes = match (op, args.get(3)) {
        ("write", Some(file)) => Some(std::fs::read(file).map_err(|e| e.to_string())?),
        ("delete", _) => None,
        _ => return Err("unknown operation".into()),
    };
    imp::with_remote_storage(&game.join("steam_api64.dll"), |rs| match &bytes {
        Some(b) => rs.write(name, b),
        None => rs.delete(name),
    })
}

#[cfg(windows)]
mod imp {
    use std::ffi::{c_char, c_void, CString};
    use std::path::Path;
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    type InitFlat = unsafe extern "C" fn(*mut c_char) -> i32;
    type GetRs = unsafe extern "C" fn() -> *mut c_void;
    type FileWrite = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_void, i32) -> u8;
    type FileDelete = unsafe extern "C" fn(*mut c_void, *const c_char) -> u8;
    type Shutdown = unsafe extern "C" fn();

    pub struct Rs {
        this: *mut c_void,
        write: FileWrite,
        delete: FileDelete,
    }

    impl Rs {
        pub fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
            let n = CString::new(name).map_err(|e| e.to_string())?;
            let len = i32::try_from(bytes.len()).map_err(|_| "file too big")?;
            // SAFETY: a live ISteamRemoteStorage, a NUL-terminated name, and a buffer of `len` bytes.
            let ok = unsafe { (self.write)(self.this, n.as_ptr(), bytes.as_ptr() as *const c_void, len) };
            if ok != 0 {
                Ok(())
            } else {
                Err("FileWrite returned false".into())
            }
        }
        pub fn delete(&self, name: &str) -> Result<(), String> {
            let n = CString::new(name).map_err(|e| e.to_string())?;
            // SAFETY: a live ISteamRemoteStorage and a NUL-terminated name.
            let ok = unsafe { (self.delete)(self.this, n.as_ptr()) };
            if ok != 0 {
                Ok(())
            } else {
                Err("FileDelete returned false".into())
            }
        }
    }

    unsafe fn proc<T>(module: *mut c_void, name: &str) -> Result<T, String> {
        let c = CString::new(name).unwrap();
        match GetProcAddress(module, c.as_ptr() as *const u8) {
            Some(f) => Ok(std::mem::transmute_copy(&f)),
            None => Err(format!("{name} is not in steam_api64.dll")),
        }
    }

    pub fn with_remote_storage(dll: &Path, f: impl FnOnce(&Rs) -> Result<(), String>) -> Result<(), String> {
        let wide: Vec<u16> = dll.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: the game's own Steam API DLL, loaded by full path; the functions are the flat API's, called with
        // the signatures of the SDK the game ships (STEAMREMOTESTORAGE_INTERFACE_VERSION016).
        unsafe {
            let module = LoadLibraryW(wide.as_ptr());
            if module.is_null() {
                return Err(format!("could not load {}", dll.display()));
            }
            let init: InitFlat = proc(module, "SteamAPI_InitFlat")?;
            let get: GetRs = proc(module, "SteamAPI_SteamRemoteStorage_v016")?;
            let write: FileWrite = proc(module, "SteamAPI_ISteamRemoteStorage_FileWrite")?;
            let delete: FileDelete = proc(module, "SteamAPI_ISteamRemoteStorage_FileDelete")?;
            let shutdown: Shutdown = proc(module, "SteamAPI_Shutdown")?;
            let mut err = [0 as c_char; 1024];
            let code = init(err.as_mut_ptr());
            if code != 0 {
                let msg = std::ffi::CStr::from_ptr(err.as_ptr()).to_string_lossy().into_owned();
                return Err(format!("SteamAPI_InitFlat {code}: {msg}"));
            }
            let this = get();
            let r = if this.is_null() { Err("no ISteamRemoteStorage".into()) } else { f(&Rs { this, write, delete }) };
            shutdown();
            r
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;
    pub struct Rs;
    impl Rs {
        pub fn write(&self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("Steam is Windows'".into())
        }
        pub fn delete(&self, _: &str) -> Result<(), String> {
            Err("Steam is Windows'".into())
        }
    }
    pub fn with_remote_storage(_: &Path, _: impl FnOnce(&Rs) -> Result<(), String>) -> Result<(), String> {
        Err("Steam is Windows'".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_helper_refuses_anything_but_a_world_slot() {
        assert_ne!(helper_main(&["write".into(), "/nowhere".into(), "global.ksd".into(), "/x".into()]), 0);
        assert_ne!(helper_main(&["write".into(), "/nowhere".into(), "../world1.ksd".into(), "/x".into()]), 0);
        assert_ne!(helper_main(&["format".into(), "/nowhere".into(), "world1.ksd".into()]), 0);
        assert_ne!(helper_main(&[]), 0);
        let r = HelperRemote { exe: "/bin/false".into(), game_dir: "/nowhere".into(), temp_dir: std::env::temp_dir() };
        assert!(r.write("character0.ksd", b"x").is_err());
    }
}
