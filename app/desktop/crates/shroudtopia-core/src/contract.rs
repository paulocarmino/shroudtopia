//! The wire shapes of `app/CONTRACT.md`, one Rust type per JSON shape. `contract.example.json` holds one real
//! request/response per route; the test at the bottom deserializes every one of them, so a change to the contract
//! breaks this crate until both sides agree.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A Steam account: steamid64 (as a string) and the Steam `PersonaName`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub steam_id: String,
    pub nick: String,
}

/// `POST /api/auth/steam/start` body (and `PATCH /api/me`): the PersonaName of `loginusers.vdf`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NickBody {
    pub nick: String,
}

/// `POST /api/auth/steam/start` answer: the page the browser opens, and the session the app polls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStart {
    pub session_id: String,
    pub login_url: String,
    pub expires_at: DateTime<Utc>,
}

/// `GET /api/auth/steam/poll/:sessionId`, 202.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginPending {
    pub status: String,
}

/// `GET /api/auth/steam/poll/:sessionId`, 200 (exactly once).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginDone {
    pub token: String,
    pub player: Player,
}

/// One device of `GET /api/devices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub device_id: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceList {
    pub devices: Vec<Device>,
}

/// `PATCH /api/me` answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Me {
    pub player: Player,
}

/// One upload of the world file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub version_id: String,
    /// The version the uploader's local copy was pulled from; `None` for the first.
    pub parent: Option<String>,
    /// steamId of the uploader.
    pub uploaded_by: String,
    pub size: u64,
    pub sha256: String,
    pub started_at: DateTime<Utc>,
    /// `None` for an autosave during a running session.
    pub ended_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// The host lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lock {
    /// steamId of the holder.
    pub holder: String,
    pub nick: String,
    pub since: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// A shared world. `GET /api/worlds` may leave out `head`, `lock`, `history` (they default to empty).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct World {
    pub world_id: String,
    pub name: String,
    #[serde(default)]
    pub members: Vec<Player>,
    #[serde(default)]
    pub head: Option<Version>,
    #[serde(default)]
    pub lock: Option<Lock>,
    #[serde(default)]
    pub diverged: bool,
    /// The last 20 versions, newest first.
    #[serde(default)]
    pub history: Vec<Version>,
}

/// `GET /api/worlds` answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldList {
    pub worlds: Vec<World>,
}

/// `POST /api/worlds` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWorld {
    pub name: String,
}

/// `POST /api/worlds/:id/invites` answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invite {
    pub code: String,
    pub expires_at: DateTime<Utc>,
}

/// `POST /api/invites/redeem` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedeemInvite {
    pub code: String,
}

/// The headers of `POST /api/worlds/:id/versions`. `X-Parent` is left out for the first version of a world (its
/// parent is null) and `X-Ended-At` for an autosave during a running session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadHeaders {
    #[serde(rename = "X-Parent", default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(rename = "X-Sha256")]
    pub sha256: String,
    #[serde(rename = "X-Started-At")]
    pub started_at: DateTime<Utc>,
    #[serde(rename = "X-Ended-At", default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
}

/// `POST /api/worlds/:id/versions` answer. `head` is a versionId here (in `World` it is the whole version).
/// `version` is the stored upload; the contract's example of a diverged upload leaves it out, so it may be absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadResponse {
    #[serde(default)]
    pub version: Option<Version>,
    pub head: Option<String>,
    pub diverged: bool,
}

/// `POST /api/worlds/:id/lock` (200 or 409) and `.../lock/heartbeat` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockResponse {
    pub lock: Lock,
}

/// `POST /api/worlds/:id/resolve` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolve {
    pub version_id: String,
}

/// One player of `GET /api/presence`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresencePlayer {
    pub steam_id: String,
    pub nick: String,
    pub app_online: bool,
    pub in_game: bool,
}

/// `GET /api/presence` answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    pub players: Vec<PresencePlayer>,
}

