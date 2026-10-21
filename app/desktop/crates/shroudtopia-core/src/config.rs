//! The app's own config, `%APPDATA%\Shroudtopia\config.json`: the server, the device token, the shared world and its
//! local slot, the version the local copy came from, the offline queue, the mods' installed version. Written whole
//! (temp file renamed over) on every change.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::contract::Player;

pub const SERVER_URL: &str = "https://shroudtopia.paulocarmino.com";
pub const DATA_FOLDER_NAME: &str = "Shroudtopia";

/// Size and modification time of a file, to tell "unchanged" without hashing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stat {
    pub size: u64,
    /// milliseconds since the epoch
    pub mtime: i64,
}

impl Stat {
    pub fn of(path: &Path) -> Option<Stat> {
        let meta = fs::metadata(path).ok().filter(|m| m.is_file())?;
        let mtime = meta.modified().ok().map(|t| DateTime::<Utc>::from(t).timestamp_millis()).unwrap_or(0);
        Some(Stat { size: meta.len(), mtime })
    }
}

/// A session's upload that did not reach the server yet (offline, or the lease was someone else's): a snapshot copy
/// of the file in the app's queue folder, with the headers it goes up with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub file: PathBuf,
    pub sha256: String,
    pub size: u64,
    pub parent: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

/// The running game session, kept so an app restarted mid-game picks it up again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub last_autosave_at: Option<DateTime<Utc>>,
}

/// The shared world of this PC and where it lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldLink {
    pub world_id: String,
    pub name: String,
    /// which save folder the world lives in (saves.rs)
    #[serde(default)]
    pub store: crate::saves::StoreKind,
    /// `world<slot>.ksd` in that store
    pub slot: u8,
    /// the version the local file is (pulled, or uploaded from it); None before the first
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub base_sha256: Option<String>,
    #[serde(default)]
    pub base_stat: Option<Stat>,
    #[serde(default)]
    pub pending: Option<Pending>,
    /// this PC's upload that the server kept as a branch (diverged), until resolved
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub session: Option<Session>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub server_url: String,
    pub token: Option<String>,
    pub player: Option<Player>,
    pub avatar_sent: bool,
    pub world: Option<WorldLink>,
    /// chosen by hand ("Trocar"); otherwise found through Steam
    pub game_folder: Option<PathBuf>,
    /// the release tag of the mods installed by this app
    pub mods_version: Option<String>,
    pub last_update_check: Option<DateTime<Utc>>,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            server_url: SERVER_URL.into(),
            token: None,
            player: None,
            avatar_sent: false,
            world: None,
            game_folder: None,
            mods_version: None,
            last_update_check: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(data_dir: &Path) -> ConfigStore {
        ConfigStore { path: data_dir.join("config.json") }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The config on disk; the default when there is none. A broken file is kept aside (`config.json.broken`) and
    /// the default used, so the app still opens.
    pub fn load(&self) -> AppConfig {
        match fs::read_to_string(&self.path) {
            Ok(text) => match serde_json::from_str::<AppConfig>(&text) {
                Ok(mut cfg) => {
                    if cfg.server_url.trim().is_empty() {
                        cfg.server_url = SERVER_URL.into();
                    }
                    cfg
                }
                Err(_) => {
                    let _ = fs::rename(&self.path, self.path.with_extension("json.broken"));
                    AppConfig::default()
                }
            },
            Err(_) => AppConfig::default(),
        }
    }

    pub fn save(&self, cfg: &AppConfig) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(cfg).map_err(io::Error::other)?)?;
        fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_survives_a_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        assert_eq!(store.load(), AppConfig::default());
        let mut cfg = AppConfig::default();
        cfg.token = Some("dev_x".into());
        cfg.world = Some(WorldLink { world_id: "w".into(), name: "n".into(), store: Default::default(), slot: 2, base: Some("v".into()), base_sha256: None, base_stat: Some(Stat { size: 1, mtime: 2 }), pending: None, branch: None, session: None });
        store.save(&cfg).unwrap();
        assert_eq!(store.load(), cfg);
        fs::write(store.path(), "{").unwrap();
        assert_eq!(store.load(), AppConfig::default());
        assert!(dir.path().join("config.json.broken").exists());
        // an old config with fields missing reads with defaults
        fs::write(store.path(), r#"{"token":"t"}"#).unwrap();
        assert_eq!(store.load().server_url, SERVER_URL);
    }
}
