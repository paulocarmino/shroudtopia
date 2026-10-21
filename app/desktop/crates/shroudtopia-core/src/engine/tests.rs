//! The engine against the in-memory server, two PCs at a time: Paulo (`me`) and his friend (`fr`), each with an own
//! data folder and save folder in a temp dir. Time is driven by the test.

use super::*;
use crate::saves::LocalStore;
use crate::testing::FakeServer;
use chrono::TimeZone;

fn br() -> FixedOffset {
    FixedOffset::west_opt(3 * 3600).unwrap()
}

/// "Entrar com a Steam", the browser finishing as `steam_id`, and the app's poll.
fn login(server: &FakeServer, e: &mut Engine, steam_id: &str) {
    server.set_now(e.rt.now);
    let url = e.login_start().unwrap();
    assert!(url.contains("/auth/steam/redirect/"));
    let now = e.rt.now;
    e.tick(now, br(), false);
    assert!(!e.logged_in(), "still waiting for the browser");
    assert!(e.snapshot().login.waiting);
    server.complete_login(&server.last_login().unwrap(), steam_id);
    e.tick(now + Duration::seconds(2), br(), false);
}

#[test]
fn steam_login_mismatch_and_logout() {
    let dir = tempfile::tempdir().unwrap();
    let server = FakeServer::new();
    let stores = Stores { local: Some(Arc::new(LocalStore::new(dir.path().join("saves")))), steam: None };
    let mut e = Engine::new(Box::new(server.client()), dir.path().join("data"), stores, None);
    e.set_player(Player { steam_id: "76561198065863968".into(), nick: "Paulo".into() });
    assert_eq!(e.snapshot().phase, "login");
    // the browser logs into another account: said, nothing saved
    login(&server, &mut e, "76561198000000009");
    assert!(!e.logged_in());
    let s = e.snapshot();
    assert_eq!((s.login.mismatch.as_deref(), s.login.steam_nick.as_deref()), (Some("Paulo"), Some("Paulo")));
    // logging in again as the right account
    login(&server, &mut e, "76561198065863968");
    assert!(e.logged_in());
    assert_eq!(e.snapshot().phase, "setup");
    assert!(e.snapshot().login.mismatch.is_none());
    // the PersonaName changed on Steam: PATCH /api/me
    e.set_player(Player { steam_id: "76561198065863968".into(), nick: "Paulo novo".into() });
    let now = e.rt.now + Duration::seconds(5);
    e.tick(now, br(), false);
    assert_eq!(e.cfg.player.as_ref().unwrap().nick, "Paulo novo");
    e.logout();
    assert!(!e.logged_in());
    assert_eq!(e.snapshot().phase, "login");
    assert!(ConfigStore::new(&dir.path().join("data")).load().token.is_none());
}

struct Pc {
    e: Engine,
    saves: PathBuf,
    data: PathBuf,
}

impl Pc {
    fn new(server: &FakeServer, root: &std::path::Path, name: &str, steam_id: &str, nick: &str) -> Pc {
        let data = root.join(name).join("data");
        let saves = root.join(name).join("saves");
        fs::create_dir_all(&saves).unwrap();
        let stores = Stores { local: Some(Arc::new(LocalStore::new(saves.clone()))), steam: None };
        let mut e = Engine::new(Box::new(server.client()), data.clone(), stores, None);
        e.set_player(Player { steam_id: steam_id.into(), nick: nick.into() });
        login(server, &mut e, steam_id);
        Pc { e, saves, data }
    }

    fn write(&self, slot: u8, bytes: &[u8]) {
        fs::write(self.saves.join(steam::slot_file(slot)), bytes).unwrap();
    }

    fn read(&self, slot: u8) -> Vec<u8> {
        fs::read(self.saves.join(steam::slot_file(slot))).unwrap()
    }

    fn slot(&self) -> u8 {
        self.e.cfg.world.as_ref().unwrap().slot
    }
}

struct World2 {
    _dir: tempfile::TempDir,
    server: FakeServer,
    me: Pc,
    fr: Pc,
    now: DateTime<Utc>,
}

