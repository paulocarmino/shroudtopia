//! An in-memory server that follows `app/CONTRACT.md` (head/branch acceptance, lease with expiry, invites, resolve),
//! for the engine's tests. Several clients (`FakeServer::client`) share one server, so a test plays both friends.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};

use crate::api::*;
use crate::contract::*;

#[derive(Default)]
struct WorldRec {
    world: World,
    versions: Vec<Version>,
    files: BTreeMap<String, Vec<u8>>,
}

#[derive(Default)]
struct State {
    now: Option<DateTime<Utc>>,
    offline: bool,
    seq: u64,
    devices: BTreeMap<String, Player>,
    worlds: BTreeMap<String, WorldRec>,
    invites: BTreeMap<String, String>,
    online: BTreeMap<String, bool>,
    in_game: BTreeMap<String, bool>,
    logins: BTreeMap<String, Login>,
    pub uploads: u64,
}

#[derive(Clone)]
struct Login {
    nick: String,
    steam_id: Option<String>,
    done: bool,
}

#[derive(Clone, Default)]
pub struct FakeServer {
    state: Arc<Mutex<State>>,
}

pub fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn http(status: u16, code: &str) -> ApiError {
    ApiError::Http { status, code: code.into(), message: code.into() }
}

impl FakeServer {
    pub fn new() -> FakeServer {
        FakeServer::default()
    }

    pub fn client(&self) -> FakeClient {
        FakeClient { server: self.clone(), token: Arc::new(Mutex::new(None)) }
    }

    pub fn set_now(&self, now: DateTime<Utc>) {
        self.state.lock().unwrap().now = Some(now);
    }

    pub fn set_offline(&self, offline: bool) {
        self.state.lock().unwrap().offline = offline;
    }

    pub fn set_in_game(&self, steam_id: &str, in_game: bool) {
        self.state.lock().unwrap().in_game.insert(steam_id.into(), in_game);
    }

    /// The browser finished the Steam login of `session_id` as `steam_id`.
    pub fn complete_login(&self, session_id: &str, steam_id: &str) {
        if let Some(l) = self.state.lock().unwrap().logins.get_mut(session_id) {
            l.steam_id = Some(steam_id.into());
        }
    }

    /// The session of the latest login started.
    pub fn last_login(&self) -> Option<String> {
        self.state.lock().unwrap().logins.keys().last().cloned()
    }

    pub fn uploads(&self) -> u64 {
        self.state.lock().unwrap().uploads
    }

    /// The world as `GET /api/worlds/:id` answers it.
    pub fn world(&self, id: &str) -> World {
        let s = self.state.lock().unwrap();
        Self::view(&s, id)
    }

    pub fn file(&self, id: &str, version: &str) -> Vec<u8> {
        self.state.lock().unwrap().worlds[id].files[version].clone()
    }

    fn now(s: &State) -> DateTime<Utc> {
        s.now.unwrap_or_else(Utc::now)
    }

    fn view(s: &State, id: &str) -> World {
        let rec = &s.worlds[id];
        let mut w = rec.world.clone();
        if w.lock.as_ref().is_some_and(|l| l.expires_at <= Self::now(s)) {
            w.lock = None;
        }
        w.history = rec.versions.iter().rev().take(20).cloned().collect();
        w
    }
}

pub struct FakeClient {
    server: FakeServer,
    token: Arc<Mutex<Option<String>>>,
}

impl FakeClient {
    fn with<T>(&self, f: impl FnOnce(&mut State, Option<Player>) -> ApiResult<T>) -> ApiResult<T> {
        let mut s = self.server.state.lock().unwrap();
        if s.offline {
            return Err(ApiError::Offline("connection refused".into()));
        }
        let me = self.token.lock().unwrap().as_ref().and_then(|t| s.devices.get(t).cloned());
        if let Some(p) = &me {
            s.online.insert(p.steam_id.clone(), true);
        }
        f(&mut s, me)
    }

    fn member(s: &State, id: &str, me: &Option<Player>) -> ApiResult<Player> {
        let me = me.clone().ok_or_else(|| http(401, "unauthorized"))?;
        let rec = s.worlds.get(id).ok_or_else(|| http(404, "not_found"))?;
        if !rec.world.members.iter().any(|m| m.steam_id == me.steam_id) {
            return Err(http(403, "forbidden"));
        }
        Ok(me)
    }
}

impl Api for FakeClient {
    fn set_token(&self, token: Option<String>) {
        *self.token.lock().unwrap() = token;
    }

