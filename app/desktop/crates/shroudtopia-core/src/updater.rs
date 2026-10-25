//! Updates from the repository's GitHub Releases (`paulocarmino/shroudtopia` is public): the latest release carries
//! `shroudtopia-release-v2.json` (the signed manifest, signature.rs), `shroudtopia-mods-v2.zip` (loaders + mods) and
//! `Shroudtopia.exe`. Nothing is installed that the manifest does not name by size and sha256.
//!
//! - The mods go into the game folder only while `enshrouded.exe` is closed (its DLLs are locked while it runs);
//!   with the game open the download waits ("Jogo aberto") and installs when it closes. `shroudtopia.json` is never
//!   touched; every replaced file is kept in the app's backup folder.
//! - The app swaps itself as tf3-mods' companion does: the running `.exe` is renamed to `Shroudtopia.old.exe`, the
//!   checked download takes its name, the new one starts with `--after-update` and deletes the old one.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::format;
use crate::signature::{self, Manifest, SignedFile, EXE_ASSET, MANIFEST_ASSET, MODS_ASSET};

pub const LATEST_URL: &str = "https://api.github.com/repos/paulocarmino/shroudtopia/releases/latest";
/// the recent releases, for the notes of the versions an update skips
pub const RELEASES_URL: &str = "https://api.github.com/repos/paulocarmino/shroudtopia/releases?per_page=30";
pub const PARTIAL_NAME: &str = "Shroudtopia.exe.partial";
pub const OLD_NAME: &str = "Shroudtopia.old.exe";
pub const AFTER_UPDATE: &str = "--after-update";
/// GitHub answers 60 unauthenticated requests an hour: the app looks at most this often by itself.
pub const CHECK_EVERY: chrono::Duration = chrono::Duration::hours(1);

#[derive(Debug, Clone, Deserialize)]
pub struct GhAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GhRelease {
    pub tag_name: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub assets: Vec<GhAsset>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
}

/// One version's release notes (its bullet lines).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Changes {
    pub version: String,
    pub items: Vec<String>,
}

/// A release that verified: what it installs and where to get it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub manifest: Manifest,
    pub mods_url: String,
    pub exe_url: String,
    /// the release notes of this version and of the ones between it and what is installed, newest first
    pub changes: Vec<Changes>,
}

#[derive(Debug, Clone, Deserialize)]
struct SignedManifest {
    payload: String,
    signature: String,
}

/// The bullet lines ("- x", "* x") of a release's notes.
pub fn changes(body: &str) -> Vec<String> {
    body.lines().filter_map(|l| l.trim().strip_prefix("- ").or_else(|| l.trim().strip_prefix("* "))).map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).take(8).collect()
}

/// The offer of a GitHub release whose manifest (`manifest_json`) is signed by `key`.
pub fn offer(gh: &GhRelease, manifest_json: &str, key: &str) -> Result<Offer, String> {
    let signed: SignedManifest = serde_json::from_str(manifest_json).map_err(|_| signature::BAD_MANIFEST.to_string())?;
    let manifest = signature::verify(&signed.payload, &signed.signature, key).map_err(str::to_string)?;
    if gh.tag_name.trim_start_matches('v') != manifest.version {
        return Err("a versão assinada não é a do release".into());
    }
    let url = |name: &str| gh.assets.iter().find(|a| a.name == name).map(|a| a.browser_download_url.clone()).ok_or(format!("o release não tem {name}"));
    let notes = Changes { version: manifest.version.clone(), items: changes(gh.body.as_deref().unwrap_or("")) };
    Ok(Offer { mods_url: url(MODS_ASSET)?, exe_url: url(EXE_ASSET)?, changes: vec![notes].into_iter().filter(|c| !c.items.is_empty()).collect(), manifest })
}

/// Adds the notes of the published releases newer than `installed` and older than the offer, so an update that skips
/// versions still tells what each of them brought.
pub fn add_skipped(offer: &mut Offer, releases: &[GhRelease], installed: &str) {
    for r in releases.iter().filter(|r| !r.draft && !r.prerelease) {
        let v = r.tag_name.trim_start_matches('v');
        if signature::newer(v, installed) && signature::newer(&offer.manifest.version, v) {
            let items = changes(r.body.as_deref().unwrap_or(""));
            if !items.is_empty() && !offer.changes.iter().any(|c| c.version == v) {
                offer.changes.push(Changes { version: v.into(), items });
            }
        }
    }
    offer.changes.sort_by(|a, b| if signature::newer(&a.version, &b.version) { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater });
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout_read(Duration::from_secs(120)).user_agent(&format!("Shroudtopia/{}", crate::APP_VERSION)).build()
}

