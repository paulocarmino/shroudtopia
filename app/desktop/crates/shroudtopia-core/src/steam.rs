//! What the app reads of Steam, from its install folder (found by the Windows glue in the registry): the account
//! logged in (steamid64 + nick), its avatar, Enshrouded's save folder and world slots, and the game's folder. Only
//! reads; nothing here writes into Steam's folders.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::vdf;

pub const APP_ID: &str = "1203620";
pub const GAME_EXE: &str = "enshrouded.exe";
/// steamid64 = this + the account id (the folder name under `userdata`).
pub const STEAMID64_BASE: u64 = 76_561_197_960_265_728;
/// World slots the game knows: `world0.ksd` … `world9.ksd`.
pub const SLOTS: u8 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamUser {
    pub steam_id: String,
    pub nick: String,
    pub account_id: u64,
}

/// One world save of this PC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub index: u8,
    pub file: String,
    pub size: u64,
    pub modified: DateTime<Utc>,
}

pub fn slot_file(index: u8) -> String {
    format!("world{index}.ksd")
}

/// `world3.ksd` -> 3.
pub fn slot_index(file: &str) -> Option<u8> {
    let n: u8 = file.strip_prefix("world")?.strip_suffix(".ksd")?.parse().ok()?;
    (n < SLOTS && file == slot_file(n)).then_some(n)
}

pub fn account_id(steam_id: &str) -> Option<u64> {
    steam_id.parse::<u64>().ok()?.checked_sub(STEAMID64_BASE)
}

/// The account of `config/loginusers.vdf` marked `"MostRecent" "1"`; when none is marked (Steam leaves it out with a
/// single account), the one with the latest `timestamp`.
pub fn current_user(steam_root: &Path) -> Option<SteamUser> {
    let text = fs::read_to_string(steam_root.join("config").join("loginusers.vdf")).ok()?;
    let root = vdf::parse(&text)?;
    let users = root.get("users")?;
    let mut best: Option<(&str, &vdf::Node, (bool, u64))> = None;
    for (id, node) in users.entries() {
        if account_id(id).is_none() {
            continue;
        }
        let rank = (node.text("MostRecent") == Some("1"), node.text("timestamp").and_then(|t| t.parse().ok()).unwrap_or(0));
        if best.as_ref().is_none_or(|b| rank > b.2) {
            best = Some((id, node, rank));
        }
    }
    let (id, node, _) = best?;
    let nick = node.text("PersonaName").or_else(|| node.text("AccountName")).unwrap_or(id).to_string();
    Some(SteamUser { steam_id: id.to_string(), nick, account_id: account_id(id)? })
}

pub fn avatar_path(steam_root: &Path, steam_id: &str) -> PathBuf {
    steam_root.join("config").join("avatarcache").join(format!("{steam_id}.png"))
}

/// `userdata/<accountId>/1203620/remote`, where Enshrouded keeps its saves (Steam Cloud's folder).
pub fn save_dir(steam_root: &Path, user: &SteamUser) -> PathBuf {
    steam_root.join("userdata").join(user.account_id.to_string()).join(APP_ID).join("remote")
}

fn modified(meta: &fs::Metadata) -> DateTime<Utc> {
    meta.modified().map(DateTime::<Utc>::from).unwrap_or_default()
}

/// The world slots in a save folder, by index.
pub fn list_slots(save_dir: &Path) -> Vec<Slot> {
    (0..SLOTS)
        .filter_map(|index| {
            let file = slot_file(index);
            let meta = fs::metadata(save_dir.join(&file)).ok().filter(|m| m.is_file())?;
            Some(Slot { index, file, size: meta.len(), modified: modified(&meta) })
        })
        .collect()
}

/// The lowest slot with no world file, and not one of `taken` (slots already mapped to a shared world).
pub fn free_slot(save_dir: &Path, taken: &[u8]) -> Option<u8> {
    (0..SLOTS).find(|i| !taken.contains(i) && !save_dir.join(slot_file(*i)).exists())
}

/// The libraries of `steamapps/libraryfolders.vdf` (Steam's own folder first).
pub fn libraries(steam_root: &Path) -> Vec<PathBuf> {
    let mut out = vec![steam_root.to_path_buf()];
    if let Some(root) = fs::read_to_string(steam_root.join("steamapps").join("libraryfolders.vdf")).ok().and_then(|t| vdf::parse(&t)) {
        if let Some(folders) = root.get("libraryfolders") {
            for (_, node) in folders.entries() {
                if let Some(path) = node.text("path") {
                    let p = PathBuf::from(path);
                    if !out.iter().any(|o| same_path(o, &p)) {
                        out.push(p);
                    }
                }
            }
        }
    }
    out
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}