    fn login_start(&self, nick: &str) -> ApiResult<LoginStart> {
        self.with(|s, _| {
            s.seq += 1;
            let id = format!("s{:04}", s.seq);
            s.logins.insert(id.clone(), Login { nick: nick.into(), steam_id: None, done: false });
            Ok(LoginStart { session_id: id.clone(), login_url: format!("https://fake/auth/steam/redirect/{id}"), expires_at: FakeServer::now(s) + Duration::minutes(10) })
        })
    }

    fn login_poll(&self, session_id: &str) -> ApiResult<LoginPoll> {
        self.with(|s, _| {
            let Some(l) = s.logins.get(session_id).cloned() else { return Ok(LoginPoll::Gone) };
            match l.steam_id {
                _ if l.done => Ok(LoginPoll::Gone),
                None => Ok(LoginPoll::Pending),
                Some(id) => {
                    s.logins.get_mut(session_id).unwrap().done = true;
                    s.seq += 1;
                    let token = format!("dev_{}", s.seq);
                    let player = Player { steam_id: id, nick: l.nick };
                    s.devices.insert(token.clone(), player.clone());
                    Ok(LoginPoll::Done(LoginDone { token, player }))
                }
            }
        })
    }

    fn devices(&self) -> ApiResult<Vec<Device>> {
        self.with(|_, me| me.map(|_| vec![]).ok_or_else(|| http(401, "unauthorized")))
    }

    fn logout(&self) -> ApiResult<()> {
        let token = self.token.lock().unwrap().clone();
        self.with(|s, _| {
            if let Some(t) = token {
                s.devices.remove(&t);
            }
            Ok(())
        })
    }

    fn patch_me(&self, nick: &str) -> ApiResult<Player> {
        let token = self.token.lock().unwrap().clone().unwrap_or_default();
        self.with(|s, me| {
            let mut me = me.ok_or_else(|| http(401, "unauthorized"))?;
            me.nick = nick.into();
            s.devices.insert(token, me.clone());
            for rec in s.worlds.values_mut() {
                for m in rec.world.members.iter_mut().filter(|m| m.steam_id == me.steam_id) {
                    m.nick = nick.into();
                }
            }
            Ok(me)
        })
    }

    fn put_avatar(&self, _png: &[u8]) -> ApiResult<()> {
        self.with(|_, me| me.map(|_| ()).ok_or_else(|| http(401, "unauthorized")))
    }

    fn list_worlds(&self) -> ApiResult<Vec<World>> {
        self.with(|s, me| {
            let me = me.ok_or_else(|| http(401, "unauthorized"))?;
            Ok(s.worlds.values().filter(|r| r.world.members.iter().any(|m| m.steam_id == me.steam_id)).map(|r| r.world.clone()).collect())
        })
    }

    fn create_world(&self, name: &str) -> ApiResult<World> {
        self.with(|s, me| {
            let me = me.ok_or_else(|| http(401, "unauthorized"))?;
            s.seq += 1;
            let id = format!("w{:04}", s.seq);
            let world = World { world_id: id.clone(), name: name.into(), members: vec![me], head: None, lock: None, diverged: false, history: vec![] };
            s.worlds.insert(id.clone(), WorldRec { world, ..Default::default() });
            Ok(FakeServer::view(s, &id))
        })
    }

    fn get_world(&self, id: &str) -> ApiResult<World> {
        self.with(|s, me| {
            Self::member(s, id, &me)?;
            Ok(FakeServer::view(s, id))
        })
    }

    fn create_invite(&self, id: &str) -> ApiResult<Invite> {
        self.with(|s, me| {
            Self::member(s, id, &me)?;
            s.seq += 1;
            let code = format!("ENSH-{:04}", s.seq);
            s.invites.insert(code.clone(), id.into());
            Ok(Invite { code, expires_at: FakeServer::now(s) + Duration::hours(24) })
        })
    }

    fn redeem(&self, code: &str) -> ApiResult<World> {
        self.with(|s, me| {
            let me = me.ok_or_else(|| http(401, "unauthorized"))?;
            let id = s.invites.remove(code).ok_or_else(|| http(404, "invalid_code"))?;
            let rec = s.worlds.get_mut(&id).unwrap();
            if !rec.world.members.iter().any(|m| m.steam_id == me.steam_id) {
                rec.world.members.push(me);
            }
            Ok(FakeServer::view(s, &id))
        })
    }