/// The latest release, verified, with the notes of the versions since `installed`; `None` when the repository has
/// none yet (GitHub's 404).
pub fn fetch_latest(key: &str, installed: Option<&str>) -> Result<Option<Offer>, String> {
    let agent = agent();
    let gh: GhRelease = match agent.get(LATEST_URL).set("Accept", "application/vnd.github+json").call() {
        Ok(r) => r.into_json().map_err(|e| format!("resposta estranha do GitHub: {e}"))?,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(ureq::Error::Status(s, _)) => return Err(format!("o GitHub respondeu {s}")),
        Err(e) => return Err(format!("sem conexão com o GitHub ({e})")),
    };
    let manifest_url = gh.assets.iter().find(|a| a.name == MANIFEST_ASSET).map(|a| a.browser_download_url.clone()).ok_or("o release não tem a assinatura")?;
    let text = agent.get(&manifest_url).call().map_err(|e| format!("não deu para baixar a assinatura ({e})"))?.into_string().map_err(|e| e.to_string())?;
    let mut found = offer(&gh, &text, key)?;
    // the skipped versions' notes are a nicety: without them the latest's notes still show
    if let Some(installed) = installed {
        if let Ok(list) = agent.get(RELEASES_URL).set("Accept", "application/vnd.github+json").call().map_err(|_| ()).and_then(|r| r.into_json::<Vec<GhRelease>>().map_err(|_| ())) {
            add_skipped(&mut found, &list, installed);
        }
    }
    Ok(Some(found))
}

/// Reads `body` into `dest`, hashing it as it comes; kept only when it is the signed file's very size and sha256
/// (otherwise removed). `progress(done, total)`.
pub fn receive(body: &mut dyn Read, dest: &Path, signed: &SignedFile, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    let r = (|| {
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut file = File::create(dest).map_err(|e| format!("não deu para gravar {}: {e}", dest.display()))?;
        let mut body = body.take(signed.size + 1);
        let mut hash = Sha256::new();
        let mut done = 0u64;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = match body.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("o download falhou ({e})")),
            };
            done += n as u64;
            if done > signed.size {
                return Err("o arquivo baixado não é o que o autor assinou".into());
            }
            hash.update(&buf[..n]);
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            progress(done, signed.size);
        }
        let got: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if done != signed.size || got != signed.sha256 {
            return Err("o arquivo baixado não é o que o autor assinou".into());
        }
        file.sync_all().map_err(|e| e.to_string())
    })();
    if r.is_err() {
        let _ = fs::remove_file(dest);
    }
    r
}

pub fn download(url: &str, dest: &Path, signed: &SignedFile, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    let resp = agent().get(url).call().map_err(|e| format!("o download falhou ({e})"))?;
    receive(&mut resp.into_reader(), dest, signed, progress)
}

/// The mods zip's layout this app installs (signature.rs, v2). `updates.json` keeps the one last installed: a PC whose
/// mods came from a v1 zip (an app up to 0.1.5) gets the v2 zip of the same release.
pub const MODS_LAYOUT: u32 = 2;

/// The files a mod folder may hold: the DLL, its manifest and Lua, and the minimap's data (images, shaders, tables).
const FOLDER_FILES: &[&str] = &["dll", "json", "lua", "txt", "md", "bin", "rgb", "rgba", "spv", "frag", "vert"];

/// Whether a path of the mods zip is one the app installs: the loaders (`shroudtopia.dll`, `winmm.dll`, EML's
/// `dbghelp.dll`), `mods/<name>.dll`, and the files of a mod folder (`mods/<folder>/...`, two levels deep at most,
/// with an extension in `FOLDER_FILES`). Anything else (another folder, `..`, the config) refuses the whole zip.
pub fn installable(path: &str) -> bool {
    let plain = |s: &str| !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.') && !s.starts_with('.');
    let folder_file = |s: &str| plain(s) && s.rsplit_once('.').is_some_and(|(_, ext)| FOLDER_FILES.contains(&ext.to_ascii_lowercase().as_str()));
    match path.split('/').collect::<Vec<_>>().as_slice() {
        [name] => matches!(*name, "shroudtopia.dll" | "winmm.dll" | "dbghelp.dll"),
        ["mods", name] => plain(name) && name.to_ascii_lowercase().ends_with(".dll"),
        ["mods", folder, name] => plain(folder) && folder_file(name),
        ["mods", folder, sub, name] => plain(folder) && plain(sub) && folder_file(name),
        _ => false,
    }
}

