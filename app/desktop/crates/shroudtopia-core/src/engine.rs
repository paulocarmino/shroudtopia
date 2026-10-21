//! The sync engine (CONTRACT.md "What the app does with the files"). One thread owns it and calls `tick` about once a
//! second with the time and whether `enshrouded.exe` runs; the window's actions (`play`, `resolve`, `share`,
//! `redeem`, `new_invite`) run on the same thread between ticks.
//!
//! The loop:
//! - the game starts: a session begins and the host lease is taken (or someone else holds it: they are the host);
//! - while it runs: the lease is renewed every 60 s; the world file is watched, and once it is unchanged (size and
//!   mtime) for 5 s and differs from the last upload, it goes up as an autosave whose parent is the previous upload;
//!   uploads only happen while this PC holds the lease;
//! - the game exits: the final upload (with `endedAt`) and the lease is released. An upload that cannot reach the
//!   server waits in a queue (a snapshot copy of the file) and goes up when the server answers again;
//! - while the game is closed: a newer head on the server is pulled into the slot (a backup of the replaced file is
//!   kept in the app's folder), unless the local file changed too: then both versions are kept and the player picks
//!   ("Vocês jogaram separados").

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Duration, FixedOffset, Utc};
use sha2::{Digest, Sha256};

use crate::api::{Api, ApiError, LockOutcome};
use crate::config::{AppConfig, ConfigStore, Pending, Session, Stat, WorldLink};
use crate::contract::*;
use crate::saves::{SaveStore, StoreKind};
use crate::steam;
use crate::sync_state::{self, Holder, Inputs, SyncState, Transfer};
use crate::view::{self, Snapshot};

pub const STEAM_PLAY: &str = "steam://rungameid/1203620";
pub const STABLE: Duration = Duration::seconds(5);
pub const HEARTBEAT: Duration = Duration::seconds(60);
pub const FETCH_IDLE: Duration = Duration::seconds(10);
pub const FETCH_PRESENCE: Duration = Duration::seconds(30);

pub fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn join_url(steam_id: &str) -> String {
    format!("steam://friends/joingame/{steam_id}")
}

/// The save stores this PC has (either may be missing: no Steam, no local folder yet is fine, it is created).
#[derive(Clone, Default)]
pub struct Stores {
    pub local: Option<Arc<dyn SaveStore>>,
    pub steam: Option<Arc<dyn SaveStore>>,
}

impl Stores {
    pub fn get(&self, kind: StoreKind) -> Option<Arc<dyn SaveStore>> {
        match kind {
            StoreKind::Local => self.local.clone(),
            StoreKind::Steam => self.steam.clone(),
        }
    }
}

pub type Publisher = Arc<dyn Fn(&Snapshot) + Send + Sync>;

/// What the engine knows of one shared world that is not in the config. The fields of the world being worked on
/// live in `Runtime` itself (`world`, `lock_mine`, ...); `select` swaps them with this stash.
#[derive(Debug, Default, Clone)]
pub struct WorldRt {
    pub world: Option<World>,
    pub lock_mine: bool,
    last_fetch: Option<DateTime<Utc>>,
    last_heartbeat: Option<DateTime<Utc>>,
    watch: Option<Stat>,
    stable_since: Option<DateTime<Utc>>,
    dirty: bool,
    wrote: bool,
}

/// What the engine knows that is not in the config: the server's view, the game, the watch, a transfer.
#[derive(Debug, Default, Clone)]
pub struct Runtime {
    /// the world `cfg.world` and the per-world fields below are about
    pub cur: Option<String>,
    /// the other worlds' per-world fields
    pub stash: std::collections::BTreeMap<String, WorldRt>,
    /// the world whose "Jogar" started the game: its lease is taken at game start
    pub launched: Option<String>,
    /// the world a transfer under way is for
    pub transfer_world: Option<String>,
    /// the slot of this world was written during this game session (its lease is taken then)
    wrote: bool,
    pub world: Option<World>,
    pub online: bool,
    /// the server answered at least once since the app started
    pub contacted: bool,
    pub presence: Vec<PresencePlayer>,
    pub game_running: bool,
    /// the game closed less than a minute ago (process::game_settled): no pull into the saves yet, Steam is still
    /// syncing the game's cloud files
    pub just_closed: bool,
    pub lock_mine: bool,
    pub transfer: Transfer,
    pub error: Option<String>,
    pub now: DateTime<Utc>,
    pub offset: Option<FixedOffset>,
    last_fetch: Option<DateTime<Utc>>,
    last_presence: Option<DateTime<Utc>>,
    last_heartbeat: Option<DateTime<Utc>>,
    watch: Option<Stat>,
    stable_since: Option<DateTime<Utc>>,
    dirty: bool,
    ticked: bool,
    pub local_player: Option<Player>,
    nick_checked: bool,
    pub login: Option<LoginStart>,
    last_login_poll: Option<DateTime<Utc>>,
    pub mismatch: Option<LoginDone>,
    pub login_error: Option<String>,
}

pub struct Engine {
    api: Box<dyn Api>,
    store: ConfigStore,
    pub cfg: AppConfig,
    data_dir: PathBuf,
    pub stores: Stores,
    avatar: Option<PathBuf>,
    pub rt: Runtime,
    publish: Option<Publisher>,
}

type R<T> = Result<T, String>;

fn api_text(e: &ApiError) -> String {
    match e {
        ApiError::Offline(_) => "Sem conexão com o servidor. Tente de novo quando a conexão voltar.".into(),
        ApiError::Http { code, .. } if code == "invite_not_found" => "Esse código não existe. Confira o que seu amigo mandou.".into(),
        ApiError::Http { code, .. } if code == "invite_used" => "Esse código já foi usado. Peça um novo para seu amigo.".into(),
        ApiError::Http { code, .. } if code == "invite_expired" => "Esse código venceu (vale por 24 horas). Peça um novo para seu amigo.".into(),
        ApiError::Http { status: 429, .. } => "Muitos códigos errados seguidos. Espere uns minutos e tente de novo.".into(),
        ApiError::Http { code, .. } if code == "world_not_found" => "O servidor não achou esse mundo para este PC.".into(),
        ApiError::Http { status: 404, .. } => "O servidor não achou isso (código vencido ou mundo removido).".into(),
        ApiError::Http { status: 401 | 403, .. } => "O servidor não reconheceu este PC.".into(),
        ApiError::Http { status, message, .. } => format!("O servidor recusou ({status}): {message}"),
    }
}