/// Every error: `{ error: "<code>", message }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
    #[serde(default)]
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const EXAMPLE: &str = include_str!("../../../../contract.example.json");

    fn example() -> Value {
        serde_json::from_str(EXAMPLE).expect("contract.example.json is JSON")
    }

    fn shape<T: serde::de::DeserializeOwned + Serialize>(value: &Value) -> T {
        let parsed: T = serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{e}: {value}"));
        // and what it writes back reads the same (no field is lost on the way)
        let again: T = serde_json::from_value(serde_json::to_value(&parsed).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(&again).unwrap(), serde_json::to_value(&parsed).unwrap());
        parsed
    }

    /// Every shape of the example, by its top-level key. A key this test does not know fails it: a route added to the
    /// contract needs its type here before the app builds.
    #[test]
    fn every_shape_of_the_example_deserializes() {
        let ex = example();
        let keys: Vec<&String> = ex.as_object().unwrap().keys().collect();
        let known = ["steamLoginStart", "steamLoginPending", "steamLoginDone", "devices", "createInvite", "redeemInvite", "world", "lockTaken", "lockBusy", "uploadDiverged", "presence"];
        for key in &keys {
            assert!(known.contains(&key.as_str()), "contract.example.json has `{key}`, which this crate does not know yet");
        }
        for key in known {
            assert!(ex.get(key).is_some(), "contract.example.json lost `{key}`");
        }

        let start_req: NickBody = shape(&ex["steamLoginStart"]["request"]);
        let start: LoginStart = shape(&ex["steamLoginStart"]["response"]);
        assert!(start.login_url.ends_with(&start.session_id));
        assert_eq!(ex["steamLoginPending"]["status"], 202);
        let pending: LoginPending = shape(&ex["steamLoginPending"]["response"]);
        assert_eq!(pending.status, "pending");
        let dev: LoginDone = shape(&ex["steamLoginDone"]["response"]);
        assert_eq!(dev.player.nick, start_req.nick);
        assert!(dev.token.starts_with("dev_"));
        let req = dev.player.clone();
        let devices: DeviceList = shape(&ex["devices"]["response"]);
        assert!(devices.devices[0].current);

        let inv: Invite = shape(&ex["createInvite"]["response"]);
        assert_eq!(inv.code, "ENSH-7KQ2");
        let red: RedeemInvite = shape(&ex["redeemInvite"]["request"]);
        assert_eq!(red.code, inv.code);

        let world: World = shape(&ex["world"]);
        assert_eq!(world.name, "Mundo do bang");
        assert_eq!(world.members.len(), 2);
        let head = world.head.as_ref().unwrap();
        assert_eq!(head.size, 1_797_049);
        assert_eq!(head.parent.as_deref(), Some("01JB7ZZZZZZZZZZZZZZZZZZZZZ"));
        assert!(head.ended_at.is_some());
        assert!(world.lock.is_none());
        assert!(!world.diverged);
        assert_eq!(world.history.first(), world.head.as_ref());

        let taken: LockResponse = shape(&ex["lockTaken"]["response"]);
        assert_eq!(taken.lock.holder, req.steam_id);
        assert_eq!(ex["lockBusy"]["status"], 409);
        let busy: LockResponse = shape(&ex["lockBusy"]["response"]);
        assert_eq!(busy.lock.nick, "rodrigao_br");

        let headers: UploadHeaders = shape(&ex["uploadDiverged"]["requestHeaders"]);
        assert_eq!(headers.parent.as_deref(), Some("01JB7ZZZZZZZZZZZZZZZZZZZZZ"));
        assert!(headers.ended_at.is_some());
        let up: UploadResponse = shape(&ex["uploadDiverged"]["response"]);
        assert!(up.diverged);
        assert_eq!(up.head.as_deref(), Some(head.version_id.as_str()));
        let stored = up.version.as_ref().expect("the stored version");
        assert_eq!(stored.sha256, headers.sha256);
        assert_eq!(stored.sha256.len(), 64, "X-Sha256 is 64 hex of the body");
        assert_eq!(stored.parent, headers.parent);

        let presence: Presence = shape(&ex["presence"]["response"]);
        assert_eq!(presence.players.len(), 2);
        assert!(presence.players[1].in_game);
    }

    /// A world as `GET /api/worlds` may list it, without head, lock and history, and the error body.
    #[test]
    fn the_short_shapes() {
        let world: World = serde_json::from_str(r#"{"worldId":"w","name":"n","members":[]}"#).unwrap();
        assert!(world.head.is_none() && world.history.is_empty() && !world.diverged);
        let list: WorldList = serde_json::from_str(r#"{"worlds":[{"worldId":"w","name":"n"}]}"#).unwrap();
        assert_eq!(list.worlds.len(), 1);
        let err: ErrorBody = serde_json::from_str(r#"{"error":"not_found","message":"no such world"}"#).unwrap();
        assert_eq!(err.error, "not_found");
        // the first upload of a world has no X-Parent
        let first = UploadHeaders { parent: None, sha256: "aa".into(), started_at: Utc::now(), ended_at: None };
        let json = serde_json::to_value(&first).unwrap();
        assert!(json.get("X-Parent").is_none() && json.get("X-Ended-At").is_none());
    }
}
