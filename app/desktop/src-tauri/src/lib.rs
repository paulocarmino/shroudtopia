//! Shroudtopia, the Tauri shell: wiring and lifecycle only. The window (no title bar: the page draws the prototype's
//! own), the tray, a single instance, the engine's thread (shroudtopia-core's sync engine, ticked every second with
//! the game's process state), the updater's thread, and the commands the page calls. What the app does is the core.

mod commands;
mod updates;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use shroudtopia_core::api::HttpApi;
use shroudtopia_core::contract::Player;
use shroudtopia_core::engine::{Engine, Stores};
use shroudtopia_core::saves::{LocalStore, SaveStore, SteamStore};
use shroudtopia_core::steam;
use shroudtopia_core::view::Snapshot;
use shroudtopia_win::steam_remote::{self, HelperRemote};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

pub const WINDOW: &str = "main";
pub const TRAY: &str = "tray";
pub const STATE_EVENT: &str = "app-state";

/// What the engine's thread is asked to do between ticks; each answers on its own channel.
pub enum Job {
    Run(Box<dyn FnOnce(&mut Engine) -> Result<serde_json::Value, String> + Send>, Sender<Result<serde_json::Value, String>>),
}

/// What the commands share.
pub struct Shared {
    pub jobs: Sender<Job>,
    pub snapshot: Mutex<Option<Snapshot>>,
    pub last_sent: Mutex<String>,
    pub data_dir: PathBuf,
    pub steam_root: Option<PathBuf>,
    pub steam_user: Option<steam::SteamUser>,
    pub stores: Stores,
    pub game_folder: Mutex<Option<PathBuf>>,
    pub updates: updates::Updates,
}

impl Shared {
    /// Runs `f` on the engine's thread and waits for its answer.
    pub fn engine<T: serde::Serialize>(&self, f: impl FnOnce(&mut Engine) -> Result<T, String> + Send + 'static) -> Result<serde_json::Value, String> {
        let (tx, rx) = mpsc::channel();
        self.jobs
            .send(Job::Run(Box::new(move |e| f(e).and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))), tx))
            .map_err(|_| "o app está fechando".to_string())?;
        rx.recv().map_err(|_| "o app está fechando".to_string())?
    }
}

pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(WINDOW) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Builds the page's whole state and sends it when it changed.
pub fn emit_state(app: &AppHandle) {
    let shared = app.state::<Shared>();
    let state = commands::ui_state(&shared);
    let text = serde_json::to_string(&state).unwrap_or_default();
    {
        let mut last = shared.last_sent.lock().unwrap();
        if *last == text {
            return;
        }
        *last = text;
    }
    // No lock is held from here on. The tray call waits for the main thread, and the main thread may be waiting for
    // `snapshot` (get_state): holding it here froze the window, blank or "Não respondendo" (2026-10-21).
    let tip = shared.snapshot.lock().unwrap().as_ref().map(shroudtopia_core::view::tray_text);
    let _ = app.emit(STATE_EVENT, state);
    if let (Some(tray), Some(tip)) = (app.tray_by_id(TRAY), tip) {
        let _ = tray.set_tooltip(Some(tip));
    }
}

fn offset() -> chrono::FixedOffset {
    *chrono::Local::now().offset()
}