/// Enshrouded's folder: the library with `appmanifest_1203620.acf`, `steamapps/common/<installdir>`.
pub fn game_folder(steam_root: &Path) -> Option<PathBuf> {
    for lib in libraries(steam_root) {
        let apps = lib.join("steamapps");
        let Ok(text) = fs::read_to_string(apps.join(format!("appmanifest_{APP_ID}.acf"))) else { continue };
        let install = vdf::parse(&text).and_then(|r| r.get("AppState").and_then(|s| s.text("installdir")).map(str::to_string)).unwrap_or_else(|| "Enshrouded".into());
        let folder = apps.join("common").join(install);
        if folder.is_dir() {
            return Some(folder);
        }
    }
    None
}

/// Whether a folder looks like Enshrouded's (the game's executable is in it).
pub fn is_game_folder(folder: &Path) -> bool {
    folder.join(GAME_EXE).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Steam folder like Paulo's: one account without MostRecent, the game in a second library.
    fn steam_fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let steam = dir.path().join("Steam");
        let lib = dir.path().join("SteamLibrary");
        fs::create_dir_all(steam.join("config/avatarcache")).unwrap();
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        fs::write(steam.join("config/loginusers.vdf"), include_str!("../fixtures/loginusers.vdf")).unwrap();
        let folders = include_str!("../fixtures/libraryfolders.vdf").replace("E:\\\\SteamLibrary", &lib.to_string_lossy().replace('\\', "\\\\"));
        fs::write(steam.join("steamapps/libraryfolders.vdf"), folders).unwrap();
        fs::create_dir_all(lib.join("steamapps/common/Enshrouded")).unwrap();
        fs::write(lib.join("steamapps/appmanifest_1203620.acf"), include_str!("../fixtures/appmanifest_1203620.acf")).unwrap();
        fs::write(lib.join("steamapps/common/Enshrouded/enshrouded.exe"), b"MZ").unwrap();
        (dir, steam)
    }

    #[test]
    fn finds_the_user_the_saves_and_the_game() {
        let (dir, steam) = steam_fixture();
        let user = current_user(&steam).unwrap();
        assert_eq!(user.steam_id, "76561198065863968");
        assert_eq!(user.nick, "[ツ] Ｌ　Ａ　Ｒ　Ａ　Ｎ　Ｊ　Ｏ");
        assert_eq!(user.account_id, 105_598_240);
        assert!(save_dir(&steam, &user).ends_with("userdata/105598240/1203620/remote") || save_dir(&steam, &user).ends_with(r"userdata\105598240\1203620\remote"));
        assert!(avatar_path(&steam, &user.steam_id).ends_with("76561198065863968.png"));
        let game = game_folder(&steam).unwrap();
        assert_eq!(game, dir.path().join("SteamLibrary/steamapps/common/Enshrouded"));
        assert!(is_game_folder(&game));
    }

    #[test]
    fn most_recent_wins_over_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("config")).unwrap();
        fs::write(
            dir.path().join("config/loginusers.vdf"),
            "\"users\"{\"76561198000000001\"{\"PersonaName\" \"a\" \"MostRecent\" \"1\" \"timestamp\" \"5\"}\"76561198000000002\"{\"PersonaName\" \"b\" \"timestamp\" \"9\"}}",
        )
        .unwrap();
        assert_eq!(current_user(dir.path()).unwrap().nick, "a");
        assert!(current_user(&dir.path().join("none")).is_none());
    }

    #[test]
    fn lists_world_slots_only() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["world0.ksd", "world2.ksd", "global.ksd", "character0.ksd", "world10.ksd", "world01.ksd"] {
            fs::write(dir.path().join(f), b"x").unwrap();
        }
        let slots = list_slots(dir.path());
        assert_eq!(slots.iter().map(|s| s.index).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(free_slot(dir.path(), &[]), Some(1));
        assert_eq!(free_slot(dir.path(), &[1]), Some(3));
        assert_eq!(slot_index("world7.ksd"), Some(7));
        assert_eq!(slot_index("world07.ksd"), None);
        assert_eq!(slot_index("global.ksd"), None);
    }
}
