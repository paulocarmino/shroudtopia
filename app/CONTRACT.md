# Sync contract — desktop app ↔ sync server

Both sides build and test against this file and `contract.example.json`. The example holds one real
request/response per route; each side's tests read it, so a change here breaks both until both agree.

## Hosts

- The API (every route below, the Steam login's redirect and callback): `https://shroudtopia-api.paulocarmino.com`.
  That is `<server>` below, and the origin the Steam login's `return_to` and `realm` are built from.
- `https://shroudtopia.paulocarmino.com` is only the landing page (`app/landing`): it answers no `/api` or
  `/auth` route, and the API host does not serve the landing page.

## Identity

- A **player** is a Steam account: `steamId` (steamid64, string) and `nick` (Steam `PersonaName`).
  The app reads both from `Steam/config/loginusers.vdf`, the entry with `"MostRecent" "1"`.
  Optional avatar: `Steam/config/avatarcache/<steamId>.png`, uploaded once as PNG.
- A **device token** authenticates the app (`Authorization: Bearer <token>`). It is issued only
  after a **Steam login** (Steam OpenID 2.0), which proves the steamId; the server never trusts a
  steamId the app sends. The token is stored in the app's config under `%APPDATA%\Shroudtopia`.

### Steam login (replaces `POST /api/devices`)

1. App: `POST /api/auth/steam/start { nick }` → `{ sessionId, loginUrl, expiresAt }` (10 min).
   `nick` is the PersonaName read from `loginusers.vdf` (Steam OpenID does not return it).
2. App opens `loginUrl` in the default browser. It points to `/auth/steam/redirect/:sessionId`,
   which redirects to `https://steamcommunity.com/openid/login` with
   `openid.return_to=https://<server>/auth/steam/callback/:sessionId` and `openid.realm=https://<server>/`.
3. Steam sends the browser back to the callback. The server verifies the assertion with Steam
   (`openid.mode=check_authentication`, must answer `is_valid:true`), checks `return_to` matches,
   takes the steamid64 from `openid.claimed_id` (`https://steamcommunity.com/openid/id/<steamid64>`),
   and shows a plain page "Pronto, pode voltar para o app".
4. App polls `GET /api/auth/steam/poll/:sessionId` every 2 s → `202 { status: "pending" }`, then
   `200 { token, player }` exactly once (a second poll is `410`), or `410 { error: "expired" }`.
5. The app compares `player.steamId` with the MostRecent account in `loginusers.vdf`; if they differ
   it says so (logged into another Steam account in the browser) and offers to log in again.

- `GET /api/devices` → `{ devices: [{ deviceId, createdAt, lastSeenAt, current }] }`;
  `DELETE /api/devices/:deviceId` revokes one; `DELETE /api/devices/current` is logout.
- `PATCH /api/me { nick }` updates the nick when the app sees a new PersonaName.

## Worlds and versions

- A **world** is one shared Enshrouded world: `worldId` (server-made, ULID), `name`, `members`.
- A **version** is one upload of the world file: `versionId` (ULID), `parent` (the versionId the
  uploader's local copy was pulled from, `null` for the first), `uploadedBy` (steamId), `size`,
  `sha256`, `startedAt`/`endedAt` of the session it closes (or `endedAt: null` for an autosave
  during a running session), `createdAt`.
- The server keeps every version (history). `head` is the latest accepted version.
- **Accepting an upload**: if `parent == head`, the upload becomes the new head. If not, the
  server stores it as a **branch** (`diverged: true` on the world) and does not move head. The app
  shows "Vocês jogaram separados" when `diverged` is true or when its own local base ≠ head and the
  local file changed since that base. `POST /api/worlds/:id/resolve { versionId }` makes the chosen
  version head and clears `diverged`; the other stays in history.
- Uploads during a session (autosaves) of the same session chain: each one's `parent` is the
  previous upload of that session.

## Host lock (lease)

- `POST /api/worlds/:id/lock` takes the lease for the caller, or answers `409` with the holder.
  The lease lasts 5 minutes and is renewed by `POST /api/worlds/:id/lock/heartbeat` every 60 s
  while `enshrouded.exe` runs. `DELETE /api/worlds/:id/lock` releases it after the final upload.
  An expired lease is free; that covers a PC that died mid-session.
- The lock does not stop the game: the app takes it when the game starts. If someone else holds
  it, the app shows them as host and does not upload this PC's changes as head (they become a
  branch if uploaded).

## Pairing

- `POST /api/worlds { name }` creates a world with the caller as member.
- `POST /api/worlds/:id/invites` → `{ code: "ENSH-7KQ2", expiresAt }`, valid 24 h, one use.
- `POST /api/invites/redeem { code }` adds the caller to the world and returns it.

## Routes

| Method | Path | Body | Answer |
|---|---|---|---|
| POST | `/api/auth/steam/start` | `{ nick }` | `{ sessionId, loginUrl, expiresAt }` |
| GET | `/api/auth/steam/poll/:sessionId` | | `202 { status }` / `200 { token, player }` / `410` |
| GET | `/api/devices` | | `{ devices }` |
| DELETE | `/api/devices/:deviceId` or `/api/devices/current` | | `204` |
| PATCH | `/api/me` | `{ nick }` | `{ player }` |
| PUT | `/api/me/avatar` | PNG bytes | `204` |
| GET | `/api/worlds` | | `{ worlds: World[] }` |
| POST | `/api/worlds` | `{ name }` | `World` |
| GET | `/api/worlds/:id` | | `World` with `head`, `lock`, `diverged`, `members`, `history` (last 20 versions) |
| DELETE | `/api/worlds/:id` | | `204`; any member deletes it for everyone (versions, files, invites); `409 { lock }` while someone else hosts it |
| POST | `/api/worlds/:id/invites` | | `{ code, expiresAt }` |
| POST | `/api/invites/redeem` | `{ code }` | `World` |
| POST | `/api/worlds/:id/versions` | raw file bytes; headers `X-Parent`, `X-Sha256`, `X-Started-At`, `X-Ended-At` | `{ version, head, diverged }` |
| GET | `/api/worlds/:id/versions/:vid/file` | | file bytes |
| POST | `/api/worlds/:id/lock` | | `{ lock }` or `409 { lock }` |
| POST | `/api/worlds/:id/lock/heartbeat` | | `{ lock }` |
| DELETE | `/api/worlds/:id/lock` | | `204` |
| POST | `/api/worlds/:id/resolve` | `{ versionId }` | `World` |
| GET | `/api/presence` | | `{ players: [{ steamId, nick, appOnline, inGame }] }` (members of the caller's worlds; `appOnline` = a request in the last 2 min) |
| GET | `/api/players/:steamId/avatar` | | PNG bytes, or `404` (self or someone who shares a world with the caller) |

Errors are `{ error: "<code>", message }` with a fitting status.

### Details the server settles (as built in `app/server`)

- Success is `200` with JSON unless the table says `204`. Every `/api` route but `POST /api/auth/steam/start`
  and the poll needs the bearer token (`401 unauthorized` otherwise; a revoked device's token is `401` too).
- A world the caller is not a member of answers `404 world_not_found`, as if it did not exist.
- **Upload headers**: `X-Sha256` is required and checked against the body (`400 sha256_mismatch`). `X-Parent`
  absent, empty or `null` means no parent. A parent that is not a version of this world is `400 unknown_parent`.
  `X-Started-At` / `X-Ended-At` are ISO 8601; absent means `null` (an autosave has no `endedAt`). The answer's
  `head` is the head's **versionId** (a string, or `null`), `version` is the stored Version.
- **diverged** is set whenever an upload is stored as a branch and is cleared only by `resolve`: an upload on head
  after that moves head but leaves `diverged: true`. `resolve` with a versionId not of this world is
  `400 unknown_version`.
- The file download carries `X-Sha256` and `X-Version-Id` headers.
- **Lock**: `POST /lock` by the holder renews it (same `since`). The heartbeat renews when the caller holds it,
  takes it when it is free or expired, and is `409 { lock }` when someone else holds it. `DELETE /lock` is `204`
  when the caller held it or nobody did, `409 { lock }` when someone else does. A `409` body also carries
  `error: "lock_held"` and `message`. `inGame` in presence = holds an unexpired lease on any world.
- **Invites**: the 4 characters come from `23456789ABCDEFGHJKMNPQRSTUVWXYZ`. Redeem accepts any case and the code
  with or without `ENSH-`. Unknown `404 invite_not_found`, spent `410 invite_used`, expired `410 invite_expired`.
  A caller who is already a member gets the world back and the code is not spent. Ten wrong codes in 10 minutes
  from one device are `429 too_many_attempts`.
- `nick` up to 64 characters, `name` up to 80. Each completed Steam login is a new device with its own token and
  sets the player's nick to the one sent at the start. World files up to 64 MB, avatars up to 512 KB (PNG
  signature checked).
- **Steam login**: the callback refuses, with a pt-BR page and no token, an assertion whose `op_endpoint` is not
  Steam's, whose `return_to` is not exactly this session's callback, whose `claimed_id` is not
  `https://steamcommunity.com/openid/id/<17 digits>` or differs from `identity`, that does not sign those fields,
  whose `response_nonce` is more than 10 minutes off or was already used, or that Steam does not confirm.
  The poll's `410` is `{ error: "claimed" }` after the token was handed out, `{ error: "expired" }` after 10 min;
  an unknown session is `404 login_not_found`. One address may start 30 logins per 10 minutes (`429`).
  `DELETE /api/devices/:deviceId` of a device that is not the caller's is `404 device_not_found`.

## What the app does with the files (not the server's business, but fixed here)

- The app maps `worldId → local slot` per PC (`world0.ksd`…`world9.ksd` in the local saves folder) in its own config. Only
  that file is synced. `global.ksd`, `characterN.ksd`, `user_settings.json` and `backup/` are never
  touched.
- Upload trigger: watch the slot file; after a write, wait until size and mtime are unchanged for
  5 s, then upload. Final upload when `enshrouded.exe` exits.
- Pull: only while the game is closed, write to a temp file in the same folder, then replace.
  Keep the replaced file as `<slot>.ksd.shroudtopia-<versionId>` in the app's own backup folder.
- **Store**: a shared world lives either in a Steam Cloud slot (default) or in the local saves
  folder `%USERPROFILE%\Saved Games\keen games\Enshrouded\default\`. Steam slots are written only
  through `ISteamRemoteStorage` with the game closed (a plain file copy is invisible to the game);
  reads can use the file on disk. See PLAN.md spike results.