/// Installs a checked mods zip into `game`: every file is read and checked first, then each one is written (a temp
/// file renamed over it); a file it replaces is copied into `backup` before. Returns the paths installed.
pub fn install_mods(zip_path: &Path, game: &Path, backup: &Path, progress: &mut dyn FnMut(u64, u64)) -> Result<Vec<String>, String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("o pacote dos mods está estragado ({e})"))?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        if !installable(&name) {
            return Err(format!("o pacote dos mods tem um arquivo que o app não instala: {name}"));
        }
        let mut bytes = Vec::new();
        entry.by_ref().take(64 << 20).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        files.push((name, bytes));
    }
    if !files.iter().any(|(n, _)| n == "shroudtopia.dll") {
        return Err("o pacote dos mods não tem o shroudtopia.dll".into());
    }
    let total = files.len() as u64;
    for (i, (name, bytes)) in files.iter().enumerate() {
        let dest = game.join(name);
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        if dest.is_file() {
            let keep = backup.join(name);
            fs::create_dir_all(keep.parent().unwrap()).and_then(|_| fs::copy(&dest, &keep)).map_err(|e| format!("não deu para guardar a cópia de {name}: {e}"))?;
        }
        let tmp = dest.with_file_name(format!("{}.shroudtopia-tmp", dest.file_name().and_then(|n| n.to_str()).unwrap_or("file")));
        fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, &dest)).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("não deu para gravar {name} na pasta do jogo ({e}). Feche o Enshrouded e tente de novo.")
        })?;
        progress(i as u64 + 1, total);
    }
    Ok(files.into_iter().map(|(n, _)| n).collect())
}

/// The swap on the folder `dir` where the running app is `exe_name` and the checked download is `PARTIAL_NAME`:
/// the app becomes `OLD_NAME`, the download takes its name, `launch` starts it. Every failing step puts the folder
/// back.
pub fn swap(dir: &Path, exe_name: &str, launch: impl FnOnce(&Path) -> io::Result<()>) -> Result<(), String> {
    let (exe, partial, old) = (dir.join(exe_name), dir.join(PARTIAL_NAME), dir.join(OLD_NAME));
    if !partial.is_file() {
        return Err("não há versão nova baixada".into());
    }
    let _ = fs::remove_file(&old);
    fs::rename(&exe, &old).map_err(|e| {
        let _ = fs::remove_file(&partial);
        format!("não deu para trocar o arquivo do app ({e})")
    })?;
    let back = |why: String| {
        let _ = fs::remove_file(&partial);
        match fs::rename(&old, &exe) {
            Ok(()) => why,
            Err(_) => format!("{why}; renomeie {OLD_NAME} para {exe_name} na pasta do app"),
        }
    };
    if let Err(e) = fs::rename(&partial, &exe) {
        return Err(back(format!("não deu para trocar o arquivo do app ({e})")));
    }
    if let Err(e) = launch(&exe) {
        let _ = fs::rename(&exe, &partial);
        return Err(back(format!("não deu para abrir a versão nova ({e})")));
    }
    Ok(())
}

/// Deletes `OLD_NAME` (the old process may hold it for a moment): `attempts` tries half a second apart.
pub fn remove_old(dir: &Path, attempts: u32) -> bool {
    let old = dir.join(OLD_NAME);
    for i in 0..attempts.max(1) {
        match fs::remove_file(&old) {
            Ok(()) => return true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return true,
            Err(_) if i + 1 < attempts => std::thread::sleep(Duration::from_millis(500)),
            Err(_) => {}
        }
    }
    false
}

// ---------------------------------------------------------------------- the card's state

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    /// a newer release that verified
    Available(Box<Offer>),
    /// "Atualizar" was clicked with the game open: nothing is downloaded until it has been closed a while
    Queued(Box<Offer>),
    /// downloaded and checked; waits for the game to close
    Ready(Box<Offer>),
    Installing(Box<Offer>, u8),
    /// the app replaced itself and restarts
    Restarting,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    /// ok | avail | blocked | installing (the prototype's four)
    pub state: String,
    pub name: String,
    pub sub: String,
    pub action: Option<crate::view::Action>,
    pub progress: Option<crate::view::ProgressView>,
    pub changes: Vec<Changes>,
    pub badge: bool,
    pub app_version: String,
}