impl Engine {
    pub fn new(api: Box<dyn Api>, data_dir: PathBuf, stores: Stores, avatar: Option<PathBuf>) -> Engine {
        let store = ConfigStore::new(&data_dir);
        let cfg = store.load();
        api.set_token(cfg.token.clone());
        let mut e = Engine { api, store, cfg, data_dir, stores, avatar, rt: Runtime { now: Utc::now(), ..Default::default() }, publish: None };
        if let Some(first) = e.cfg.worlds.first().map(|w| w.world_id.clone()) {
            let _ = e.select(&first);
        }
        e
    }

    // ------------------------------------------------------------------ several worlds

    /// Writes the world being worked on back into the list.
    fn flush_cur(&mut self) {
        if let Some(l) = self.cfg.world.clone() {
            match self.cfg.worlds.iter_mut().find(|w| w.world_id == l.world_id) {
                Some(w) => *w = l,
                None => self.cfg.worlds.push(l),
            }
        }
    }

    fn take_rt(&mut self) -> WorldRt {
        WorldRt {
            world: self.rt.world.take(),
            lock_mine: std::mem::take(&mut self.rt.lock_mine),
            last_fetch: self.rt.last_fetch.take(),
            last_heartbeat: self.rt.last_heartbeat.take(),
            watch: self.rt.watch.take(),
            stable_since: self.rt.stable_since.take(),
            dirty: std::mem::take(&mut self.rt.dirty),
            wrote: std::mem::take(&mut self.rt.wrote),
        }
    }

    fn put_rt(&mut self, w: WorldRt) {
        self.rt.world = w.world;
        self.rt.lock_mine = w.lock_mine;
        self.rt.last_fetch = w.last_fetch;
        self.rt.last_heartbeat = w.last_heartbeat;
        self.rt.watch = w.watch;
        self.rt.stable_since = w.stable_since;
        self.rt.dirty = w.dirty;
        self.rt.wrote = w.wrote;
    }

    /// Makes `world_id` the world the engine works on (`cfg.world`, `rt.world`, `rt.lock_mine`...).
    pub fn select(&mut self, world_id: &str) -> R<()> {
        if self.rt.cur.as_deref() == Some(world_id) && self.cfg.world.is_some() {
            return Ok(());
        }
        self.flush_cur();
        let target = self.cfg.worlds.iter().find(|w| w.world_id == world_id).cloned().ok_or("Esse mundo não está neste PC.")?;
        if let Some(cur) = self.rt.cur.take() {
            let stashed = self.take_rt();
            self.rt.stash.insert(cur, stashed);
        }
        let next = self.rt.stash.remove(world_id).unwrap_or_default();
        self.put_rt(next);
        self.cfg.world = Some(target);
        self.rt.cur = Some(world_id.to_string());
        Ok(())
    }

    /// Every shared world's id, in the order they were added.
    pub fn world_ids(&mut self) -> Vec<String> {
        self.flush_cur();
        self.cfg.worlds.iter().map(|w| w.world_id.clone()).collect()
    }

    /// A new shared world: added to the list and made the one worked on.
    fn add_world(&mut self, link: WorldLink, world: Option<World>) {
        let id = link.world_id.clone();
        self.flush_cur();
        if !self.cfg.worlds.iter().any(|w| w.world_id == id) {
            self.cfg.worlds.push(link);
        }
        let _ = self.select(&id);
        if world.is_some() {
            self.rt.world = world;
        }
        self.save();
    }

    /// The slots of the shared worlds of a store (so a new world never takes one of them).
    pub fn taken_slots(&mut self, kind: StoreKind) -> Vec<u8> {
        self.flush_cur();
        self.cfg.worlds.iter().filter(|w| w.store == kind).map(|w| w.slot).collect()
    }

    /// Runs `f` on every shared world in turn, and comes back to the one that was selected.
    pub fn each_world(&mut self, mut f: impl FnMut(&mut Engine)) {
        let back = self.rt.cur.clone();
        for id in self.world_ids() {
            if self.select(&id).is_ok() {
                f(self);
            }
        }
        if let Some(b) = back {
            let _ = self.select(&b);
        }
    }

    /// Forgets every world (another player logged in on this PC).
    fn clear_worlds(&mut self) {
        self.cfg.world = None;
        self.cfg.worlds.clear();
        self.rt.cur = None;
        self.rt.stash.clear();
        let _ = self.take_rt();
    }

    fn release_all(&mut self) {
        self.each_world(|e| e.release());
    }

    pub fn set_publisher(&mut self, publish: Publisher) {
        self.publish = Some(publish);
    }

    pub fn save(&mut self) {
        self.flush_cur();
        if let Err(e) = self.store.save(&self.cfg) {
            self.rt.error = Some(format!("Não deu para gravar a configuração do app: {e}"));
        }
    }

    /// The Steam account of this PC (read at start). Another account than the config's starts over with a new
    /// device (its token belongs to the other player).
    /// The Steam account of this PC, as `loginusers.vdf` says (read at start). The server's player (`cfg.player`)
    /// comes only from a Steam login; this one is what the login is compared with, and its PersonaName is sent with
    /// `PATCH /api/me` when it changes.
    pub fn set_player(&mut self, player: Player) {
        self.rt.local_player = Some(player);
        self.rt.nick_checked = false;
    }

    /// The player this app is logged in as (proven by Steam), else this PC's Steam account.
    pub fn me(&self) -> Option<&str> {
        self.cfg.player.as_ref().or(self.rt.local_player.as_ref()).map(|p| p.steam_id.as_str())
    }

    pub fn logged_in(&self) -> bool {
        self.cfg.token.is_some()
    }

