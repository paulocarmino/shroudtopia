//! The server's routes (`app/CONTRACT.md`) as a trait, so the engine is tested against an in-memory server
//! (`testing::FakeServer`), and its HTTP implementation over `ureq`.

use std::io::{self, Read, Write};
use std::sync::Mutex;
use std::time::Duration;

use crate::contract::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// No answer: no network, DNS, timeout, a 5xx. The app is "sem conexão".
    Offline(String),
    /// An answer that refuses: the status and the contract's error body.
    Http { status: u16, code: String, message: String },
}

impl ApiError {
    pub fn is_offline(&self) -> bool {
        matches!(self, ApiError::Offline(_))
    }
    pub fn status(&self) -> Option<u16> {
        match self {
            ApiError::Http { status, .. } => Some(*status),
            ApiError::Offline(_) => None,
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Offline(m) => write!(f, "sem conexão com o servidor ({m})"),
            ApiError::Http { status, code, message } => write!(f, "{status} {code}: {message}"),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// `POST /api/worlds/:id/lock`: 200 takes it, 409 says who holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockOutcome {
    Taken(Lock),
    Busy(Lock),
}

/// Bytes so far, total.
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64);

/// `GET /api/auth/steam/poll/:sessionId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginPoll {
    Pending,
    Done(LoginDone),
    /// 410: expired, or already handed out
    Gone,
}

pub trait Api: Send {
    fn set_token(&self, token: Option<String>);
    fn login_start(&self, nick: &str) -> ApiResult<LoginStart>;
    fn login_poll(&self, session_id: &str) -> ApiResult<LoginPoll>;
    fn devices(&self) -> ApiResult<Vec<Device>>;
    /// `DELETE /api/devices/current`
    fn logout(&self) -> ApiResult<()>;
    fn patch_me(&self, nick: &str) -> ApiResult<Player>;
    fn put_avatar(&self, png: &[u8]) -> ApiResult<()>;
    fn list_worlds(&self) -> ApiResult<Vec<World>>;
    fn create_world(&self, name: &str) -> ApiResult<World>;
    fn get_world(&self, id: &str) -> ApiResult<World>;
    /// `DELETE /api/worlds/:id`: gone for every member
    fn delete_world(&self, id: &str) -> ApiResult<()>;
    fn create_invite(&self, id: &str) -> ApiResult<Invite>;
    fn redeem(&self, code: &str) -> ApiResult<World>;
    fn upload(&self, id: &str, bytes: &[u8], headers: &UploadHeaders, progress: Progress) -> ApiResult<UploadResponse>;
    /// Writes the file into `out`; returns its size.
    fn download(&self, id: &str, version: &str, out: &mut dyn Write, progress: Progress) -> ApiResult<u64>;
    fn lock(&self, id: &str) -> ApiResult<LockOutcome>;
    fn heartbeat(&self, id: &str) -> ApiResult<Lock>;
    fn release(&self, id: &str) -> ApiResult<()>;
    fn resolve(&self, id: &str, version: &str) -> ApiResult<World>;
    fn presence(&self) -> ApiResult<Vec<PresencePlayer>>;
}

pub struct HttpApi {
    base: String,
    token: Mutex<Option<String>>,
    agent: ureq::Agent,
}

impl HttpApi {
    pub fn new(base: &str, user_agent: &str) -> HttpApi {
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(8)).timeout_read(Duration::from_secs(60)).user_agent(user_agent).build();
        HttpApi { base: base.trim_end_matches('/').to_string(), token: Mutex::new(None), agent }
    }

    fn req(&self, method: &str, path: &str) -> ureq::Request {
        let r = self.agent.request(method, &format!("{}{path}", self.base));
        match self.token.lock().unwrap().as_deref() {
            Some(t) => r.set("Authorization", &format!("Bearer {t}")),
            None => r,
        }
    }

    fn json<T: serde::de::DeserializeOwned>(result: Result<ureq::Response, ureq::Error>) -> ApiResult<T> {
        let resp = Self::check(result)?;
        resp.into_json::<T>().map_err(|e| ApiError::Offline(format!("bad answer: {e}")))
    }

    fn check(result: Result<ureq::Response, ureq::Error>) -> ApiResult<ureq::Response> {
        match result {
            Ok(resp) => Ok(resp),
            Err(ureq::Error::Status(status, resp)) if status < 500 => {
                let body: Option<ErrorBody> = resp.into_json().ok();
                Err(ApiError::Http { status, code: body.as_ref().map(|b| b.error.clone()).unwrap_or_default(), message: body.map(|b| b.message).unwrap_or_default() })
            }
            Err(ureq::Error::Status(status, _)) => Err(ApiError::Offline(format!("status {status}"))),
            Err(e) => Err(ApiError::Offline(e.to_string())),
        }
    }
}

fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// A body that tells how much of it was read.
struct Counting<'a, 'b> {
    bytes: &'a [u8],
    at: usize,
    progress: Progress<'b>,
}

impl Read for Counting<'_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = (&self.bytes[self.at..]).read(buf)?;
        self.at += n;
        (self.progress)(self.at as u64, self.bytes.len() as u64);
        Ok(n)
    }
}

impl Api for HttpApi {
    fn set_token(&self, token: Option<String>) {
        *self.token.lock().unwrap() = token;
    }