/// The update card, in the prototype's words.
pub fn view(phase: &Phase, installed: Option<&str>, checked: Option<DateTime<Utc>>, error: Option<&str>, none_published: bool, game_running: bool, now: DateTime<Utc>, off: FixedOffset) -> UpdateView {
    let act = |label: &str, cls: &str, disabled: bool| Some(crate::view::Action { label: label.into(), kind: "update".into(), icon: None, cls: cls.into(), disabled });
    let app_version = crate::APP_VERSION.to_string();
    let mods = installed.map(|v| format!("Mods v{v}")).unwrap_or_else(|| "Mods".into());
    match phase {
        Phase::Available(o) if game_running => UpdateView {
            state: "blocked".into(),
            name: format!("Versão {} disponível", o.manifest.version),
            sub: "Feche o Enshrouded para instalar.".into(),
            action: act("Atualizar", "primary", false),
            progress: None,
            changes: o.changes.clone(),
            badge: true,
            app_version,
        },
        Phase::Available(o) => UpdateView {
            state: "avail".into(),
            name: format!("Versão {} disponível", o.manifest.version),
            sub: match installed {
                Some(v) => format!("Você está na {v} · {}", format::size(o.manifest.mods.size)),
                None => format::size(o.manifest.mods.size),
            },
            action: act("Atualizar", "primary", false),
            progress: None,
            changes: o.changes.clone(),
            badge: true,
            app_version,
        },
        Phase::Queued(o) => UpdateView {
            state: "blocked".into(),
            name: format!("Versão {} na fila", o.manifest.version),
            sub: "Nada é baixado nem instalado com o Enshrouded aberto. Instala sozinho assim que ele fechar e o save terminar de subir.".into(),
            action: act("Atualizar", "primary", true),
            progress: None,
            changes: o.changes.clone(),
            badge: true,
            app_version,
        },
        Phase::Ready(o) => UpdateView {
            state: "blocked".into(),
            name: format!("Versão {} pronta", o.manifest.version),
            sub: "Instala sozinho assim que o Enshrouded fechar e o save terminar de subir.".into(),
            action: act("Atualizar", "primary", true),
            progress: None,
            changes: o.changes.clone(),
            badge: true,
            app_version,
        },
        Phase::Installing(o, pct) => UpdateView {
            state: "installing".into(),
            name: format!("Instalando {}", o.manifest.version),
            sub: String::new(),
            action: None,
            progress: Some(crate::view::ProgressView { pct: *pct, text: "Copiando os mods para a pasta do jogo".into() }),
            changes: o.changes.clone(),
            badge: false,
            app_version,
        },
        Phase::Restarting => UpdateView { state: "installing".into(), name: "Reiniciando o app".into(), sub: String::new(), action: None, progress: Some(crate::view::ProgressView { pct: 100, text: "Abrindo a versão nova".into() }), changes: vec![], badge: false, app_version },
        Phase::Idle | Phase::Checking => UpdateView {
            state: "ok".into(),
            name: mods,
            sub: match (phase, error, checked) {
                (Phase::Checking, _, _) => "Verificando…".into(),
                (_, None, Some(t)) if !none_published && (now - t).num_seconds() < 60 => "Tudo atualizado · verificado agora".into(),
                (_, Some(e), _) => format!("Não deu para verificar: {e}"),
                (_, None, Some(t)) if none_published => format!("Nenhuma versão publicada ainda · verificado {}", format::day_time(t, now, off)),
                (_, None, Some(t)) => format!("Tudo atualizado · verificado {}", format::day_time(t, now, off)),
                (_, None, None) => "Ainda não verificado".into(),
            },
            action: act("Verificar", "", matches!(phase, Phase::Checking)),
            progress: None,
            changes: vec![],
            badge: false,
            app_version,
        },
    }
}