    // ------------------------------------------------------------------ Steam login (CONTRACT.md "Steam login")

    /// "Entrar com a Steam": a login session on the server; the URL is opened in the browser by the glue.
    pub fn login_start(&mut self) -> R<String> {
        let nick = self.rt.local_player.as_ref().map(|p| p.nick.clone()).unwrap_or_default();
        self.rt.login_error = None;
        self.rt.mismatch = None;
        let start = self.call(|a| a.login_start(&nick)).map_err(|e| api_text(&e))?;
        let url = start.login_url.clone();
        self.rt.login = Some(start);
        self.rt.last_login_poll = None;
        Ok(url)
    }

    pub fn login_cancel(&mut self) {
        self.rt.login = None;
        self.rt.mismatch = None;
    }

    fn poll_login(&mut self) {
        let Some(start) = self.rt.login.clone() else { return };
        if !Self::due(self.rt.last_login_poll, self.rt.now, Duration::seconds(2)) {
            return;
        }
        self.rt.last_login_poll = Some(self.rt.now);
        match self.call(|a| a.login_poll(&start.session_id)) {
            Ok(crate::api::LoginPoll::Pending) => {
                if self.rt.now > start.expires_at {
                    self.rt.login = None;
                    self.rt.login_error = Some("O login expirou. Tente de novo.".into());
                }
            }
            Ok(crate::api::LoginPoll::Done(done)) => {
                self.rt.login = None;
                let local = self.rt.local_player.as_ref().map(|p| p.steam_id.clone());
                if local.is_some_and(|id| id != done.player.steam_id) {
                    // logged into another Steam account in the browser: said, and the player picks
                    self.rt.mismatch = Some(done);
                } else {
                    self.accept_login(done);
                }
            }
            Ok(crate::api::LoginPoll::Gone) => {
                self.rt.login = None;
                self.rt.login_error = Some("O login expirou. Tente de novo.".into());
            }
            Err(e) if !e.is_offline() => {
                self.rt.login = None;
                self.rt.login_error = Some(api_text(&e));
            }
            Err(_) => {}
        }
    }

    fn accept_login(&mut self, done: LoginDone) {
        if self.cfg.player.as_ref().is_some_and(|p| p.steam_id != done.player.steam_id) {
            // another player on this PC: the worlds were the other one's
            self.clear_worlds();
        }
        self.api.set_token(Some(done.token.clone()));
        self.cfg.token = Some(done.token);
        self.cfg.player = Some(done.player);
        self.cfg.avatar_sent = false;
        self.rt.nick_checked = false;
        self.save();
        self.refresh();
    }

    /// The mismatch screen's "Continuar como <conta do navegador>".
    pub fn login_accept_mismatch(&mut self) -> R<()> {
        let done = self.rt.mismatch.take().ok_or("Nenhum login esperando.")?;
        self.accept_login(done);
        Ok(())
    }

    /// "Sair": this device's token is revoked on the server (`DELETE /api/devices/current`) and forgotten here. The
    /// world's mapping stays, so logging in again picks it up.
    pub fn logout(&mut self) {
        if self.cfg.token.is_some() {
            self.release_all();
            let _ = self.call(|a| a.logout());
        }
        self.api.set_token(None);
        self.cfg.token = None;
        self.cfg.player = None;
        self.rt.world = None;
        self.rt.presence.clear();
        self.save();
    }

    fn check_nick(&mut self) {
        if self.rt.nick_checked || self.cfg.token.is_none() {
            return;
        }
        let (Some(local), Some(server)) = (self.rt.local_player.clone(), self.cfg.player.clone()) else { return };
        if local.steam_id != server.steam_id || local.nick == server.nick {
            self.rt.nick_checked = true;
            return;
        }
        if let Ok(p) = self.call(|a| a.patch_me(&local.nick)) {
            self.cfg.player = Some(p);
            self.save();
            self.rt.nick_checked = true;
        }
    }

    fn data(&self, sub: &str) -> PathBuf {
        self.data_dir.join(sub)
    }

    fn link_store(&self) -> R<(WorldLink, Arc<dyn SaveStore>)> {
        let link = self.cfg.world.clone().ok_or("Nenhum mundo compartilhado neste PC.")?;
        let store = self.stores.get(link.store).ok_or(match link.store {
            StoreKind::Steam => "A pasta de saves do Steam não foi achada neste PC.",
            StoreKind::Local => "A pasta de saves do jogo não foi achada neste PC.",
        })?;
        Ok((link, store))
    }

    fn note_api(&mut self, e: &ApiError) {
        // a revoked device (logged out elsewhere, or removed): back to "Entrar com a Steam"
        if e.status() == Some(401) && self.cfg.token.is_some() {
            self.cfg.token = None;
            self.api.set_token(None);
            self.rt.lock_mine = false;
            for w in self.rt.stash.values_mut() {
                w.lock_mine = false;
            }
            self.rt.login_error = Some("Este PC saiu da conta. Entre com a Steam de novo.".into());
            self.save();
        }
        if e.is_offline() {
            self.rt.online = false;
        } else {
            self.rt.online = true;
            self.rt.contacted = true;
        }
    }

    fn ok_api(&mut self) {
        self.rt.online = true;
        self.rt.contacted = true;
    }

    fn call<T>(&mut self, f: impl FnOnce(&dyn Api) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let r = f(self.api.as_ref());
        match &r {
            Ok(_) => self.ok_api(),
            Err(e) => self.note_api(e),
        }
        r
    }

    // ------------------------------------------------------------------ device

    pub fn ensure_device(&mut self) -> R<()> {
        if self.cfg.token.is_some() {
            Ok(())
        } else {
            Err("Entre com a Steam primeiro.".into())
        }
    }

    fn send_avatar(&mut self) {
        if self.cfg.avatar_sent || self.cfg.token.is_none() {
            return;
        }
        let Some(png) = self.avatar.as_ref().and_then(|p| fs::read(p).ok()) else { return };
        if self.call(|a| a.put_avatar(&png)).is_ok() {
            self.cfg.avatar_sent = true;
            self.save();
        }
    }