    fn upload(&self, id: &str, bytes: &[u8], h: &UploadHeaders, progress: Progress) -> ApiResult<UploadResponse> {
        self.with(|s, me| {
            let me = Self::member(s, id, &me)?;
            if sha_hex(bytes) != h.sha256 {
                return Err(http(400, "sha256_mismatch"));
            }
            progress(bytes.len() as u64, bytes.len() as u64);
            s.seq += 1;
            s.uploads += 1;
            let now = FakeServer::now(s);
            let v = Version {
                version_id: format!("v{:04}", s.seq),
                parent: h.parent.clone(),
                uploaded_by: me.steam_id.clone(),
                size: bytes.len() as u64,
                sha256: h.sha256.clone(),
                started_at: h.started_at,
                ended_at: h.ended_at,
                created_at: now,
            };
            let rec = s.worlds.get_mut(id).unwrap();
            let head = rec.world.head.as_ref().map(|v| v.version_id.clone());
            rec.files.insert(v.version_id.clone(), bytes.to_vec());
            rec.versions.push(v.clone());
            if h.parent == head {
                rec.world.head = Some(v.clone());
            } else {
                rec.world.diverged = true;
            }
            Ok(UploadResponse { version: Some(v), head: rec.world.head.as_ref().map(|v| v.version_id.clone()), diverged: rec.world.diverged })
        })
    }

    fn download(&self, id: &str, version: &str, out: &mut dyn Write, progress: Progress) -> ApiResult<u64> {
        let bytes = self.with(|s, me| {
            Self::member(s, id, &me)?;
            s.worlds[id].files.get(version).cloned().ok_or_else(|| http(404, "not_found"))
        })?;
        out.write_all(&bytes).unwrap();
        progress(bytes.len() as u64, bytes.len() as u64);
        Ok(bytes.len() as u64)
    }

    fn lock(&self, id: &str) -> ApiResult<LockOutcome> {
        self.with(|s, me| {
            let me = Self::member(s, id, &me)?;
            let now = FakeServer::now(s);
            let rec = s.worlds.get_mut(id).unwrap();
            match &rec.world.lock {
                Some(l) if l.expires_at > now && l.holder != me.steam_id => Ok(LockOutcome::Busy(l.clone())),
                _ => {
                    let lock = Lock { holder: me.steam_id.clone(), nick: me.nick.clone(), since: now, expires_at: now + Duration::minutes(5) };
                    rec.world.lock = Some(lock.clone());
                    Ok(LockOutcome::Taken(lock))
                }
            }
        })
    }

    fn heartbeat(&self, id: &str) -> ApiResult<Lock> {
        self.with(|s, me| {
            let me = Self::member(s, id, &me)?;
            let now = FakeServer::now(s);
            let rec = s.worlds.get_mut(id).unwrap();
            match &mut rec.world.lock {
                Some(l) if l.holder == me.steam_id && l.expires_at > now => {
                    l.expires_at = now + Duration::minutes(5);
                    Ok(l.clone())
                }
                Some(l) if l.expires_at > now => Err(http(409, "lock_held")),
                // free or expired: the heartbeat takes it (CONTRACT.md, details the server settles)
                _ => {
                    let lock = Lock { holder: me.steam_id.clone(), nick: me.nick.clone(), since: now, expires_at: now + Duration::minutes(5) };
                    rec.world.lock = Some(lock.clone());
                    Ok(lock)
                }
            }
        })
    }

    fn release(&self, id: &str) -> ApiResult<()> {
        self.with(|s, me| {
            let me = Self::member(s, id, &me)?;
            let rec = s.worlds.get_mut(id).unwrap();
            if rec.world.lock.as_ref().is_some_and(|l| l.holder == me.steam_id) {
                rec.world.lock = None;
            }
            Ok(())
        })
    }

    fn resolve(&self, id: &str, version: &str) -> ApiResult<World> {
        self.with(|s, me| {
            Self::member(s, id, &me)?;
            let rec = s.worlds.get_mut(id).unwrap();
            let v = rec.versions.iter().find(|v| v.version_id == version).cloned().ok_or_else(|| http(404, "not_found"))?;
            rec.world.head = Some(v);
            rec.world.diverged = false;
            Ok(FakeServer::view(s, id))
        })
    }

    fn presence(&self) -> ApiResult<Vec<PresencePlayer>> {
        self.with(|s, me| {
            let me = me.ok_or_else(|| http(401, "unauthorized"))?;
            let mut out: Vec<PresencePlayer> = Vec::new();
            for rec in s.worlds.values().filter(|r| r.world.members.iter().any(|m| m.steam_id == me.steam_id)) {
                for m in &rec.world.members {
                    if !out.iter().any(|p| p.steam_id == m.steam_id) {
                        out.push(PresencePlayer {
                            steam_id: m.steam_id.clone(),
                            nick: m.nick.clone(),
                            app_online: s.online.get(&m.steam_id).copied().unwrap_or(false),
                            in_game: s.in_game.get(&m.steam_id).copied().unwrap_or(false),
                        });
                    }
                }
            }
            Ok(out)
        })
    }
}
