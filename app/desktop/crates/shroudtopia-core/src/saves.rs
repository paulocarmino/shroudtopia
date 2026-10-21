//! Where a shared world's `worldN.ksd` lives, behind one trait with two stores (a per-world setting chosen at
//! first run):
//!
//! - `LocalStore`: Enshrouded 1.0's local saves, plain files in `<Saved Games>\keen games\Enshrouded\default`.
//! - `SteamStore`: the Steam Cloud slots. Reading is the file on disk (`userdata/<id>/1203620/remote`); writing and
//!   deleting go through Steam's remote storage API (`RemoteStorage`, done by the Windows glue in a short-lived helper
//!   process with the game's own `steam_api64.dll`), because a file copied into that folder by hand is invisible
//!   in-game. Never while the game runs.
//!
//! Only `worldN.ksd` files are ever touched; every other file of either folder is left alone.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::config::Stat;
use crate::steam::{self, Slot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreKind {
    #[default]
    Steam,
    Local,
}

pub trait SaveStore: Send + Sync {
    fn kind(&self) -> StoreKind;
    /// The folder the files are in (for the window, and to tell a world's file apart).
    fn dir(&self) -> &Path;
    fn list(&self) -> Vec<Slot> {
        steam::list_slots(self.dir())
    }
    fn stat(&self, slot: u8) -> Option<Stat> {
        Stat::of(&self.dir().join(steam::slot_file(slot)))
    }
    fn read(&self, slot: u8) -> io::Result<Vec<u8>> {
        fs::read(self.dir().join(steam::slot_file(slot)))
    }
    fn exists(&self, slot: u8) -> bool {
        self.dir().join(steam::slot_file(slot)).is_file()
    }
    /// The whole file, replaced at once.
    fn write(&self, slot: u8, bytes: &[u8]) -> io::Result<()>;
    fn delete(&self, slot: u8) -> io::Result<()>;
    /// The lowest slot with no world, not one of `taken`.
    fn free_slot(&self, taken: &[u8]) -> Option<u8> {
        (0..steam::SLOTS).find(|i| !taken.contains(i) && !self.exists(*i))
    }
}

pub struct LocalStore {
    dir: PathBuf,
}

impl LocalStore {
    pub fn new(dir: PathBuf) -> LocalStore {
        LocalStore { dir }
    }
}

impl SaveStore for LocalStore {
    fn kind(&self) -> StoreKind {
        StoreKind::Local
    }
    fn dir(&self) -> &Path {
        &self.dir
    }
    /// A temp file in the same folder, renamed over the slot: the game never sees half a world.
    fn write(&self, slot: u8, bytes: &[u8]) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let file = self.dir.join(steam::slot_file(slot));
        let tmp = self.dir.join(format!("{}.shroudtopia-tmp", steam::slot_file(slot)));
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, &file).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }
    fn delete(&self, slot: u8) -> io::Result<()> {
        match fs::remove_file(self.dir.join(steam::slot_file(slot))) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

/// Steam's remote storage, by file name (`world2.ksd`). The Windows glue's implementation starts the helper.
pub trait RemoteStorage: Send + Sync {
    fn write(&self, name: &str, bytes: &[u8]) -> io::Result<()>;
    fn delete(&self, name: &str) -> io::Result<()>;
}

pub struct SteamStore {
    dir: PathBuf,
    remote: Arc<dyn RemoteStorage>,
}

impl SteamStore {
    pub fn new(dir: PathBuf, remote: Arc<dyn RemoteStorage>) -> SteamStore {
        SteamStore { dir, remote }
    }
}

impl SaveStore for SteamStore {
    fn kind(&self) -> StoreKind {
        StoreKind::Steam
    }
    fn dir(&self) -> &Path {
        &self.dir
    }
    fn write(&self, slot: u8, bytes: &[u8]) -> io::Result<()> {
        self.remote.write(&steam::slot_file(slot), bytes)?;
        // Steam writes the file into the folder itself; check it is the bytes that were sent (it may take a moment)
        let file = self.dir.join(steam::slot_file(slot));
        for _ in 0..20 {
            if fs::read(&file).is_ok_and(|on_disk| on_disk == bytes) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Err(io::Error::other("o Steam aceitou o arquivo, mas ele não apareceu na pasta de saves"))
    }
    fn delete(&self, slot: u8) -> io::Result<()> {
        self.remote.delete(&steam::slot_file(slot))
    }
}

/// A `RemoteStorage` that writes into a folder, standing in for Steam in tests.
#[cfg(test)]
pub struct FolderRemote(pub PathBuf);

#[cfg(test)]
impl RemoteStorage for FolderRemote {
    fn write(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        fs::write(self.0.join(name), bytes)
    }
    fn delete(&self, name: &str) -> io::Result<()> {
        fs::remove_file(self.0.join(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_store_writes_whole_files_and_finds_free_slots() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(dir.path().join("default"));
        assert_eq!(store.free_slot(&[]), Some(0));
        store.write(0, b"world").unwrap();
        assert_eq!(store.read(0).unwrap(), b"world");
        assert_eq!(store.stat(0).unwrap().size, 5);
        assert_eq!(store.free_slot(&[1]), Some(2));
        assert_eq!(store.list().len(), 1);
        assert!(!store.dir().join("world0.ksd.shroudtopia-tmp").exists());
        store.delete(0).unwrap();
        store.delete(0).unwrap();
        assert!(!store.exists(0));
    }

    #[test]
    fn steam_store_writes_through_remote_storage_and_checks_the_result() {
        let dir = tempfile::tempdir().unwrap();
        let store = SteamStore::new(dir.path().into(), Arc::new(FolderRemote(dir.path().into())));
        store.write(2, b"abc").unwrap();
        assert_eq!(store.read(2).unwrap(), b"abc");
        struct Liar;
        impl RemoteStorage for Liar {
            fn write(&self, _: &str, _: &[u8]) -> io::Result<()> {
                Ok(())
            }
            fn delete(&self, _: &str) -> io::Result<()> {
                Ok(())
            }
        }
        let lying = SteamStore::new(dir.path().into(), Arc::new(Liar));
        assert!(lying.write(3, b"x").is_err(), "a write Steam did not make is not taken as done");
    }
}