    // ------------------------------------------------------------------ server view

    fn fetch(&mut self) {
        let Some(id) = self.cfg.world.as_ref().map(|w| w.world_id.clone()) else { return };
        self.rt.last_fetch = Some(self.rt.now);
        match self.call(|a| a.get_world(&id)) {
            Ok(w) => {
                if let Some(link) = self.cfg.world.as_mut() {
                    if link.name != w.name {
                        link.name = w.name.clone();
                    }
                    // a branch of ours that the server resolved away (or made head) is settled
                    if !w.diverged && link.branch.is_some() {
                        link.branch = None;
                    }
                }
                self.rt.world = Some(w);
            }
            Err(e) if !e.is_offline() => self.rt.error = Some(api_text(&e)),
            Err(_) => {}
        }
    }

    fn fetch_presence(&mut self) {
        self.rt.last_presence = Some(self.rt.now);
        if let Ok(p) = self.call(|a| a.presence()) {
            self.rt.presence = p;
        }
    }

    fn due(last: Option<DateTime<Utc>>, now: DateTime<Utc>, every: Duration) -> bool {
        last.is_none_or(|t| now - t >= every || now < t)
    }

    /// Who holds the lease, as far as this PC knows (nobody when the server is not answering).
    pub fn holder(&self) -> Holder {
        if !self.rt.online {
            return Holder::None;
        }
        if self.rt.lock_mine {
            return Holder::Me;
        }
        match self.rt.world.as_ref().and_then(|w| w.lock.as_ref()) {
            Some(l) if l.expires_at > self.rt.now => {
                if Some(l.holder.as_str()) == self.me() {
                    Holder::Me
                } else {
                    Holder::Other
                }
            }
            _ => Holder::None,
        }
    }

    pub fn foreign_lock(&self) -> Option<&Lock> {
        (self.holder() == Holder::Other).then(|| self.rt.world.as_ref()?.lock.as_ref()).flatten()
    }

    // ------------------------------------------------------------------ the local file

    /// Whether the slot's file differs from the version it came from (size+mtime first, the hash when they moved).
    pub fn local_changed(&mut self) -> bool {
        let Ok((link, store)) = self.link_store() else { return false };
        let Some(stat) = store.stat(link.slot) else { return false };
        if link.base_stat == Some(stat) {
            return false;
        }
        let Some(base_sha) = link.base_sha256.clone() else { return true };
        let Ok(bytes) = store.read(link.slot) else { return false };
        if sha_hex(&bytes) == base_sha {
            if let Some(l) = self.cfg.world.as_mut() {
                l.base_stat = Some(stat);
            }
            self.save();
            false
        } else {
            true
        }
    }

    pub fn behind(&self) -> bool {
        let head = self.rt.world.as_ref().and_then(|w| w.head.as_ref()).map(|h| h.version_id.as_str());
        let base = self.cfg.world.as_ref().and_then(|l| l.base.as_deref());
        head.is_some() && head != base
    }

    pub fn inputs(&mut self) -> Inputs {
        let local_changed = self.cfg.world.is_some() && self.local_changed();
        Inputs {
            online: self.rt.online,
            game_running: self.rt.game_running,
            lock: self.holder(),
            server_diverged: self.rt.world.as_ref().is_some_and(|w| w.diverged) || self.cfg.world.as_ref().is_some_and(|l| l.branch.is_some()),
            behind: self.behind(),
            local_changed,
            transfer: if self.rt.transfer_world == self.rt.cur { self.rt.transfer } else { Transfer::Idle },
        }
    }

    pub fn state(&mut self) -> SyncState {
        sync_state::derive(&self.inputs())
    }

    pub fn snapshot(&mut self) -> Snapshot {
        view::snapshot(self)
    }

    pub fn publish_now(&mut self) {
        if let Some(p) = self.publish.clone() {
            let s = self.snapshot();
            p(&s);
        }
    }

    /// Runs `f` with a progress callback that publishes the snapshot with the transfer's bytes.
    fn with_transfer<T>(&mut self, start: Transfer, f: impl FnOnce(&dyn Api, &mut dyn FnMut(u64, u64)) -> T) -> T {
        self.rt.transfer = start;
        self.rt.transfer_world = self.rt.cur.clone();
        let base = if self.publish.is_some() { Some(self.snapshot()) } else { None };
        if let (Some(p), Some(s)) = (&self.publish, &base) {
            p(s);
        }
        let publish = self.publish.clone();
        let wid = self.rt.cur.clone();
        let is_up = matches!(start, Transfer::Up { .. });
        let mut last_pct = 255u8;
        let mut progress = |done: u64, total: u64| {
            let pct = crate::format::percent(done, total);
            if pct == last_pct {
                return;
            }
            last_pct = pct;
            if let (Some(p), Some(s)) = (&publish, &base) {
                let mut s = s.clone();
                view::patch_progress(&mut s, wid.as_deref(), if is_up { Transfer::Up { done, total } } else { Transfer::Down { done, total } });
                p(&s);
            }
        };
        let out = f(self.api.as_ref(), &mut progress);
        self.rt.transfer = Transfer::Idle;
        self.rt.transfer_world = None;
        out
    }

    // ------------------------------------------------------------------ the loop

    pub fn tick(&mut self, now: DateTime<Utc>, offset: FixedOffset, game_running: bool) {
        self.rt.now = now;
        self.rt.offset = Some(offset);
        let was = self.rt.game_running;
        let first = !self.rt.ticked;
        self.rt.ticked = true;
        self.rt.game_running = game_running;

        self.poll_login();
        if self.cfg.token.is_none() {
            return;
        }
        self.check_nick();
        if Self::due(self.rt.last_presence, now, FETCH_PRESENCE) {
            self.fetch_presence();
            self.send_avatar();
        }
        let starting = game_running && (!was || first);
        let ending = !game_running && was;
        self.each_world(|e| e.tick_world(starting, ending));
        if ending {
            self.rt.launched = None;
        }
    }

