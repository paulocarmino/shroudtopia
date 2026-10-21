//! The commands the page calls. Each one that touches the sync runs on the engine's thread (`Shared::engine`);
//! they are async so a long one (a pull before "Jogar") never blocks the window.

use serde::Serialize;
use serde_json::{json, Value};
use shroudtopia_core::format;
use shroudtopia_core::mods;
use shroudtopia_core::saves::StoreKind;
use shroudtopia_core::steam;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::Shared;

type R = Result<Value, String>;

fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl std::future::Future<Output = T> {
    async move { tauri::async_runtime::spawn_blocking(f).await.expect("a command's thread") }
}

/// Everything the page shows, in one object (also sent as the `app-state` event when it changes).
pub fn ui_state(shared: &Shared) -> Value {
    let game_running = shroudtopia_win::process::game_running();
    let snapshot = shared.snapshot.lock().unwrap().clone();
    let game = shared.game_folder.lock().unwrap().clone();
    let (mod_list, mods_error) = match &game {
        None => (vec![], Some("A pasta do Enshrouded não foi achada. Escolha em Atualização › Este PC › Pasta do jogo.".to_string())),
        Some(g) => match mods::view(&mods::config_path(g), Some(&g.join("mods"))) {
            Ok(list) => (list, None),
            Err(mods::ModsError::Invalid(e)) => (vec![], Some(format!("O shroudtopia.json da pasta do jogo não pôde ser lido ({e})."))),
            Err(mods::ModsError::Io(e)) => (vec![], Some(format!("Não deu para ler o shroudtopia.json ({e})."))),
            Err(mods::ModsError::Missing) => (vec![], None),
        },
    };
    json!({
        "sync": snapshot,
        "gameRunning": game_running,
        "mods": { "items": mod_list, "error": mods_error },
        "update": shared.updates.view(game_running),
        "settings": {
            "gameFolder": game.map(|g| g.display().to_string()),
            "autostart": shroudtopia_win::autostart::is_on(),
            "appVersion": shroudtopia_core::APP_VERSION,
            "steamFound": shared.steam_user.is_some(),
            "localStore": shared.stores.local.as_ref().map(|s| s.dir().display().to_string()),
            "steamStore": shared.stores.steam.as_ref().map(|s| s.dir().display().to_string()),
        },
    })
}

// Every command runs off the main thread (async + blocking): the main thread also draws the window and serves the
// tray, so a command waiting on a lock there could freeze both.
#[tauri::command]
pub async fn get_state(app: AppHandle) -> Value {
    blocking(move || ui_state(&app.state::<Shared>())).await
}

fn open(app: &AppHandle, url: &str) -> Result<(), String> {
    app.opener().open_url(url, None::<&str>).map_err(|e| format!("Não deu para abrir {url}: {e}"))
}

#[tauri::command]
pub async fn play(app: AppHandle) -> R {
    blocking(move || {
        let url = app.state::<Shared>().engine(|e| e.play())?;
        open(&app, url.as_str().unwrap_or(shroudtopia_core::engine::STEAM_PLAY))?;
        Ok(Value::Null)
    })
    .await
}