impl World2 {
    /// Paulo shares world0 ("Mundo do bang"), the friend redeems it into his free slot.
    fn new() -> World2 {
        let dir = tempfile::tempdir().unwrap();
        let server = FakeServer::new();
        let now = br().with_ymd_and_hms(2026, 10, 21, 14, 0, 0).unwrap().with_timezone(&Utc);
        server.set_now(now);
        let mut me = Pc::new(&server, dir.path(), "me", "76561198065863968", "[ツ] Ｌ　Ａ　Ｒ　Ａ　Ｎ　Ｊ　Ｏ");
        let mut fr = Pc::new(&server, dir.path(), "fr", "76561198000000001", "rodrigao_br");
        me.write(0, b"world v1");
        fr.write(0, b"the friend's own world");
        me.e.rt.now = now;
        fr.e.rt.now = now;
        let invite = me.e.share(StoreKind::Local, 0, StoreKind::Local, "Mundo do bang").unwrap();
        assert!(invite.code.starts_with("ENSH-"));
        fr.e.redeem(&invite.code, StoreKind::Local).unwrap();
        assert_eq!(fr.slot(), 1, "the friend's world0 is his own: the shared one goes to a free slot");
        assert_eq!(fr.read(1), b"world v1");
        assert_eq!(fr.read(0), b"the friend's own world", "no world of his is touched");
        let mut w = World2 { _dir: dir, server, me, fr, now };
        w.tick(false, false);
        w
    }

    fn at(&mut self, secs: i64) {
        self.now += Duration::seconds(secs);
        self.server.set_now(self.now);
    }

    fn tick(&mut self, me_game: bool, fr_game: bool) {
        self.server.set_now(self.now);
        // the fetch interval is not what these tests are about: every tick sees the server
        self.me.e.refresh();
        self.fr.e.refresh();
        self.me.e.tick(self.now, br(), me_game);
        self.fr.e.tick(self.now, br(), fr_game);
    }
}

#[test]
fn sharing_and_redeeming() {
    let mut w = World2::new();
    let world = w.server.world(&w.me.e.cfg.world.as_ref().unwrap().world_id);
    assert_eq!(world.members.len(), 2);
    let head = world.head.unwrap();
    assert_eq!(head.parent, None, "the first version has no parent");
    assert_eq!(w.me.e.cfg.world.as_ref().unwrap().base.as_deref(), Some(head.version_id.as_str()));
    assert_eq!(w.fr.e.cfg.world.as_ref().unwrap().base.as_deref(), Some(head.version_id.as_str()));
    assert_eq!(w.me.e.state(), SyncState::Sync);
    assert_eq!(w.fr.e.state(), SyncState::Sync);
    let s = w.me.e.snapshot();
    let v = s.world.unwrap();
    assert_eq!(v.status, "Em dia");
    assert_eq!(v.aside, "com rodrigao_br");
    assert_eq!(v.action.unwrap().label, "Jogar");
    assert_eq!(v.history[0].tags.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["neste PC", "no servidor"]);
    assert_eq!(s.people[1].sub, "online");
    // a wrong code
    let mut third = Pc::new(&w.server, w._dir.path(), "x", "76561198000000002", "x");
    assert!(third.e.redeem("ENSH-ZZZZ", StoreKind::Local).is_err());
}