    /// One shared world's part of the tick (it is the selected one).
    fn tick_world(&mut self, starting: bool, ending: bool) {
        let now = self.rt.now;
        let game_running = self.rt.game_running;
        let every = if game_running { Duration::seconds(15) } else { FETCH_IDLE };
        if Self::due(self.rt.last_fetch, now, every) {
            self.fetch();
        }
        if starting {
            self.start_session();
        }
        if ending {
            self.end_session();
        }
        if game_running {
            self.while_running();
        } else {
            // a session left over by an app that closed with the game open
            if self.cfg.world.as_ref().is_some_and(|l| l.session.is_some()) {
                self.end_session();
            }
            self.while_closed();
        }
    }

    fn start_session(&mut self) {
        let now = self.rt.now;
        let Ok((link, store)) = self.link_store() else { return };
        if link.session.is_none() {
            // to the millisecond, as the server keeps it: the session's uploads all carry this very start
            let started = DateTime::<Utc>::from_timestamp_millis(now.timestamp_millis()).unwrap_or(now);
            if let Some(l) = self.cfg.world.as_mut() {
                l.session = Some(Session { started_at: started, last_autosave_at: None });
            }
            self.save();
        }
        self.rt.watch = store.stat(link.slot);
        self.rt.stable_since = None;
        self.rt.dirty = false;
        self.rt.wrote = false;
        // the game does not say which world it opens: the lease is taken now only for the world its "Jogar"
        // started; the others take theirs on the first write of their slot (while_running)
        if self.rt.launched.is_some() && self.rt.launched == self.rt.cur {
            self.try_lock();
        }
    }

    /// Takes the lease when it is free and this PC is not behind (playing an old copy as host would lose the
    /// friend's session). Returns who holds it after.
    fn try_lock(&mut self) -> Holder {
        if self.rt.lock_mine {
            return Holder::Me;
        }
        if self.behind() && self.rt.game_running {
            return self.holder();
        }
        let Some(id) = self.cfg.world.as_ref().map(|w| w.world_id.clone()) else { return Holder::None };
        match self.call(|a| a.lock(&id)) {
            Ok(LockOutcome::Taken(lock)) => {
                self.rt.lock_mine = true;
                self.rt.last_heartbeat = Some(self.rt.now);
                if let Some(w) = self.rt.world.as_mut() {
                    w.lock = Some(lock);
                }
                Holder::Me
            }
            Ok(LockOutcome::Busy(lock)) => {
                if let Some(w) = self.rt.world.as_mut() {
                    w.lock = Some(lock);
                }
                Holder::Other
            }
            Err(_) => Holder::None,
        }
    }

    fn release(&mut self) {
        if !self.rt.lock_mine {
            return;
        }
        let Some(id) = self.cfg.world.as_ref().map(|w| w.world_id.clone()) else { return };
        // a release that does not reach the server is fine: the lease expires by itself
        let _ = self.call(|a| a.release(&id));
        self.rt.lock_mine = false;
        if let Some(w) = self.rt.world.as_mut() {
            if w.lock.as_ref().is_some_and(|l| Some(l.holder.as_str()) == self.cfg.player.as_ref().map(|p| p.steam_id.as_str())) {
                w.lock = None;
            }
        }
    }

    fn while_running(&mut self) {
        let now = self.rt.now;
        if self.rt.lock_mine {
            if Self::due(self.rt.last_heartbeat, now, HEARTBEAT) {
                let id = self.cfg.world.as_ref().unwrap().world_id.clone();
                match self.call(|a| a.heartbeat(&id)) {
                    Ok(lock) => {
                        self.rt.last_heartbeat = Some(now);
                        if let Some(w) = self.rt.world.as_mut() {
                            w.lock = Some(lock);
                        }
                    }
                    Err(e) if !e.is_offline() => self.rt.lock_mine = false,
                    Err(_) => {}
                }
            }
        } else if self.rt.online
            && (self.holder() == Holder::Me || (self.holder() == Holder::None && (self.rt.wrote || (self.rt.launched.is_some() && self.rt.launched == self.rt.cur))))
        {
            // the server still names this PC (the app restarted with the game open: the holder's lock POST renews
            // it), or this world is the one being played (launched by its Jogar, or its slot was written)
            self.try_lock();
        }
        // a session that waited in the queue goes first, so this session's uploads chain from it
        if self.rt.lock_mine && self.cfg.world.as_ref().is_some_and(|l| l.pending.is_some()) {
            self.flush_pending(true);
        }
        let Ok((link, store)) = self.link_store() else { return };
        let stat = store.stat(link.slot);
        if stat != self.rt.watch {
            self.rt.watch = stat;
            self.rt.stable_since = Some(now);
            if !self.rt.wrote {
                // the first write of this slot in the session: this is the world being played, take its lease
                self.rt.wrote = true;
                if !self.rt.lock_mine && self.rt.online && self.holder() == Holder::None {
                    self.try_lock();
                }
            }
        }
        if self.rt.stable_since.is_some_and(|t| now - t >= STABLE) {
            self.rt.stable_since = None;
            self.rt.dirty = true;
        }
        if self.rt.dirty && self.rt.lock_mine {
            if !self.local_changed() {
                self.rt.dirty = false;
            } else if self.upload_current(None).is_ok() {
                self.rt.dirty = false;
                if let Some(s) = self.cfg.world.as_mut().and_then(|l| l.session.as_mut()) {
                    s.last_autosave_at = Some(now);
                }
                self.save();
            }
        }
    }

    fn end_session(&mut self) {
        let now = self.rt.now;
        let started = self.cfg.world.as_mut().and_then(|l| l.session.take()).map(|s| s.started_at);
        self.save();
        self.rt.stable_since = None;
        self.rt.dirty = false;
        let diverged = self.rt.world.as_ref().is_some_and(|w| w.diverged);
        if self.local_changed() && !diverged {
            self.queue_pending(started.unwrap_or(now), Some(now));
        }
        if self.cfg.world.as_ref().is_some_and(|l| l.pending.is_some()) {
            self.flush_pending(false);
        }
        self.release();
    }