/// The engine's thread: jobs as they come, a tick every second.
fn engine_loop(mut engine: Engine, jobs: Receiver<Job>, app: AppHandle) {
    let mut last_tick = std::time::Instant::now() - Duration::from_secs(5);
    loop {
        let wait = Duration::from_secs(1).saturating_sub(last_tick.elapsed());
        match jobs.recv_timeout(wait) {
            Ok(Job::Run(f, reply)) => {
                engine.rt.now = chrono::Utc::now();
                engine.rt.offset = Some(offset());
                let r = f(&mut engine);
                // published before the answer, so the page's get_state right after a command already sees it
                engine.publish_now();
                let _ = reply.send(r);
            }
            Err(RecvTimeoutError::Timeout) => {
                last_tick = std::time::Instant::now();
                let running = shroudtopia_win::process::game_running();
                engine.rt.just_closed = !running && !shroudtopia_win::process::game_settled();
                engine.tick(chrono::Utc::now(), offset(), running);
                engine.publish_now();
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let _ = &app;
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Abrir", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Sair do app", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit_item])?;
    TrayIconBuilder::with_id(TRAY)
        .icon(app.default_window_icon().cloned().expect("the window icon is compiled in"))
        .tooltip("Shroudtopia")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_window(app),
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn quit(app: &AppHandle) {
    // off the caller's thread: from the tray menu that is the main thread, and the engine may be waiting on it
    let app = app.clone();
    std::thread::spawn(move || {
        // the lease is given back if this PC holds it (the game is closed when that matters; otherwise it expires)
        let _ = app.state::<Shared>().engine(|e| {
            if !e.rt.game_running {
                e.forget_lock();
            }
            Ok(())
        });
        app.exit(0);
    });
}

fn make_stores(steam_root: Option<&PathBuf>, user: Option<&steam::SteamUser>, game: Option<PathBuf>, data_dir: &std::path::Path) -> Stores {
    let local = shroudtopia_win::saved_games().map(|sg| Arc::new(LocalStore::new(steam::local_save_dir(&sg))) as Arc<dyn SaveStore>);
    let steam_store = match (steam_root, user) {
        (Some(root), Some(user)) => {
            let remote = HelperRemote {
                exe: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("Shroudtopia.exe")),
                game_dir: game.unwrap_or_default(),
                temp_dir: data_dir.join("tmp"),
            };
            Some(Arc::new(SteamStore::new(steam::save_dir(root, user), Arc::new(remote))) as Arc<dyn SaveStore>)
        }
        _ => None,
    };
    Stores { local, steam: steam_store }
}

pub fn make_stores_for(shared: &Shared, game: Option<PathBuf>) -> Stores {
    make_stores(shared.steam_root.as_ref(), shared.steam_user.as_ref(), game, &shared.data_dir)
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let data_dir = shroudtopia_win::data_dir();
    let _ = std::fs::create_dir_all(&data_dir);
    let steam_root = shroudtopia_win::steam_root();
    let steam_user = steam_root.as_ref().and_then(|r| steam::current_user(r));
    let cfg = shroudtopia_core::config::ConfigStore::new(&data_dir).load();
    let game_folder = cfg.game_folder.clone().filter(|g| steam::is_game_folder(g)).or_else(|| steam_root.as_ref().and_then(|r| steam::game_folder(r)));
    shroudtopia_win::diag::note_start(game_folder.as_deref());
    let stores = make_stores(steam_root.as_ref(), steam_user.as_ref(), game_folder.clone(), &data_dir);
    let server =std::env::args().skip_while(|a| a != "--server").nth(1).unwrap_or(cfg.server_url.clone());
    let api = HttpApi::new(&server, &format!("Shroudtopia/{}", shroudtopia_core::APP_VERSION));
    let avatar = match (&steam_root, &steam_user) {
        (Some(r), Some(u)) => Some(steam::avatar_path(r, &u.steam_id)),
        _ => None,
    };
    let mut engine = Engine::new(Box::new(api), data_dir.clone(), stores.clone(), avatar);
    if let Some(u) = &steam_user {
        engine.set_player(Player { steam_id: u.steam_id.clone(), nick: u.nick.clone() });
    }
    let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
    let updates = updates::Updates::new(&data_dir);
    app.manage(Shared {
        jobs: jobs_tx,
        snapshot: Mutex::new(None),
        last_sent: Mutex::new(String::new()),
        data_dir: data_dir.clone(),
        steam_root,
        steam_user,
        stores,
        game_folder: Mutex::new(game_folder),
        updates,
    });
    let h = handle.clone();
    engine.set_publisher(Arc::new(move |s: &Snapshot| {
        *h.state::<Shared>().snapshot.lock().unwrap() = Some(s.clone());
        emit_state(&h);
    }));
    let h = handle.clone();
    std::thread::spawn(move || engine_loop(engine, jobs_rx, h));
    // mods and update state change on their own (the file, the game): looked at every 2 s
    let h = handle.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        emit_state(&h);
    });
    updates::start(handle.clone());

    build_tray(&handle)?;
    let minimized = std::env::args().any(|a| a == shroudtopia_win::autostart::MINIMIZED);
    if !minimized {
        show_window(&handle);
    }
    // the app this one replaced is deleted once this one is up
    let after = std::env::args().any(|a| a == shroudtopia_core::updater::AFTER_UPDATE);
    std::thread::spawn(move || {
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
            shroudtopia_core::updater::remove_old(&dir, if after { 30 } else { 1 });
        }
    });
    Ok(())
}

/// After an update the old app may still be quitting; while its single-instance mutex exists the plugin would hand
/// this start to it. Wait for it to go (tf3-mods' companion does the same).
#[cfg(windows)]
fn wait_for_previous(identifier: &str, limit: Duration) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
    let name: Vec<u16> = format!("{identifier}-sim").encode_utf16().chain(std::iter::once(0)).collect();
    let deadline = std::time::Instant::now() + limit;
    loop {
        // SAFETY: a NUL-terminated name; a handle given is closed at once
        let handle = unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr()) };
        if handle.is_null() || std::time::Instant::now() >= deadline {
            return;
        }
        unsafe { CloseHandle(handle) };
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(not(windows))]
fn wait_for_previous(_identifier: &str, _limit: Duration) {}

pub fn run() {
    shroudtopia_win::diag::install_panic_log();
    let args: Vec<String> = std::env::args().collect();
    // the Steam remote storage helper (steam_remote.rs): one operation, then exit, no window
    if let Some(at) = args.iter().position(|a| a == steam_remote::FLAG) {
        std::process::exit(steam_remote::helper_main(&args[at + 1..]));
    }
    if let Some(exe) = shroudtopia_win::diag::running_from_temp() {
        shroudtopia_win::diag::note(&format!("refused to start from the temp folder: {}", exe.display()));
        shroudtopia_win::diag::message(
            "Shroudtopia",
            "O Shroudtopia foi aberto de dentro do .zip.\n\nClique com o botão direito no Shroudtopia.zip, escolha \"Extrair tudo\" para uma pasta (por exemplo Documentos) e abra o Shroudtopia.exe de lá.",
        );
        return;
    }
    let context = tauri::generate_context!();
    if args.iter().any(|a| a == shroudtopia_core::updater::AFTER_UPDATE) {
        wait_for_previous(&context.config().identifier, Duration::from_secs(15));
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(setup)
        .on_window_event(|window, event| {
            // closing the window hides it: the app lives in the tray and keeps syncing
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::play,
            commands::join,
            commands::resolve,
            commands::login_start,
            commands::login_cancel,
            commands::login_accept_mismatch,
            commands::logout,
            commands::list_slots,
            commands::share,
            commands::new_invite,
            commands::delete_world,
            commands::redeem,
            commands::set_mod,
            commands::update_check,
            commands::update_apply,
            commands::set_autostart,
            commands::choose_game_folder,
            commands::move_store,
            commands::hide_window,
        ])
        .build(context)
        .expect("failed to build Shroudtopia")
        .run(|_, _| {});
}