#[test]
fn a_session_hosts_autosaves_and_closes() {
    let mut w = World2::new();
    w.at(60);
    w.tick(true, false);
    assert!(w.me.e.rt.lock_mine, "the game started: the lease is taken");
    assert_eq!(w.me.e.state(), SyncState::Me);
    assert_eq!(w.fr.e.state(), SyncState::Friend);
    let fr_view = w.fr.e.snapshot();
    let fv = fr_view.world.unwrap();
    assert_eq!(fv.status, "[ツ] Ｌ　Ａ　Ｒ　Ａ　Ｎ　Ｊ　Ｏ está hospedando");
    assert_eq!(fv.line, "Desde 14:01 · o mundo fica com ele até fechar o jogo");
    assert_eq!(fv.action.unwrap().kind, "join");
    assert_eq!(fv.history[0].tags[0].cls, "friendlive");
    let mv = w.me.e.snapshot().world.unwrap();
    assert_eq!(mv.status, "Você está hospedando");
    assert_eq!(mv.line, "Desde 14:01 · nenhum autosave enviado ainda");
    assert!(mv.action.unwrap().disabled);

    // an autosave: written, then unchanged for 5 s
    let base = w.me.e.cfg.world.as_ref().unwrap().base.clone();
    w.me.write(0, b"world v2 autosave");
    w.at(1);
    w.tick(true, false);
    w.at(3);
    w.tick(true, false);
    assert_eq!(w.server.uploads(), 1, "not yet: the file has to be still for 5 s");
    w.at(3);
    w.tick(true, false);
    assert_eq!(w.server.uploads(), 2);
    let id = w.me.e.cfg.world.as_ref().unwrap().world_id.clone();
    let head = w.server.world(&id).head.unwrap();
    assert_eq!(head.parent, base, "the autosave's parent is what the file came from");
    assert!(head.ended_at.is_none());
    assert_eq!(w.me.e.snapshot().world.unwrap().line, "Desde 14:01 · último autosave enviado às 14:01");
    // the friend does not pull while Paulo hosts
    assert_eq!(w.fr.read(1), b"world v1");

    // heartbeats keep the lease
    for _ in 0..8 {
        w.at(60);
        w.tick(true, false);
    }
    assert!(w.me.e.rt.lock_mine);
    assert_eq!(w.fr.e.state(), SyncState::Friend);

    // the game exits: the final upload, with endedAt, chained on the autosave, and the lease goes back
    w.me.write(0, b"world v3 final");
    w.at(60);
    w.tick(false, false);
    let world = w.server.world(&id);
    let fin = world.head.clone().unwrap();
    assert_eq!(fin.parent.as_deref(), Some(head.version_id.as_str()));
    assert!(fin.ended_at.is_some());
    assert!(world.lock.is_none());
    assert!(!w.me.e.rt.lock_mine);
    assert_eq!(w.me.e.state(), SyncState::Sync);
    // the friend's next tick pulls it, keeping a backup of what was there
    w.at(1);
    w.tick(false, false);
    assert_eq!(w.fr.read(1), b"world v3 final");
    let backup = w.fr.data.join("backup").join(format!("world1.ksd.shroudtopia-{}", fin.version_id));
    assert_eq!(fs::read(backup).unwrap(), b"world v1");
    assert_eq!(w.fr.e.state(), SyncState::Sync);
    let line = w.fr.e.snapshot().world.unwrap().line;
    assert!(line.starts_with("[ツ] Ｌ　Ａ　Ｒ　Ａ　Ｎ　Ｊ　Ｏ jogou por último · hoje, "), "{line}");
    assert!(line.ends_with(" de sessão"));
    // one row per session in the history (the autosave and the final upload are one)
    let rows = w.fr.e.snapshot().world.unwrap().history;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].when, "hoje, 14:01 – 14:10");
}

#[test]
fn a_pull_never_happens_with_the_game_open() {
    let mut w = World2::new();
    // the friend hosts and closes
    w.at(60);
    w.tick(false, true);
    w.fr.write(1, b"friend session");
    w.at(60);
    w.tick(false, false);
    // Paulo's game is open (in the menu): nothing is written into his slot
    w.at(1);
    w.tick(true, false);
    assert_eq!(w.me.read(0), b"world v1");
    assert_eq!(w.me.e.state(), SyncState::Behind);
    assert!(w.me.e.snapshot().world.unwrap().line.ends_with("baixa quando você fechar o jogo"));
    assert!(!w.me.e.rt.lock_mine, "playing an old copy as host would lose the friend's session");
    assert!(w.me.e.pull(&w.server.world(&w.me.e.cfg.world.as_ref().unwrap().world_id).head.unwrap()).is_err());
    w.at(30);
    w.tick(false, false);
    assert_eq!(w.me.read(0), b"friend session");
}