    fn while_closed(&mut self) {
        if self.rt.transfer != Transfer::Idle {
            return;
        }
        let pending = self.cfg.world.as_ref().is_some_and(|l| l.pending.is_some());
        let settled = self.cfg.world.as_ref().is_some_and(|l| l.branch.is_none()) && !self.rt.world.as_ref().is_some_and(|w| w.diverged);
        if pending {
            if self.rt.online || Self::due(self.rt.last_fetch, self.rt.now, FETCH_IDLE) {
                self.flush_pending(false);
            }
            return;
        }
        if !self.rt.online || !settled {
            return;
        }
        if self.local_changed() {
            if !self.behind() {
                // played while this app was closed: that session goes up now (its times are the file's)
                let (link, store) = match self.link_store() {
                    Ok(x) => x,
                    Err(_) => return,
                };
                let at = store.stat(link.slot).and_then(|s| DateTime::<Utc>::from_timestamp_millis(s.mtime)).unwrap_or(self.rt.now);
                self.queue_pending(at, Some(at));
                self.flush_pending(false);
            }
            // behind and changed: "Vocês jogaram separados", the player picks
            return;
        }
        if self.behind() && self.holder() != Holder::Other && !self.rt.just_closed {
            let head = self.rt.world.as_ref().and_then(|w| w.head.clone()).unwrap();
            if let Err(e) = self.pull(&head) {
                self.rt.error = Some(e);
            }
        }
    }

    // ------------------------------------------------------------------ upload

    /// A snapshot copy of the slot's file in the queue folder, with the headers it goes up with.
    fn queue_pending(&mut self, started: DateTime<Utc>, ended: Option<DateTime<Utc>>) {
        let Ok((link, store)) = self.link_store() else { return };
        let Ok(bytes) = store.read(link.slot) else { return };
        let dir = self.data("queue");
        let file = dir.join(format!("{}.ksd", link.world_id));
        if fs::create_dir_all(&dir).and_then(|_| fs::write(&file, &bytes)).is_err() {
            self.rt.error = Some("Não deu para guardar a sessão na fila do app.".into());
            return;
        }
        let pending = Pending { file, sha256: sha_hex(&bytes), size: bytes.len() as u64, parent: link.base.clone(), started_at: started, ended_at: ended };
        if let Some(l) = self.cfg.world.as_mut() {
            l.pending = Some(pending);
        }
        self.save();
    }

    /// Sends the queued upload: takes the lease if needed (waits while someone else hosts), and gives it back after
    /// unless `keep_lock` (the game is running).
    fn flush_pending(&mut self, keep_lock: bool) {
        let Some(p) = self.cfg.world.as_ref().and_then(|l| l.pending.clone()) else { return };
        match self.try_lock() {
            Holder::Me => {}
            _ => return,
        }
        let Ok(bytes) = fs::read(&p.file) else {
            // the copy is gone: nothing to send
            if let Some(l) = self.cfg.world.as_mut() {
                l.pending = None;
            }
            self.save();
            return;
        };
        let headers = UploadHeaders { parent: p.parent.clone(), sha256: p.sha256.clone(), started_at: p.started_at, ended_at: p.ended_at };
        if self.send(&bytes, &headers).is_ok() {
            let _ = fs::remove_file(&p.file);
            if let Some(l) = self.cfg.world.as_mut() {
                l.pending = None;
            }
            self.save();
            self.fetch();
        }
        if !keep_lock {
            self.release();
        }
    }

    /// Uploads the slot's file as it is now (`ended`: None for an autosave).
    fn upload_current(&mut self, ended: Option<DateTime<Utc>>) -> R<()> {
        let (link, store) = self.link_store()?;
        let bytes = store.read(link.slot).map_err(|e| format!("Não deu para ler o mundo: {e}"))?;
        let started = link.session.as_ref().map(|s| s.started_at).unwrap_or_else(|| DateTime::<Utc>::from_timestamp_millis(self.rt.now.timestamp_millis()).unwrap_or(self.rt.now));
        let headers = UploadHeaders { parent: link.base.clone(), sha256: sha_hex(&bytes), started_at: started, ended_at: ended };
        self.send(&bytes, &headers).map_err(|e| api_text(&e))?;
        if let Some(stat) = store.stat(link.slot) {
            if store.read(link.slot).is_ok_and(|now| sha_hex(&now) == headers.sha256) {
                if let Some(l) = self.cfg.world.as_mut() {
                    l.base_stat = Some(stat);
                }
                self.save();
            }
        }
        Ok(())
    }

    fn send(&mut self, bytes: &[u8], headers: &UploadHeaders) -> Result<(), ApiError> {
        let id = self.cfg.world.as_ref().map(|w| w.world_id.clone()).unwrap_or_default();
        let total = bytes.len() as u64;
        let r = self.with_transfer(Transfer::Up { done: 0, total }, |api, progress| api.upload(&id, bytes, headers, progress));
        match &r {
            Ok(_) => self.ok_api(),
            Err(e) => self.note_api(e),
        }
        let resp = r?;
        let vid = resp.version.as_ref().map(|v| v.version_id.clone());
        if let Some(l) = self.cfg.world.as_mut() {
            let became_head = !resp.diverged || (vid.is_some() && vid == resp.head);
            if became_head {
                l.base = vid.or(resp.head.clone());
                l.base_sha256 = Some(headers.sha256.clone());
                l.base_stat = None;
            } else {
                l.branch = vid;
            }
        }
        if let (Some(w), Some(v)) = (self.rt.world.as_mut(), resp.version) {
            if !resp.diverged {
                w.head = Some(v.clone());
            }
            w.diverged = resp.diverged;
            w.history.insert(0, v);
        }
        self.save();
        Ok(())
    }

    // ------------------------------------------------------------------ pull

