//! The little of the registry the app touches, all under `HKEY_CURRENT_USER`: Steam's `SteamPath`, and the Run
//! key's value for "Iniciar com o Windows". (tf3-mods' companion-win registry.rs.)

use std::io;

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::io;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn read_text(subkey: &str, value: &str) -> Option<String> {
        let (subkey, value) = (wide(subkey), wide(value));
        let mut size: u32 = 0;
        // SAFETY: NUL-terminated names; a null buffer asks only for the size.
        let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, subkey.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut size) };
        if status != ERROR_SUCCESS || size == 0 || size > 64 * 1024 {
            return None;
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2) + 1];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: the buffer holds `size` bytes and outlives the call.
        let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, subkey.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buffer.as_mut_ptr() as *mut c_void, &mut size) };
        if status != ERROR_SUCCESS {
            return None;
        }
        let length = buffer.iter().position(|unit| *unit == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..length]))
    }

    pub fn write_text(subkey: &str, value: &str, text: &str) -> io::Result<()> {
        let (subkey, value, text) = (wide(subkey), wide(value), wide(text));
        // SAFETY: NUL-terminated names and data; the size is the data's in bytes.
        let status = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, subkey.as_ptr(), value.as_ptr(), REG_SZ, text.as_ptr() as *const c_void, (text.len() * 2) as u32) };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status as i32))
        }
    }

    pub fn delete_value(subkey: &str, value: &str) -> io::Result<()> {
        let (subkey, value) = (wide(subkey), wide(value));
        // SAFETY: NUL-terminated names.
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey.as_ptr(), value.as_ptr()) };
        if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status as i32))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    pub fn read_text(_: &str, _: &str) -> Option<String> {
        None
    }
    pub fn write_text(_: &str, _: &str, _: &str) -> io::Result<()> {
        Err(io::Error::other("the registry is Windows'"))
    }
    pub fn delete_value(_: &str, _: &str) -> io::Result<()> {
        Ok(())
    }
}

pub fn read_text(subkey: &str, value: &str) -> Option<String> {
    imp::read_text(subkey, value)
}

pub fn write_text(subkey: &str, value: &str, text: &str) -> io::Result<()> {
    imp::write_text(subkey, value, text)
}

pub fn delete_value(subkey: &str, value: &str) -> io::Result<()> {
    imp::delete_value(subkey, value)
}