/// Where downloads wait: `<data>\updates\<version>\`.
pub fn staging(data_dir: &Path, version: &str) -> PathBuf {
    data_dir.join("updates").join(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::testing::{sign, test_key};
    use crate::signature::HEADER;
    use std::io::Cursor;

    fn sha(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            for (name, bytes) in files {
                z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
                z.write_all(bytes).unwrap();
            }
            z.finish().unwrap();
        }
        out.into_inner()
    }

    fn gh(version: &str, body: &str) -> GhRelease {
        let a = |n: &str| GhAsset { name: n.into(), browser_download_url: format!("https://github.com/x/{n}"), size: 1 };
        GhRelease { tag_name: format!("v{version}"), body: Some(body.into()), assets: vec![a(EXE_ASSET), a(MODS_ASSET), a(MANIFEST_ASSET)], draft: false, prerelease: false }
    }

    fn manifest_json(version: &str, exe: &[u8], mods: &[u8]) -> String {
        let payload = format!("{HEADER}\nversion {version}\napp 0.1.1\nfile {EXE_ASSET} {} {}\nfile {MODS_ASSET} {} {}\n", exe.len(), sha(exe), mods.len(), sha(mods));
        serde_json::json!({ "payload": payload, "signature": sign(&payload) }).to_string()
    }

    #[test]
    fn a_signed_release_is_offered_and_another_is_not() {
        let mods = zip_of(&[("shroudtopia.dll", b"MZ loader")]);
        let m = manifest_json("0.4.3", b"MZ app", &mods);
        let o = offer(&gh("0.4.3", "Notas\n- Voo: volta a funcionar\n* Básicos: opção nova\n"), &m, &test_key()).unwrap();
        assert_eq!(o.manifest.version, "0.4.3");
        assert_eq!(o.changes, [Changes { version: "0.4.3".into(), items: vec!["Voo: volta a funcionar".into(), "Básicos: opção nova".into()] }]);
        // installed 0.4.0: the notes of 0.4.1 and 0.4.2 come too, newest first; 0.4.0, a draft and a newer one do not
        let mut o2 = o.clone();
        let mut draft = gh("0.4.2", "- rascunho");
        draft.draft = true;
        add_skipped(&mut o2, &[gh("0.4.4", "- futura"), gh("0.4.1", "- um"), draft, gh("0.4.2", "- dois"), gh("0.4.0", "- zero"), gh("0.4.3", "- repetida")], "0.4.0");
        assert_eq!(o2.changes.iter().map(|c| c.version.as_str()).collect::<Vec<_>>(), ["0.4.3", "0.4.2", "0.4.1"]);
        assert_eq!(o2.changes[1].items, ["dois"]);
        assert!(offer(&gh("0.4.4", ""), &m, &test_key()).is_err(), "the tag must be the signed version");
        assert!(offer(&gh("0.4.3", ""), &m, crate::signature::PRODUCT_KEY).is_err(), "another key");
        assert!(offer(&gh("0.4.3", ""), "{}", &test_key()).is_err());
    }

    #[test]
    fn downloads_are_kept_only_when_they_are_the_signed_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("x/Shroudtopia.exe.partial");
        let body = b"MZ new app".to_vec();
        let signed = SignedFile { name: EXE_ASSET.into(), size: body.len() as u64, sha256: sha(&body) };
        receive(&mut &body[..], &dest, &signed, &mut |_, _| {}).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), body);
        let mut other = body.clone();
        other[3] ^= 1;
        for bad in [&other[..], &body[..4], &[body.clone(), b"!".to_vec()].concat()[..]] {
            assert!(receive(&mut &bad[..], &dest, &signed, &mut |_, _| {}).is_err());
            assert!(!dest.exists());
        }
    }

    #[test]
    fn mods_install_into_the_game_folder_keeping_backups_and_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("Enshrouded");
        fs::create_dir_all(game.join("mods")).unwrap();
        fs::write(game.join("shroudtopia.dll"), b"old loader").unwrap();
        fs::write(game.join("shroudtopia.json"), b"{\"mine\":true}").unwrap();
        let zip = dir.path().join("m.zip");
        fs::write(&zip, zip_of(&[("shroudtopia.dll", b"new loader"), ("winmm.dll", b"proxy"), ("mods/basics_mod.dll", b"basics"), ("mods/flight_mod.dll", b"flight")])).unwrap();
        let backup = dir.path().join("backup");
        let done = install_mods(&zip, &game, &backup, &mut |_, _| {}).unwrap();
        assert_eq!(done.len(), 4);
        assert_eq!(fs::read(game.join("shroudtopia.dll")).unwrap(), b"new loader");
        assert_eq!(fs::read(game.join("mods/flight_mod.dll")).unwrap(), b"flight");
        assert_eq!(fs::read(backup.join("shroudtopia.dll")).unwrap(), b"old loader");
        assert_eq!(fs::read(game.join("shroudtopia.json")).unwrap(), b"{\"mine\":true}");
        // mod folders (the minimap, EML's Lua mods) and EML itself
        fs::write(&zip, zip_of(&[("shroudtopia.dll", b"new loader"), ("dbghelp.dll", b"eml"), ("mods/minimap_mod/minimap_mod.dll", b"map"), ("mods/minimap_mod/mod.json", b"{}"), ("mods/minimap_mod/embervale_minimap_terrain.frag", b"f"), ("mods/minimap_mod/embervale_minimap_terrain.vert", b"v"), ("mods/max-stack/src/mod.lua", b"lua")])).unwrap();
        assert_eq!(install_mods(&zip, &game, &backup, &mut |_, _| {}).unwrap().len(), 7);
        assert_eq!(fs::read(game.join("mods/minimap_mod/embervale_minimap_terrain.vert")).unwrap(), b"v");
        assert_eq!(fs::read(game.join("mods/max-stack/src/mod.lua")).unwrap(), b"lua");
        assert_eq!(fs::read(game.join("dbghelp.dll")).unwrap(), b"eml");
        // a zip with anything else is refused whole, before a file is written
        for bad in ["shroudtopia.json", "../evil.dll", "mods/../../x.dll", "mods/a/b/c/x.dll", "mods/sub/x.exe", "mods/sub/.hidden.json", "mods/../x.json", "other.exe", "/abs.dll"] {
            fs::write(&zip, zip_of(&[("shroudtopia.dll", b"newer"), (bad, b"x")])).unwrap();
            assert!(install_mods(&zip, &game, &backup, &mut |_, _| {}).is_err(), "{bad}");
            assert_eq!(fs::read(game.join("shroudtopia.dll")).unwrap(), b"new loader", "{bad}");
        }
    }

    #[test]
    fn the_swap_and_its_way_back() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Shroudtopia.exe"), b"old").unwrap();
        fs::write(dir.path().join(PARTIAL_NAME), b"new").unwrap();
        assert!(swap(dir.path(), "Shroudtopia.exe", |_| Err(io::Error::other("no"))).is_err());
        assert_eq!(fs::read(dir.path().join("Shroudtopia.exe")).unwrap(), b"old");
        fs::write(dir.path().join(PARTIAL_NAME), b"new").unwrap();
        swap(dir.path(), "Shroudtopia.exe", |_| Ok(())).unwrap();
        assert_eq!(fs::read(dir.path().join("Shroudtopia.exe")).unwrap(), b"new");
        assert!(remove_old(dir.path(), 1));
        assert!(!dir.path().join(OLD_NAME).exists());
    }

    #[test]
    fn the_card_says_the_prototypes_words() {
        let now = Utc::now();
        let off = FixedOffset::east_opt(0).unwrap();
        let mods = zip_of(&[("shroudtopia.dll", b"x")]);
        let o = Box::new(offer(&gh("0.4.3", "- a"), &manifest_json("0.4.3", b"e", &mods), &test_key()).unwrap());
        let v = view(&Phase::Available(o.clone()), Some("0.4.2"), None, None, false, false, now, off);
        assert_eq!((v.state.as_str(), v.name.as_str()), ("avail", "Versão 0.4.3 disponível"));
        assert!(v.sub.starts_with("Você está na 0.4.2 · "));
        let v = view(&Phase::Ready(o.clone()), Some("0.4.2"), None, None, false, true, now, off);
        assert_eq!((v.state.as_str(), v.name.as_str(), v.sub.as_str()), ("blocked", "Versão 0.4.3 pronta", "Instala sozinho assim que o Enshrouded fechar e o save terminar de subir."));
        assert!(v.action.unwrap().disabled);
        // clicked with the game open: queued, nothing downloaded, the button stays off
        let v = view(&Phase::Queued(o.clone()), Some("0.4.2"), None, None, false, true, now, off);
        assert_eq!((v.state.as_str(), v.name.as_str()), ("blocked", "Versão 0.4.3 na fila"));
        assert!(v.action.unwrap().disabled);
        let v = view(&Phase::Installing(o, 70), Some("0.4.2"), None, None, false, false, now, off);
        assert_eq!((v.name.as_str(), v.progress.unwrap().text.as_str()), ("Instalando 0.4.3", "Copiando os mods para a pasta do jogo"));
        let v = view(&Phase::Idle, Some("0.4.2"), Some(now), None, false, false, now, off);
        assert_eq!(v.name, "Mods v0.4.2");
        assert_eq!(v.sub, "Tudo atualizado · verificado agora");
        assert_eq!(v.action.unwrap().label, "Verificar");
        let v = view(&Phase::Idle, Some("0.4.2"), Some(now - chrono::Duration::minutes(5)), None, false, false, now, off);
        assert!(v.sub.starts_with("Tudo atualizado · verificado hoje, "));
    }
}
