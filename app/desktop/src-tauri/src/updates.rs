//! The updater's thread: looks at GitHub's latest release at start, hourly and on "Verificar" (game open or not),
//! downloads and checks it on "Atualizar" and installs the mods only while the game is closed (else it waits in line
//! and installs by itself as soon as the game closes), then swaps the app's own `.exe` when the release has a newer
//! one and restarts.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shroudtopia_core::signature::{self, PRODUCT_KEY};
use shroudtopia_core::updater::{self, Offer, Phase, UpdateView};
use tauri::{AppHandle, Manager};

use crate::Shared;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Saved {
    mods_version: Option<String>,
    /// the layout of the zip `mods_version` came in (updater::MODS_LAYOUT); 0 when an app up to 0.1.5 installed it
    mods_layout: u32,
    last_check: Option<DateTime<Utc>>,
}

#[derive(Default)]
struct Inner {
    phase: Phase,
    error: Option<String>,
    saved: Saved,
    check_now: bool,
    apply_now: bool,
    /// the files of the offer, downloaded and checked
    mods_zip: Option<PathBuf>,
    exe_ready: bool,
    /// the last check found no release on GitHub at all
    none_published: bool,
}

pub struct Updates {
    file: PathBuf,
    inner: Mutex<Inner>,
}

impl Updates {
    pub fn new(data_dir: &Path) -> Updates {
        let file = data_dir.join("updates.json");
        let saved: Saved = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        // look at every start: what a check found lives only in memory, so an app reopened within the hour said
        // "Tudo atualizado" over a release it had already seen
        Updates { file, inner: Mutex::new(Inner { saved, check_now: true, ..Default::default() }) }
    }

    fn save(&self, inner: &Inner) {
        let _ = std::fs::write(&self.file, serde_json::to_vec_pretty(&inner.saved).unwrap_or_default());
    }

    pub fn view(&self, game_running: bool) -> UpdateView {
        let i = self.inner.lock().unwrap();
        updater::view(&i.phase, i.saved.mods_version.as_deref(), i.saved.last_check, i.error.as_deref(), i.none_published, game_running, Utc::now(), *chrono::Local::now().offset())
    }

    pub fn check(&self) {
        self.inner.lock().unwrap().check_now = true;
    }

    pub fn apply(&self) {
        self.inner.lock().unwrap().apply_now = true;
    }
}

fn wanted(offer: &Offer, saved: &Saved) -> bool {
    saved.mods_layout < updater::MODS_LAYOUT
        || saved.mods_version.as_deref().is_none_or(|v| signature::newer(&offer.manifest.version, v))
        || signature::newer(&offer.manifest.app, shroudtopia_core::APP_VERSION)
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || loop {
        step(&app);
        std::thread::sleep(Duration::from_secs(2));
    });
}

fn set_phase(app: &AppHandle, phase: Phase) {
    app.state::<Shared>().updates.inner.lock().unwrap().phase = phase;
    crate::emit_state(app);
}

fn step(app: &AppHandle) {
    let shared = app.state::<Shared>();
    let u = &shared.updates;
    // the check is a small request and runs any time; the download and the install wait for the game to be closed
    // (a friend's PC froze on 2026-10-21 with the game and an update together) and for no save to be on its way
    // (the install may restart the app). Looked at again before each step.
    let closed = free(app);
    let (due, apply, phase) = {
        let mut i = u.inner.lock().unwrap();
        let due = i.check_now || (matches!(i.phase, Phase::Idle) && i.saved.last_check.is_none_or(|t| Utc::now() - t >= updater::CHECK_EVERY || Utc::now() < t));
        i.check_now = false;
        let apply = std::mem::take(&mut i.apply_now);
        (due, apply, i.phase.clone())
    };
    if due && matches!(phase, Phase::Idle | Phase::Available(_)) {
        set_phase(app, Phase::Checking);
        let started = std::time::Instant::now();
        let installed = u.inner.lock().unwrap().saved.mods_version.clone();
        let r = updater::fetch_latest(PRODUCT_KEY.trim(), installed.as_deref());
        // a check takes a blink: keep "Verificando…" on screen long enough to be seen
        if let Some(rest) = Duration::from_millis(800).checked_sub(started.elapsed()) {
            std::thread::sleep(rest);
        }
        let mut i = u.inner.lock().unwrap();
        i.saved.last_check = Some(Utc::now());
        match r {
            Ok(Some(offer)) if wanted(&offer, &i.saved) => {
                i.error = None;
                i.none_published = false;
                i.phase = Phase::Available(Box::new(offer));
            }
            Ok(found) => {
                i.error = None;
                i.none_published = found.is_none();
                i.phase = Phase::Idle;
            }
            Err(e) => {
                i.error = Some(e);
                i.phase = Phase::Idle;
            }
        }
        u.save(&i);
        drop(i);
        crate::emit_state(app);
        return;
    }
    let queued = matches!(phase, Phase::Queued(_));
    match phase {
        Phase::Available(offer) if apply && !closed => set_phase(app, Phase::Queued(offer)),
        Phase::Available(offer) | Phase::Queued(offer) if (apply || queued) && closed => {
            if let Err(e) = download(app, &offer) {
                let mut i = u.inner.lock().unwrap();
                i.error = Some(e);
                i.phase = Phase::Available(offer);
                drop(i);
                crate::emit_state(app);
                return;
            }
            set_phase(app, Phase::Ready(offer.clone()));
            if free(app) {
                install(app, &offer);
            }
        }
        Phase::Ready(offer) if closed => install(app, &offer),
        _ => {}
    }
}