#[test]
fn play_pulls_first() {
    let mut w = World2::new();
    w.at(60);
    w.tick(false, true);
    w.fr.write(1, b"friend session");
    w.at(60);
    w.fr.e.refresh();
    w.fr.e.tick(w.now, br(), false);
    // Paulo's app has not ticked since: "Jogar" pulls before it starts the game
    assert_eq!(w.me.e.play().unwrap(), STEAM_PLAY);
    assert_eq!(w.me.read(0), b"friend session");
}

#[test]
fn offline_queue_goes_up_when_the_server_answers() {
    let mut w = World2::new();
    w.server.set_offline(true);
    w.at(60);
    w.tick(true, false);
    assert_eq!(w.me.e.state(), SyncState::Offline);
    let v = w.me.e.snapshot().world.unwrap();
    assert_eq!(v.status, "Sem conexão com o servidor");
    assert_eq!(v.note.unwrap().text, "Se o rodrigao_br jogar nesse meio-tempo, vocês vão ter que escolher qual versão segue.");
    w.me.write(0, b"offline session");
    w.at(3600);
    w.tick(false, false);
    let pending = w.me.e.cfg.world.as_ref().unwrap().pending.clone().expect("queued");
    assert!(pending.file.starts_with(w.me.data.join("queue")));
    // the live file changes again before the server is back: the queued snapshot is what goes up
    w.me.write(0, b"offline session");
    w.server.set_offline(false);
    w.at(10);
    w.tick(false, false);
    assert!(w.me.e.cfg.world.as_ref().unwrap().pending.is_none());
    let id = w.me.e.cfg.world.as_ref().unwrap().world_id.clone();
    let head = w.server.world(&id).head.unwrap();
    assert_eq!(w.server.file(&id, &head.version_id), b"offline session");
    assert_eq!(head.started_at, pending.started_at);
    assert!(w.server.world(&id).lock.is_none());
    w.at(10);
    w.tick(false, false);
    assert_eq!(w.fr.read(1), b"offline session");
}

#[test]
fn playing_apart_diverges_and_either_side_can_win() {
    for keep_mine in [true, false] {
        let mut w = World2::new();
        // both play offline-from-each-other: the friend uploads first, Paulo was offline
        w.server.set_offline(true);
        w.at(60);
        w.me.e.tick(w.now, br(), true);
        w.me.write(0, b"paulo apart");
        w.at(600);
        w.me.e.tick(w.now, br(), false);
        assert!(w.me.e.cfg.world.as_ref().unwrap().pending.is_some());
        w.server.set_offline(false);
        w.fr.e.refresh();
        w.fr.e.tick(w.now, br(), true);
        w.fr.write(1, b"friend apart");
        w.at(600);
        w.fr.e.tick(w.now, br(), false);
        // Paulo is back: his session goes up as a branch
        w.at(10);
        w.tick(false, false);
        let id = w.me.e.cfg.world.as_ref().unwrap().world_id.clone();
        assert!(w.server.world(&id).diverged);
        assert_eq!(w.me.e.state(), SyncState::Diverged);
        assert_eq!(w.fr.e.state(), SyncState::Diverged);
        let v = w.me.e.snapshot().world.unwrap();
        assert_eq!(v.status, "Vocês jogaram separados");
        let c = v.choices.unwrap();
        assert_eq!(c.mine.t, "Seguir com a sua");
        assert!(c.mine.s.ends_with(" · neste PC"));
        assert_eq!(c.theirs.t, "Seguir com a do rodrigao_br");
        assert!(v.history.iter().filter(|r| r.tags.iter().any(|t| t.cls == "warn")).count() == 2);
        // nothing is pulled over either copy while it is undecided
        assert_eq!(w.me.read(0), b"paulo apart");
        assert_eq!(w.fr.read(1), b"friend apart");

        w.me.e.resolve(keep_mine).unwrap();
        let world = w.server.world(&id);
        assert!(!world.diverged);
        w.at(10);
        w.tick(false, false);
        let winner: &[u8] = if keep_mine { b"paulo apart" } else { b"friend apart" };
        assert_eq!(w.me.read(0), winner);
        assert_eq!(w.fr.read(1), winner, "keep_mine {keep_mine}");
        assert_eq!(w.me.e.state(), SyncState::Sync);
        assert_eq!(w.fr.e.state(), SyncState::Sync);
        // the losing copy stays in the server's history and in a backup
        assert!(w.server.world(&id).history.len() >= 3);
        assert!(fs::read_dir(if keep_mine { w.fr.data.join("backup") } else { w.me.data.join("backup") }).unwrap().count() >= 1);
    }
}