    /// Puts `version` in the slot: downloaded whole and checked against its sha256, the replaced file kept in
    /// `<data>\backup\world<N>.ksd.shroudtopia-<versionId>`, then written in one go. Never while the game runs.
    pub fn pull(&mut self, version: &Version) -> R<()> {
        if self.rt.game_running {
            return Err("Feche o Enshrouded para baixar o mundo.".into());
        }
        let (link, store) = self.link_store()?;
        let id = link.world_id.clone();
        let vid = version.version_id.clone();
        let mut bytes = Vec::with_capacity(version.size as usize);
        let r = self.with_transfer(Transfer::Down { done: 0, total: version.size }, |api, progress| api.download(&id, &vid, &mut bytes, progress));
        match &r {
            Ok(_) => self.ok_api(),
            Err(e) => self.note_api(e),
        }
        r.map_err(|e| api_text(&e))?;
        if sha_hex(&bytes) != version.sha256 {
            return Err("O arquivo baixado não confere com o do servidor. Nada foi trocado.".into());
        }
        if self.rt.game_running {
            return Err("O Enshrouded abriu durante o download; o mundo baixa quando ele fechar.".into());
        }
        if let Ok(old) = store.read(link.slot) {
            let dir = self.data("backup");
            let backup = dir.join(format!("{}.shroudtopia-{}", steam::slot_file(link.slot), vid));
            fs::create_dir_all(&dir).and_then(|_| fs::write(&backup, &old)).map_err(|e| format!("Não deu para guardar a cópia de segurança do mundo: {e}. Nada foi trocado."))?;
        }
        store.write(link.slot, &bytes).map_err(|e| format!("Não deu para gravar o mundo no save: {e}"))?;
        if let Some(l) = self.cfg.world.as_mut() {
            l.base = Some(vid);
            l.base_sha256 = Some(version.sha256.clone());
            l.base_stat = store.stat(link.slot);
            l.branch = None;
        }
        self.save();
        Ok(())
    }

    // ------------------------------------------------------------------ the window's actions