/// "Entrar pelo Steam": the host's game through Steam (`steam://friends/joingame/<steamid64>`).
#[tauri::command]
pub async fn join(app: AppHandle) -> R {
    blocking(move || {
        let lock = app.state::<Shared>().engine(|e| e.foreign_lock().map(|l| (l.holder.clone(), l.nick.clone())).ok_or_else(|| "Ninguém está hospedando agora.".to_string()))?;
        let holder = lock.get(0).and_then(Value::as_str).unwrap_or_default().to_string();
        let nick = lock.get(1).and_then(Value::as_str).unwrap_or("o host").to_string();
        // Steam's "Join game" on the host. The current Steam client often ignores it when it comes from outside
        // (seen 2026-10-21): when the game has not started 10 s later, start it and say where to join.
        open(&app, &shroudtopia_core::engine::join_url(&holder))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if shroudtopia_win::process::game_running() {
                return Ok(Value::Null);
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        open(&app, shroudtopia_core::engine::STEAM_PLAY)?;
        Ok(json!({ "note": format!("O Enshrouded está abrindo. Lá dentro, vá em Jogar › Entrar e escolha o mundo de {nick} na lista de amigos.") }))
    })
    .await
}

#[tauri::command]
pub async fn resolve(app: AppHandle, keep_mine: bool) -> R {
    blocking(move || app.state::<Shared>().engine(move |e| e.resolve(keep_mine))).await
}

#[tauri::command]
pub async fn login_start(app: AppHandle) -> R {
    blocking(move || {
        let url = app.state::<Shared>().engine(|e| e.login_start())?;
        open(&app, url.as_str().unwrap_or_default())?;
        Ok(Value::Null)
    })
    .await
}

#[tauri::command]
pub async fn login_cancel(app: AppHandle) -> R {
    blocking(move || {
        app.state::<Shared>().engine(|e| {
            e.login_cancel();
            Ok(())
        })
    })
    .await
}

#[tauri::command]
pub async fn login_accept_mismatch(app: AppHandle) -> R {
    blocking(move || app.state::<Shared>().engine(|e| e.login_accept_mismatch())).await
}

#[tauri::command]
pub async fn logout(app: AppHandle) -> R {
    blocking(move || {
        app.state::<Shared>().engine(|e| {
            e.logout();
            Ok(())
        })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SlotRow {
    store: StoreKind,
    index: u8,
    file: String,
    title: String,
    sub: String,
}

/// "Qual mundo?": the worlds of this PC, Steam Cloud's and the local folder's.
#[tauri::command]
pub async fn list_slots(app: AppHandle) -> Value {
    blocking(move || slots(&app.state::<Shared>())).await
}

fn slots(shared: &Shared) -> Value {
    let off = *chrono::Local::now().offset();
    let mut rows = Vec::new();
    for (kind, store) in [(StoreKind::Steam, shared.stores.steam.clone()), (StoreKind::Local, shared.stores.local.clone())] {
        let Some(store) = store else { continue };
        for s in store.list() {
            rows.push(SlotRow {
                store: kind,
                index: s.index,
                file: s.file.clone(),
                title: s.name.clone().unwrap_or_else(|| format!("Mundo {}", s.index + 1)),
                sub: format!("{} · jogado em {} · {}", format::size(s.size), format::played_at(s.modified, off), if kind == StoreKind::Steam { "Steam" } else { "Local" }),
            });
        }
    }
    let free = |k: StoreKind| shared.stores.get(k).and_then(|s| s.free_slot(&[])).map(steam::slot_file);
    json!({ "slots": rows, "freeSteam": free(StoreKind::Steam), "freeLocal": free(StoreKind::Local), "steam": shared.stores.steam.is_some(), "local": shared.stores.local.is_some() })
}

#[tauri::command]
pub async fn share(app: AppHandle, store: StoreKind, slot: u8, into: StoreKind, name: String) -> R {
    blocking(move || app.state::<Shared>().engine(move |e| e.share(store, slot, into, &name))).await
}

#[tauri::command]
pub async fn new_invite(app: AppHandle) -> R {
    blocking(move || app.state::<Shared>().engine(|e| e.new_invite())).await
}

#[tauri::command]
pub async fn redeem(app: AppHandle, code: String, into: StoreKind) -> R {
    blocking(move || {
        app.state::<Shared>().engine(move |e| {
            let w = e.redeem(&code, into)?;
            let me = e.me().unwrap_or("").to_string();
            let from = w.members.iter().find(|m| m.steam_id != me).map(|m| m.nick.clone()).unwrap_or_default();
            let size = w.head.as_ref().map(|h| format::size(h.size)).unwrap_or_else(|| "—".into());
            let file = e.cfg.world.as_ref().map(|l| steam::slot_file(l.slot)).unwrap_or_default();
            Ok(json!({ "name": w.name, "sub": format!("de {from} · {size} · foi para o espaço {file}"), "initial": format::initial(&from) }))
        })
    })
    .await
}

#[tauri::command]
pub async fn set_mod(app: AppHandle, mod_key: String, key: String, value: Value) -> R {
    blocking(move || set_mod_now(&app, mod_key, key, value)).await
}

fn set_mod_now(app: &AppHandle, mod_key: String, key: String, value: Value) -> R {
    let shared = app.state::<Shared>();
    let game = shared.game_folder.lock().unwrap().clone().ok_or("A pasta do Enshrouded não foi achada.")?;
    mods::set(&mods::config_path(&game), &mod_key, &key, value).map_err(|e| match e {
        mods::ModsError::Invalid(e) => format!("O shroudtopia.json não pôde ser lido ({e}); nada foi mudado."),
        mods::ModsError::Io(e) => format!("Não deu para gravar o shroudtopia.json ({e})."),
        mods::ModsError::Missing => "Não deu para gravar o shroudtopia.json.".to_string(),
    })?;
    crate::emit_state(app);
    Ok(Value::Null)
}

#[tauri::command]
pub async fn update_check(app: AppHandle) {
    blocking(move || {
        app.state::<Shared>().updates.check();
        crate::emit_state(&app);
    })
    .await
}

#[tauri::command]
pub async fn update_apply(app: AppHandle) {
    blocking(move || app.state::<Shared>().updates.apply()).await
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, on: bool) -> R {
    blocking(move || {
        shroudtopia_win::autostart::set(on).map_err(|e| format!("Não deu para mudar a inicialização com o Windows: {e}"))?;
        crate::emit_state(&app);
        Ok(Value::Null)
    })
    .await
}

/// "Trocar": a folder picked by hand, kept only when it has `enshrouded.exe`.
#[tauri::command]
pub async fn choose_game_folder(app: AppHandle) -> R {
    blocking(move || {
        let Some(picked) = app.dialog().file().set_title("Pasta do Enshrouded").blocking_pick_folder() else { return Ok(Value::Null) };
        let path = picked.into_path().map_err(|e| e.to_string())?;
        if !steam::is_game_folder(&path) {
            return Err("Essa pasta não tem o enshrouded.exe.".into());
        }
        let shared = app.state::<Shared>();
        *shared.game_folder.lock().unwrap() = Some(path.clone());
        let stores = crate::make_stores_for(&shared, Some(path.clone()));
        shared.engine(move |e| {
            e.cfg.game_folder = Some(path);
            e.stores = stores;
            e.save();
            Ok(())
        })?;
        crate::emit_state(&app);
        Ok(Value::Null)
    })
    .await
}

#[tauri::command]
pub async fn move_store(app: AppHandle, into: StoreKind) -> R {
    blocking(move || app.state::<Shared>().engine(move |e| e.move_store(into))).await
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window(crate::WINDOW) {
        let _ = w.hide();
    }
}