#[test]
fn the_up_state_while_the_final_upload_runs() {
    let mut w = World2::new();
    let seen: Arc<std::sync::Mutex<Vec<(SyncState, Option<u8>, String)>>> = Default::default();
    let s2 = seen.clone();
    w.me.e.set_publisher(Arc::new(move |s: &Snapshot| {
        if let Some(v) = &s.world {
            s2.lock().unwrap().push((v.state, v.progress.as_ref().map(|p| p.pct), v.status.clone()));
        }
    }));
    w.at(60);
    w.tick(true, false);
    w.me.write(0, b"x");
    w.at(600);
    w.tick(false, false);
    let seen = seen.lock().unwrap();
    assert!(seen.iter().any(|(s, p, t)| *s == SyncState::Up && *p == Some(100) && t == "Enviando sua sessão"), "{seen:?}");
}

#[test]
fn a_friends_lock_blocks_the_pull_and_a_late_lock_is_taken() {
    let mut w = World2::new();
    // the friend hosts; an autosave of his moves the head
    w.at(60);
    w.tick(false, true);
    w.fr.write(1, b"friend autosave");
    w.at(6);
    w.tick(false, true);
    w.at(6);
    w.tick(false, true);
    assert_eq!(w.me.e.state(), SyncState::Friend);
    assert_eq!(w.me.read(0), b"world v1", "no pull while he hosts");
    // Paulo opens the game to join: still the friend hosting
    w.at(5);
    w.tick(true, true);
    assert_eq!(w.me.e.state(), SyncState::Friend);
    assert!(!w.me.e.rt.lock_mine);
    let people = w.me.e.snapshot().people;
    assert_eq!((people[1].sub.as_str(), people[1].dot.as_str()), ("hospedando", "friend"));
}

#[test]
fn moving_the_world_between_stores_copies_it() {
    let dir = tempfile::tempdir().unwrap();
    let server = FakeServer::new();
    let a = dir.path().join("steam");
    let b = dir.path().join("local");
    fs::create_dir_all(&a).unwrap();
    fs::write(a.join("world0.ksd"), b"cloud world").unwrap();
    let stores = Stores { local: Some(Arc::new(LocalStore::new(b.clone()))), steam: Some(Arc::new(LocalStore::new(a.clone()))) };
    let mut e = Engine::new(Box::new(server.client()), dir.path().join("data"), stores, None);
    e.set_player(Player { steam_id: "76561198065863968".into(), nick: "p".into() });
    login(&server, &mut e, "76561198065863968");
    // a cloud world shared as a local one is copied into a free local slot first
    e.share(StoreKind::Steam, 0, StoreKind::Local, "zz").unwrap();
    assert_eq!(fs::read(b.join("world0.ksd")).unwrap(), b"cloud world");
    assert_eq!(e.cfg.world.as_ref().unwrap().store, StoreKind::Local);
    e.move_store(StoreKind::Steam).unwrap();
    let link = e.cfg.world.clone().unwrap();
    assert_eq!((link.store, link.slot), (StoreKind::Steam, 1));
    assert_eq!(fs::read(a.join("world1.ksd")).unwrap(), b"cloud world");
    assert_eq!(fs::read(a.join("world0.ksd")).unwrap(), b"cloud world", "the old file stays");
    assert!(!e.local_changed());
}