    fn login_start(&self, nick: &str) -> ApiResult<LoginStart> {
        Self::json(self.req("POST", "/api/auth/steam/start").send_json(NickBody { nick: nick.into() }))
    }

    fn login_poll(&self, session_id: &str) -> ApiResult<LoginPoll> {
        match self.req("GET", &format!("/api/auth/steam/poll/{}", enc(session_id))).call() {
            Ok(resp) if resp.status() == 202 => Ok(LoginPoll::Pending),
            Err(ureq::Error::Status(410, _)) => Ok(LoginPoll::Gone),
            other => Self::json::<LoginDone>(other).map(LoginPoll::Done),
        }
    }

    fn devices(&self) -> ApiResult<Vec<Device>> {
        Self::json::<DeviceList>(self.req("GET", "/api/devices").call()).map(|d| d.devices)
    }

    fn logout(&self) -> ApiResult<()> {
        Self::check(self.req("DELETE", "/api/devices/current").call()).map(|_| ())
    }

    fn patch_me(&self, nick: &str) -> ApiResult<Player> {
        Self::json::<Me>(self.req("PATCH", "/api/me").send_json(NickBody { nick: nick.into() })).map(|m| m.player)
    }

    fn put_avatar(&self, png: &[u8]) -> ApiResult<()> {
        Self::check(self.req("PUT", "/api/me/avatar").set("Content-Type", "image/png").send_bytes(png)).map(|_| ())
    }

    fn list_worlds(&self) -> ApiResult<Vec<World>> {
        Self::json::<WorldList>(self.req("GET", "/api/worlds").call()).map(|l| l.worlds)
    }

    fn create_world(&self, name: &str) -> ApiResult<World> {
        Self::json(self.req("POST", "/api/worlds").send_json(CreateWorld { name: name.into() }))
    }

    fn get_world(&self, id: &str) -> ApiResult<World> {
        Self::json(self.req("GET", &format!("/api/worlds/{}", enc(id))).call())
    }

    fn create_invite(&self, id: &str) -> ApiResult<Invite> {
        Self::json(self.req("POST", &format!("/api/worlds/{}/invites", enc(id))).send_bytes(&[]))
    }

    fn redeem(&self, code: &str) -> ApiResult<World> {
        Self::json(self.req("POST", "/api/invites/redeem").send_json(RedeemInvite { code: code.into() }))
    }

    fn upload(&self, id: &str, bytes: &[u8], h: &UploadHeaders, progress: Progress) -> ApiResult<UploadResponse> {
        let mut r = self
            .req("POST", &format!("/api/worlds/{}/versions", enc(id)))
            .set("Content-Type", "application/octet-stream")
            .set("Content-Length", &bytes.len().to_string())
            .set("X-Sha256", &h.sha256)
            .set("X-Started-At", &h.started_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        if let Some(p) = &h.parent {
            r = r.set("X-Parent", p);
        }
        if let Some(e) = &h.ended_at {
            r = r.set("X-Ended-At", &e.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
        Self::json(r.send(Counting { bytes, at: 0, progress }))
    }

    fn download(&self, id: &str, version: &str, out: &mut dyn Write, progress: Progress) -> ApiResult<u64> {
        let resp = Self::check(self.req("GET", &format!("/api/worlds/{}/versions/{}/file", enc(id), enc(version))).call())?;
        let total: u64 = resp.header("Content-Length").and_then(|l| l.parse().ok()).unwrap_or(0);
        let mut reader = resp.into_reader().take(1 << 30);
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| ApiError::Offline(e.to_string()))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|e| ApiError::Offline(format!("write: {e}")))?;
            done += n as u64;
            progress(done, total.max(done));
        }
        Ok(done)
    }

    fn lock(&self, id: &str) -> ApiResult<LockOutcome> {
        match self.req("POST", &format!("/api/worlds/{}/lock", enc(id))).send_bytes(&[]) {
            Err(ureq::Error::Status(409, resp)) => resp.into_json::<LockResponse>().map(|l| LockOutcome::Busy(l.lock)).map_err(|e| ApiError::Offline(format!("bad answer: {e}"))),
            other => Self::json::<LockResponse>(other).map(|l| LockOutcome::Taken(l.lock)),
        }
    }

    fn heartbeat(&self, id: &str) -> ApiResult<Lock> {
        Self::json::<LockResponse>(self.req("POST", &format!("/api/worlds/{}/lock/heartbeat", enc(id))).send_bytes(&[])).map(|l| l.lock)
    }

    fn release(&self, id: &str) -> ApiResult<()> {
        Self::check(self.req("DELETE", &format!("/api/worlds/{}/lock", enc(id))).call()).map(|_| ())
    }

    fn delete_world(&self, id: &str) -> ApiResult<()> {
        Self::check(self.req("DELETE", &format!("/api/worlds/{}", enc(id))).call()).map(|_| ())
    }

    fn resolve(&self, id: &str, version: &str) -> ApiResult<World> {
        Self::json(self.req("POST", &format!("/api/worlds/{}/resolve", enc(id))).send_json(Resolve { version_id: version.into() }))
    }

    fn presence(&self) -> ApiResult<Vec<PresencePlayer>> {
        Self::json::<Presence>(self.req("GET", "/api/presence").call()).map(|p| p.players)
    }
}