/// The game is closed and no save is going up or down.
fn free(app: &AppHandle) -> bool {
    let syncing = app.state::<Shared>().snapshot.lock().unwrap().as_ref().is_some_and(|s| s.syncing);
    !syncing && !shroudtopia_win::process::game_running()
}

fn download(app: &AppHandle, offer: &Offer) -> Result<(), String> {
    let shared = app.state::<Shared>();
    let stage = updater::staging(&shared.data_dir, &offer.manifest.version);
    let zip = stage.join(signature::MODS_ASSET);
    set_phase(app, Phase::Installing(Box::new(offer.clone()), 0));
    let mut last = 0u8;
    updater::download(&offer.mods_url, &zip, &offer.manifest.mods, &mut |d, t| {
        let pct = (shroudtopia_core::format::percent(d, t) / 2).min(49);
        if pct != last {
            last = pct;
            set_phase(app, Phase::Installing(Box::new(offer.clone()), pct));
        }
    })?;
    let mut exe_ready = false;
    if signature::newer(&offer.manifest.app, shroudtopia_core::APP_VERSION) {
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            updater::download(&offer.exe_url, &dir.join(updater::PARTIAL_NAME), &offer.manifest.exe, &mut |_, _| {})?;
            exe_ready = true;
        }
    }
    let mut i = shared.updates.inner.lock().unwrap();
    i.mods_zip = Some(zip);
    i.exe_ready = exe_ready;
    Ok(())
}

fn install(app: &AppHandle, offer: &Offer) {
    let shared = app.state::<Shared>();
    // the last look before the game folder is written: the game may have opened during the download
    if !free(app) {
        set_phase(app, Phase::Ready(Box::new(offer.clone())));
        return;
    }
    let zip = shared.updates.inner.lock().unwrap().mods_zip.clone();
    let game = shared.game_folder.lock().unwrap().clone();
    let (Some(zip), Some(game)) = (zip, game) else {
        let mut i = shared.updates.inner.lock().unwrap();
        i.error = Some("a pasta do jogo não foi achada (Este PC › Pasta do jogo)".into());
        i.phase = Phase::Available(Box::new(offer.clone()));
        drop(i);
        crate::emit_state(app);
        return;
    };
    let old = shared.updates.inner.lock().unwrap().saved.mods_version.clone().unwrap_or_else(|| "anterior".into());
    let backup = shared.data_dir.join("backup").join(format!("mods-{old}"));
    set_phase(app, Phase::Installing(Box::new(offer.clone()), 50));
    let r = updater::install_mods(&zip, &game, &backup, &mut |d, t| {
        set_phase(app, Phase::Installing(Box::new(offer.clone()), 50 + (shroudtopia_core::format::percent(d, t) / 2)));
    });
    let exe_ready = {
        let mut i = shared.updates.inner.lock().unwrap();
        match &r {
            Ok(_) => {
                i.saved.mods_version = Some(offer.manifest.version.clone());
                i.saved.mods_layout = updater::MODS_LAYOUT;
                i.error = None;
                i.phase = Phase::Idle;
            }
            Err(e) => {
                i.error = Some(e.clone());
                i.phase = Phase::Available(Box::new(offer.clone()));
            }
        }
        shared.updates.save(&i);
        r.is_ok() && i.exe_ready
    };
    crate::emit_state(app);
    if exe_ready {
        restart_into_new(app);
    }
}

fn restart_into_new(app: &AppHandle) {
    let Some(exe) = std::env::current_exe().ok() else { return };
    let (Some(dir), Some(name)) = (exe.parent(), exe.file_name().and_then(|n| n.to_str())) else { return };
    set_phase(app, Phase::Restarting);
    let r = updater::swap(dir, name, |new| std::process::Command::new(new).arg(updater::AFTER_UPDATE).spawn().map(|_| ()));
    match r {
        Ok(()) => crate::quit(app),
        Err(e) => {
            let shared = app.state::<Shared>();
            let mut i = shared.updates.inner.lock().unwrap();
            i.error = Some(e);
            i.phase = Phase::Idle;
            drop(i);
            crate::emit_state(app);
        }
    }
}