    /// "Jogar": pulls first when behind (and nothing would be lost), then the URL that starts the game.
    pub fn play(&mut self, world_id: Option<&str>) -> R<&'static str> {
        if let Some(id) = world_id {
            self.select(id)?;
        }
        if self.rt.game_running {
            return Err("O Enshrouded já está aberto.".into());
        }
        if self.rt.just_closed {
            return Err("O Enshrouded acabou de fechar. Espera um minuto e clica em Jogar de novo.".into());
        }
        if self.cfg.world.is_some() {
            self.fetch();
            if self.rt.online && self.behind() && !self.local_changed() && self.holder() != Holder::Other {
                let head = self.rt.world.as_ref().and_then(|w| w.head.clone()).unwrap();
                self.pull(&head)?;
            }
            self.rt.launched = self.rt.cur.clone();
        }
        Ok(STEAM_PLAY)
    }

    /// The two versions of a divergence: this PC's and the other's.
    pub fn competing(&self) -> (Option<Version>, Option<Version>) {
        let me = self.me().map(str::to_string);
        let Some(w) = self.rt.world.clone() else { return (None, None) };
        let branch = self.cfg.world.as_ref().and_then(|l| l.branch.clone());
        let mine = w.head.clone().filter(|h| Some(&h.uploaded_by) == me.as_ref()).or_else(|| branch.and_then(|b| w.history.iter().find(|v| v.version_id == b).cloned())).or_else(|| w.history.iter().find(|v| Some(&v.uploaded_by) == me.as_ref()).cloned());
        let theirs = w.head.clone().filter(|h| Some(&h.uploaded_by) != me.as_ref()).or_else(|| w.history.iter().find(|v| Some(&v.uploaded_by) != me.as_ref()).cloned());
        (mine, theirs)
    }

    /// "Seguir com a sua" (`keep_mine`) or "Seguir com a do <amigo>". The other version stays in the history.
    pub fn resolve(&mut self, world_id: Option<&str>, keep_mine: bool) -> R<()> {
        if let Some(id) = world_id {
            self.select(id)?;
        }
        if self.rt.game_running {
            return Err("Feche o Enshrouded antes de escolher.".into());
        }
        self.fetch();
        let id = self.link_store()?.0.world_id;
        if keep_mine {
            // this PC's changes that never went up go up first (they become a branch, then the head)
            if self.local_changed() && self.cfg.world.as_ref().is_some_and(|l| l.branch.is_none()) {
                if self.cfg.world.as_ref().is_some_and(|l| l.pending.is_none()) {
                    self.queue_pending(self.rt.now, Some(self.rt.now));
                }
                self.flush_pending(false);
                if self.cfg.world.as_ref().is_some_and(|l| l.pending.is_some()) {
                    return Err("Não deu para enviar a sua versão agora. Tente de novo.".into());
                }
            }
            let vid = self.cfg.world.as_ref().and_then(|l| l.branch.clone()).or_else(|| self.competing().0.map(|v| v.version_id)).ok_or("A sua versão não está no servidor.")?;
            let w = self.call(|a| a.resolve(&id, &vid)).map_err(|e| api_text(&e))?;
            let sha = w.head.as_ref().map(|h| h.sha256.clone());
            self.rt.world = Some(w);
            if let Some(l) = self.cfg.world.as_mut() {
                l.base = Some(vid);
                l.base_sha256 = sha;
                l.base_stat = None;
                l.branch = None;
                l.pending = None;
            }
            self.save();
            Ok(())
        } else {
            let theirs = self.competing().1.ok_or("A versão do seu amigo não está no servidor.")?;
            let w = self.call(|a| a.resolve(&id, &theirs.version_id)).map_err(|e| api_text(&e))?;
            self.rt.world = Some(w);
            if let Some(l) = self.cfg.world.as_mut() {
                if let Some(p) = l.pending.take() {
                    let _ = fs::remove_file(p.file);
                }
                l.branch = None;
            }
            self.save();
            self.pull(&theirs)
        }
    }

    /// First run, "Compartilhar um mundo meu": a new world on the server, the chosen save as its first version, and
    /// an invite. A Steam Cloud save shared into the local store is copied into a free local slot first.
    pub fn share(&mut self, from: StoreKind, slot: u8, into: StoreKind, name: &str) -> R<Invite> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Dê um nome para o mundo.".into());
        }
        if self.rt.game_running && into != from {
            return Err("Feche o Enshrouded para copiar o mundo.".into());
        }
        self.ensure_device()?;
        if self.taken_slots(from).contains(&slot) {
            return Err("Esse mundo já está compartilhado.".into());
        }
        let src = self.stores.get(from).ok_or("A pasta desse mundo não foi achada.")?;
        let dst = self.stores.get(into).ok_or("A pasta de saves escolhida não foi achada.")?;
        let bytes = src.read(slot).map_err(|e| format!("Não deu para ler o mundo: {e}"))?;
        let at = src.stat(slot).and_then(|s| DateTime::<Utc>::from_timestamp_millis(s.mtime)).unwrap_or(self.rt.now);
        let slot = if from == into {
            slot
        } else {
            let taken = self.taken_slots(into);
            let free = dst.free_slot(&taken).ok_or("Não há espaço livre de save (world0 a world9 estão ocupados).")?;
            dst.write(free, &bytes).map_err(|e| format!("Não deu para copiar o mundo: {e}"))?;
            free
        };
        let world = self.call(|a| a.create_world(name)).map_err(|e| api_text(&e))?;
        let link = WorldLink { world_id: world.world_id.clone(), name: world.name.clone(), store: into, slot, base: None, base_sha256: None, base_stat: None, pending: None, branch: None, session: None };
        self.add_world(link, Some(world));
        let headers = UploadHeaders { parent: None, sha256: sha_hex(&bytes), started_at: at, ended_at: Some(at) };
        if let Err(e) = self.send(&bytes, &headers) {
            // the world exists: its first version goes up from the queue when the server answers again
            self.queue_pending(at, Some(at));
            if !e.is_offline() {
                return Err(api_text(&e));
            }
        }
        if let Some(l) = self.cfg.world.as_mut() {
            l.base_stat = dst.stat(slot);
        }
        self.save();
        self.new_invite(None)
    }

    pub fn new_invite(&mut self, world_id: Option<&str>) -> R<Invite> {
        if let Some(id) = world_id {
            self.select(id)?;
        }
        let id = self.cfg.world.as_ref().map(|w| w.world_id.clone()).ok_or("Nenhum mundo compartilhado neste PC.")?;
        self.call(|a| a.create_invite(&id)).map_err(|e| api_text(&e))
    }

    /// First run, "Tenho um código": joins the world and downloads it into a free slot of `into`.
    pub fn redeem(&mut self, code: &str, into: StoreKind) -> R<World> {
        let code = code.trim().to_uppercase();
        if code.len() < 4 {
            return Err("Cole o código que seu amigo mandou.".into());
        }
        if self.rt.game_running {
            return Err("Feche o Enshrouded para baixar o mundo.".into());
        }
        self.ensure_device()?;
        let store = self.stores.get(into).ok_or("A pasta de saves escolhida não foi achada.")?;
        let world = self.call(|a| a.redeem(&code)).map_err(|e| api_text(&e))?;
        if self.world_ids().contains(&world.world_id) {
            // already one of this PC's worlds (the server gives it back and keeps the code)
            self.select(&world.world_id)?;
            return Ok(world);
        }
        let taken = self.taken_slots(into);
        let slot = store.free_slot(&taken).ok_or("Não há espaço livre de save (world0 a world9 estão ocupados).")?;
        let link = WorldLink { world_id: world.world_id.clone(), name: world.name.clone(), store: into, slot, base: None, base_sha256: None, base_stat: None, pending: None, branch: None, session: None };
        self.add_world(link, Some(world.clone()));
        if let Some(head) = world.head.clone() {
            self.pull(&head)?;
        }
        Ok(world)
    }

    /// Where the redeemed world would go (the free slot), for the paste step.
    pub fn free_slot(&mut self, into: StoreKind) -> Option<u8> {
        let taken = self.taken_slots(into);
        self.stores.get(into)?.free_slot(&taken)
    }

    /// Forces a server refresh on the next tick (after the window asks for it).
    pub fn refresh(&mut self) {
        self.rt.last_fetch = None;
        self.rt.last_presence = None;
        for w in self.rt.stash.values_mut() {
            w.last_fetch = None;
        }
    }

    /// The world's setting "Steam" / "Local": the file is copied into a free slot of the other store (the old file
    /// stays where it was, untouched) and the world lives there from then on. Only with the game closed.
    pub fn move_store(&mut self, world_id: Option<&str>, into: StoreKind) -> R<()> {
        if let Some(id) = world_id {
            self.select(id)?;
        }
        let (link, from) = self.link_store()?;
        if link.store == into {
            return Ok(());
        }
        if self.rt.game_running {
            return Err("Feche o Enshrouded para mudar onde o mundo fica.".into());
        }
        let to = self.stores.get(into).ok_or("A pasta de saves escolhida não foi achada.")?;
        let taken = self.taken_slots(into);
        let slot = to.free_slot(&taken).ok_or("Não há espaço livre de save (world0 a world9 estão ocupados).")?;
        if let Ok(bytes) = from.read(link.slot) {
            to.write(slot, &bytes).map_err(|e| format!("Não deu para copiar o mundo: {e}"))?;
        }
        if let Some(l) = self.cfg.world.as_mut() {
            l.store = into;
            l.slot = slot;
            l.base_stat = to.stat(slot);
        }
        self.save();
        Ok(())
    }

    /// Gives the lease back now (the app is quitting with the game closed).
    pub fn forget_lock(&mut self) {
        self.release_all();
    }

    /// Stops sharing on this PC: forgets the world (the save file and the server's copy stay).
    pub fn forget_world(&mut self) {
        self.release();
        if let Some(id) = self.rt.cur.take() {
            self.cfg.worlds.retain(|w| w.world_id != id);
        }
        self.cfg.world = None;
        let _ = self.take_rt();
        self.save();
        if let Some(first) = self.cfg.worlds.first().map(|w| w.world_id.clone()) {
            let _ = self.select(&first);
        }
    }
}

#[cfg(test)]
mod tests;
