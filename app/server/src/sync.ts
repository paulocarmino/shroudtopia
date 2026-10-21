// The rules of the sync (app/CONTRACT.md), over SQLite. Everything here is synchronous: Node runs one of these calls at
// a time, so a decision such as "is this upload's parent the head?" and the write that follows it cannot interleave
// with another request. The bytes are not here: the routes put them in the object store first and then call
// acceptVersion.
import { DatabaseSync } from 'node:sqlite'
import { hashToken, newInviteCode, newToken, normalizeInviteCode, ulid } from './ids.ts'

export type Clock = () => number

export class SyncError extends Error {
  readonly status: number
  readonly code: string
  readonly body: Record<string, unknown> | undefined
  constructor(status: number, code: string, message: string, body?: Record<string, unknown>) {
    super(message)
    this.status = status
    this.code = code
    this.body = body
  }
}

export interface Player {
  steamId: string
  nick: string
}

export interface Version {
  versionId: string
  parent: string | null
  uploadedBy: string
  size: number
  sha256: string
  startedAt: string | null
  endedAt: string | null
  createdAt: string
}

export interface Lock {
  holder: string
  nick: string
  since: string
  expiresAt: string
}

export interface World {
  worldId: string
  name: string
  members: Player[]
  head: Version | null
  lock: Lock | null
  diverged: boolean
  history: Version[]
}

export interface Rules {
  /** how long a lease lasts without a heartbeat */
  leaseMs: number
  /** how long an invite code is valid */
  inviteMs: number
  /** a request within this long ago makes a player `appOnline` */
  onlineMs: number
  /** how many versions `history` holds */
  historyLength: number
  /** how long a Steam login session waits for the browser and the poll */
  loginMs: number
}

export const DEFAULT_RULES: Rules = {
  leaseMs: 5 * 60_000,
  inviteMs: 24 * 60 * 60_000,
  onlineMs: 2 * 60_000,
  historyLength: 20,
  loginMs: 10 * 60_000,
}

const iso = (ms: number) => new Date(ms).toISOString()
const isoOrNull = (ms: number | null) => (ms === null ? null : iso(ms))

const SCHEMA = [
  // 1
  `
  CREATE TABLE players (
    steam_id TEXT PRIMARY KEY,
    nick TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    has_avatar INTEGER NOT NULL DEFAULT 0
  );
  CREATE TABLE devices (
    token_hash TEXT PRIMARY KEY,
    steam_id TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL
  );
  CREATE INDEX devices_by_player ON devices(steam_id);
  CREATE TABLE worlds (
    world_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    head TEXT,
    diverged INTEGER NOT NULL DEFAULT 0
  );
  CREATE TABLE members (
    world_id TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    steam_id TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (world_id, steam_id)
  );
  CREATE INDEX members_by_player ON members(steam_id);
  CREATE TABLE versions (
    version_id TEXT PRIMARY KEY,
    world_id TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    parent TEXT,
    uploaded_by TEXT NOT NULL,
    size INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    started_at INTEGER,
    ended_at INTEGER,
    created_at INTEGER NOT NULL
  );
  CREATE INDEX versions_by_world ON versions(world_id);
  CREATE TABLE invites (
    code TEXT PRIMARY KEY,
    world_id TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    redeemed_by TEXT,
    redeemed_at INTEGER
  );
  CREATE TABLE locks (
    world_id TEXT PRIMARY KEY REFERENCES worlds(world_id) ON DELETE CASCADE,
    holder TEXT NOT NULL,
    since INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
  );
  `,
  // 2: Steam login. Tokens issued before it were never proven, so they are dropped.
  `
  DROP TABLE devices;
  CREATE TABLE devices (
    device_id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    steam_id TEXT NOT NULL REFERENCES players(steam_id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL
  );
  CREATE INDEX devices_by_player ON devices(steam_id);
  CREATE TABLE login_sessions (
    session_id TEXT PRIMARY KEY,
    nick TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    steam_id TEXT,
    verified_at INTEGER,
    claimed_at INTEGER
  );
  CREATE TABLE openid_nonces (
    nonce TEXT PRIMARY KEY,
    used_at INTEGER NOT NULL
  );
  `,
]

interface VersionRow {
  version_id: string
  parent: string | null
  uploaded_by: string
  size: number
  sha256: string
  started_at: number | null
  ended_at: number | null
  created_at: number
}

interface WorldRow {
  world_id: string
  name: string
  head: string | null
  diverged: number
}

interface LockRow {
  holder: string
  since: number
  expires_at: number
  nick: string
}

const toVersion = (row: VersionRow): Version => ({
  versionId: row.version_id,
  parent: row.parent,
  uploadedBy: row.uploaded_by,
  size: row.size,
  sha256: row.sha256,
  startedAt: isoOrNull(row.started_at),
  endedAt: isoOrNull(row.ended_at),
  createdAt: iso(row.created_at),
})

export class Sync {
  readonly db: DatabaseSync
  readonly rules: Rules
  readonly now: Clock

  constructor(path: string, options: { clock?: Clock; rules?: Partial<Rules> } = {}) {
    this.db = new DatabaseSync(path)
    this.now = options.clock ?? Date.now
    this.rules = { ...DEFAULT_RULES, ...options.rules }
    this.db.exec('PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;')
    this.migrate()
  }

  close(): void {
    this.db.close()
  }

  private migrate(): void {
    const { user_version: current } = this.db.prepare('PRAGMA user_version').get() as { user_version: number }
    for (let i = current; i < SCHEMA.length; i++) {
      this.tx(() => {
        this.db.exec(SCHEMA[i]!)
        this.db.exec(`PRAGMA user_version = ${i + 1}`)
      })
    }
  }

  private tx<T>(fn: () => T): T {
    this.db.exec('BEGIN IMMEDIATE')
    try {
      const result = fn()
      this.db.exec('COMMIT')
      return result
    } catch (error) {
      this.db.exec('ROLLBACK')
      throw error
    }
  }

  // ---- devices and players

  /**
   * A new device token for a Steam account whose steamId was proven (Steam login, or the tests). The nick is updated
   * to the one sent. Only the token's sha256 is kept.
   */
  issueDevice(steamId: string, nick: string): { token: string; deviceId: string; player: Player } {
    return this.tx(() => this.insertDevice(steamId, nick))
  }

  /** issueDevice without its own transaction, for callers already in one */
  private insertDevice(steamId: string, nick: string): { token: string; deviceId: string; player: Player } {
    const now = this.now()
    const token = newToken()
    const deviceId = ulid(now)
    this.db
      .prepare(
        `INSERT INTO players (steam_id, nick, created_at, last_seen_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(steam_id) DO UPDATE SET nick = excluded.nick, last_seen_at = excluded.last_seen_at`,
      )
      .run(steamId, nick, now, now)
    this.db
      .prepare('INSERT INTO devices (device_id, token_hash, steam_id, created_at, last_seen_at) VALUES (?, ?, ?, ?, ?)')
      .run(deviceId, hashToken(token), steamId, now, now)
    return { token, deviceId, player: { steamId, nick } }
  }

  /** The player and device a token belongs to, and marks both as seen; null for an unknown (or revoked) token. */
  authenticate(token: string): { player: Player; deviceId: string } | null {
    const row = this.db
      .prepare('SELECT d.device_id, p.steam_id, p.nick FROM devices d JOIN players p ON p.steam_id = d.steam_id WHERE d.token_hash = ?')
      .get(hashToken(token)) as { device_id: string; steam_id: string; nick: string } | undefined
    if (!row) return null
    const now = this.now()
    this.db.prepare('UPDATE players SET last_seen_at = ? WHERE steam_id = ?').run(now, row.steam_id)
    this.db.prepare('UPDATE devices SET last_seen_at = ? WHERE device_id = ?').run(now, row.device_id)
    return { player: { steamId: row.steam_id, nick: row.nick }, deviceId: row.device_id }
  }

  devices(steamId: string, currentDeviceId: string): { deviceId: string; createdAt: string; lastSeenAt: string; current: boolean }[] {
    const rows = this.db
      .prepare('SELECT device_id, created_at, last_seen_at FROM devices WHERE steam_id = ? ORDER BY created_at, device_id')
      .all(steamId) as { device_id: string; created_at: number; last_seen_at: number }[]
    return rows.map(row => ({ deviceId: row.device_id, createdAt: iso(row.created_at), lastSeenAt: iso(row.last_seen_at), current: row.device_id === currentDeviceId }))
  }

  /** Revokes one of the player's own devices; another player's device is as unknown as a made-up id. */
  revokeDevice(steamId: string, deviceId: string): void {
    const removed = this.db.prepare('DELETE FROM devices WHERE device_id = ? AND steam_id = ?').run(deviceId, steamId)
    if (removed.changes === 0) throw new SyncError(404, 'device_not_found', 'no such device')
  }

  setNick(steamId: string, nick: string): Player {
    this.db.prepare('UPDATE players SET nick = ? WHERE steam_id = ?').run(nick, steamId)
    return { steamId, nick }
  }

  // ---- Steam login sessions (steam.ts checks what Steam says; this keeps the state)

  startLogin(nick: string): { sessionId: string; expiresAt: number } {
    const now = this.now()
    // what expired long ago is of no use to anyone
    this.db.prepare('DELETE FROM login_sessions WHERE expires_at <= ?').run(now - 60 * 60_000)
    this.db.prepare('DELETE FROM openid_nonces WHERE used_at <= ?').run(now - 24 * 60 * 60_000)
    const sessionId = ulid(now)
    const expiresAt = now + this.rules.loginMs
    this.db.prepare('INSERT INTO login_sessions (session_id, nick, created_at, expires_at) VALUES (?, ?, ?, ?)').run(sessionId, nick, now, expiresAt)
    return { sessionId, expiresAt }
  }

  /** True when the session exists, has not expired and Steam has not vouched for it yet. */
  pendingLogin(sessionId: string): boolean {
    const row = this.db.prepare('SELECT expires_at, verified_at FROM login_sessions WHERE session_id = ?').get(sessionId) as
      | { expires_at: number; verified_at: number | null }
      | undefined
    return !!row && row.verified_at === null && row.expires_at > this.now()
  }

  /**
   * Records the steamId Steam vouched for. A nonce is used once across all sessions and a session is completed
   * once, so a replayed assertion fails here even if Steam would still call it valid.
   */
  completeLogin(sessionId: string, steamId: string, nonce: string): void {
    const now = this.now()
    this.tx(() => {
      if (!this.pendingLogin(sessionId)) throw new SyncError(410, 'expired', 'this login is expired or already done')
      const seen = this.db.prepare('SELECT 1 AS x FROM openid_nonces WHERE nonce = ?').get(nonce)
      if (seen) throw new SyncError(400, 'replayed', 'this Steam answer was already used')
      this.db.prepare('INSERT INTO openid_nonces (nonce, used_at) VALUES (?, ?)').run(nonce, now)
      this.db.prepare('UPDATE login_sessions SET steam_id = ?, verified_at = ? WHERE session_id = ?').run(steamId, now, sessionId)
    })
  }

  /** pending, or the token exactly once; picked up already or expired: 410. The device is made at pick-up. */
  pollLogin(sessionId: string): { status: 'pending' } | { token: string; player: Player } {
    const now = this.now()
    return this.tx(() => {
      const row = this.db.prepare('SELECT nick, expires_at, steam_id, verified_at, claimed_at FROM login_sessions WHERE session_id = ?').get(sessionId) as
        | { nick: string; expires_at: number; steam_id: string | null; verified_at: number | null; claimed_at: number | null }
        | undefined
      if (!row) throw new SyncError(404, 'login_not_found', 'no such login session')
      if (row.claimed_at !== null) throw new SyncError(410, 'claimed', 'this login was already picked up')
      if (row.expires_at <= now) throw new SyncError(410, 'expired', 'this login has expired, start again')
      if (row.verified_at === null || row.steam_id === null) return { status: 'pending' as const }
      this.db.prepare('UPDATE login_sessions SET claimed_at = ? WHERE session_id = ?').run(now, sessionId)
      const { token, player } = this.insertDevice(row.steam_id, row.nick)
      return { token, player }
    })
  }

  setAvatar(steamId: string, present: boolean): void {
    this.db.prepare('UPDATE players SET has_avatar = ? WHERE steam_id = ?').run(present ? 1 : 0, steamId)
  }

  hasAvatar(steamId: string): boolean {
    const row = this.db.prepare('SELECT has_avatar FROM players WHERE steam_id = ?').get(steamId) as { has_avatar: number } | undefined
    return row?.has_avatar === 1
  }

  /** True when the two players are the same or share a world. */
  canSee(viewer: string, other: string): boolean {
    if (viewer === other) return true
    const row = this.db
      .prepare('SELECT 1 AS x FROM members a JOIN members b ON a.world_id = b.world_id WHERE a.steam_id = ? AND b.steam_id = ? LIMIT 1')
      .get(viewer, other)
    return row !== undefined
  }

  // ---- worlds

  createWorld(steamId: string, name: string): World {
    const now = this.now()
    const worldId = ulid(now)
    this.tx(() => {
      this.db.prepare('INSERT INTO worlds (world_id, name, created_by, created_at) VALUES (?, ?, ?, ?)').run(worldId, name, steamId, now)
      this.db.prepare('INSERT INTO members (world_id, steam_id, joined_at) VALUES (?, ?, ?)').run(worldId, steamId, now)
    })
    return this.world(worldId)
  }

  /** The world, if the player is a member; 404 otherwise (a non-member is not told the world exists). */
  requireMember(worldId: string, steamId: string): void {
    const row = this.db.prepare('SELECT 1 AS x FROM members WHERE world_id = ? AND steam_id = ?').get(worldId, steamId)
    if (!row) throw new SyncError(404, 'world_not_found', 'no such world, or you are not a member of it')
  }

  listWorlds(steamId: string): World[] {
    const rows = this.db
      .prepare('SELECT w.world_id FROM worlds w JOIN members m ON m.world_id = w.world_id WHERE m.steam_id = ? ORDER BY w.created_at, w.world_id')
      .all(steamId) as { world_id: string }[]
    return rows.map(row => this.world(row.world_id))
  }

  world(worldId: string): World {
    const row = this.db.prepare('SELECT world_id, name, head, diverged FROM worlds WHERE world_id = ?').get(worldId) as WorldRow | undefined
    if (!row) throw new SyncError(404, 'world_not_found', 'no such world')
    const members = (
      this.db
        .prepare('SELECT p.steam_id, p.nick FROM members m JOIN players p ON p.steam_id = m.steam_id WHERE m.world_id = ? ORDER BY m.joined_at, m.rowid')
        .all(worldId) as { steam_id: string; nick: string }[]
    ).map(m => ({ steamId: m.steam_id, nick: m.nick }))
    const history = (
      this.db.prepare('SELECT * FROM versions WHERE world_id = ? ORDER BY rowid DESC LIMIT ?').all(worldId, this.rules.historyLength) as unknown as VersionRow[]
    ).map(toVersion)
    return {
      worldId: row.world_id,
      name: row.name,
      members,
      head: row.head ? this.version(worldId, row.head) : null,
      lock: this.activeLock(worldId),
      diverged: row.diverged === 1,
      history,
    }
  }

  version(worldId: string, versionId: string): Version {
    const row = this.db.prepare('SELECT * FROM versions WHERE world_id = ? AND version_id = ?').get(worldId, versionId) as unknown as VersionRow | undefined
    if (!row) throw new SyncError(404, 'version_not_found', 'no such version in this world')
    return toVersion(row)
  }

  hasVersion(worldId: string, versionId: string): boolean {
    return this.db.prepare('SELECT 1 AS x FROM versions WHERE world_id = ? AND version_id = ?').get(worldId, versionId) !== undefined
  }

  // ---- versions

  /**
   * Records an upload whose bytes are already stored under `versionId`. The rule: when its parent is the head (both
   * null for the first upload) it becomes the head; otherwise it is kept as a branch, head stays, and the world is
   * diverged until someone resolves it. A parent that is not a version of this world is refused.
   */
  acceptVersion(
    worldId: string,
    uploadedBy: string,
    upload: { versionId: string; parent: string | null; size: number; sha256: string; startedAt: number | null; endedAt: number | null },
  ): { version: Version; head: string | null; diverged: boolean } {
    return this.tx(() => {
      const world = this.db.prepare('SELECT head, diverged FROM worlds WHERE world_id = ?').get(worldId) as { head: string | null; diverged: number } | undefined
      if (!world) throw new SyncError(404, 'world_not_found', 'no such world')
      if (upload.parent !== null && !this.hasVersion(worldId, upload.parent)) {
        throw new SyncError(400, 'unknown_parent', 'X-Parent is not a version of this world')
      }
      this.db
        .prepare(
          `INSERT INTO versions (version_id, world_id, parent, uploaded_by, size, sha256, started_at, ended_at, created_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
        )
        .run(upload.versionId, worldId, upload.parent, uploadedBy, upload.size, upload.sha256, upload.startedAt, upload.endedAt, this.now())
      let head = world.head
      let diverged = world.diverged === 1
      if (upload.parent === world.head) {
        head = upload.versionId
      } else {
        diverged = true
      }
      this.db.prepare('UPDATE worlds SET head = ?, diverged = ? WHERE world_id = ?').run(head, diverged ? 1 : 0, worldId)
      return { version: this.version(worldId, upload.versionId), head, diverged }
    })
  }

  /** The chosen version becomes head and the world is no longer diverged; every version stays in history. */
  resolve(worldId: string, versionId: string): World {
    if (!this.hasVersion(worldId, versionId)) throw new SyncError(400, 'unknown_version', 'versionId is not a version of this world')
    this.db.prepare('UPDATE worlds SET head = ?, diverged = 0 WHERE world_id = ?').run(versionId, worldId)
    return this.world(worldId)
  }

  // ---- invites

  createInvite(worldId: string, steamId: string): { code: string; expiresAt: string } {
    const now = this.now()
    const expiresAt = now + this.rules.inviteMs
    for (let attempt = 0; attempt < 20; attempt++) {
      const code = newInviteCode()
      // a code that is spent or expired can be given out again
      const taken = this.db.prepare('SELECT redeemed_at, expires_at FROM invites WHERE code = ?').get(code) as { redeemed_at: number | null; expires_at: number } | undefined
      if (taken && taken.redeemed_at === null && taken.expires_at > now) continue
      this.db
        .prepare(
          `INSERT INTO invites (code, world_id, created_by, created_at, expires_at) VALUES (?, ?, ?, ?, ?)
           ON CONFLICT(code) DO UPDATE SET world_id = excluded.world_id, created_by = excluded.created_by,
             created_at = excluded.created_at, expires_at = excluded.expires_at, redeemed_by = NULL, redeemed_at = NULL`,
        )
        .run(code, worldId, steamId, now, expiresAt)
      return { code, expiresAt: iso(expiresAt) }
    }
    throw new SyncError(503, 'no_code', 'could not find a free invite code, try again')
  }

  /**
   * Adds the player to the invite's world. One use, 24 h. A player who is already a member gets the world back and
   * the code is not spent.
   */
  redeemInvite(text: string, steamId: string): World {
    const code = normalizeInviteCode(text)
    const now = this.now()
    return this.tx(() => {
      const invite = this.db.prepare('SELECT world_id, expires_at, redeemed_at FROM invites WHERE code = ?').get(code) as
        | { world_id: string; expires_at: number; redeemed_at: number | null }
        | undefined
      if (!invite) throw new SyncError(404, 'invite_not_found', 'no such invite code')
      const member = this.db.prepare('SELECT 1 AS x FROM members WHERE world_id = ? AND steam_id = ?').get(invite.world_id, steamId)
      if (member) return this.world(invite.world_id)
      if (invite.redeemed_at !== null) throw new SyncError(410, 'invite_used', 'this invite code was already used')
      if (invite.expires_at <= now) throw new SyncError(410, 'invite_expired', 'this invite code has expired')
      this.db.prepare('UPDATE invites SET redeemed_by = ?, redeemed_at = ? WHERE code = ?').run(steamId, now, code)
      this.db.prepare('INSERT INTO members (world_id, steam_id, joined_at) VALUES (?, ?, ?)').run(invite.world_id, steamId, now)
      return this.world(invite.world_id)
    })
  }

  // ---- the host lock

  /** The lease, or null when there is none or it has expired. */
  activeLock(worldId: string): Lock | null {
    const row = this.db
      .prepare('SELECT l.holder, l.since, l.expires_at, p.nick FROM locks l JOIN players p ON p.steam_id = l.holder WHERE l.world_id = ?')
      .get(worldId) as LockRow | undefined
    if (!row || row.expires_at <= this.now()) return null
    return { holder: row.holder, nick: row.nick, since: iso(row.since), expiresAt: iso(row.expires_at) }
  }

  /**
   * Takes the lease for the player, or renews it if they hold it already. Someone else holding an unexpired lease is
   * a 409 that carries their lock. A heartbeat is the same call: it renews, or takes a lease that is free again
   * (expired while the PC slept).
   */
  takeLock(worldId: string, steamId: string): Lock {
    const now = this.now()
    return this.tx(() => {
      const current = this.activeLock(worldId)
      if (current && current.holder !== steamId) throw new SyncError(409, 'lock_held', `${current.nick} is hosting this world`, { lock: current })
      const since = current ? Date.parse(current.since) : now
      this.db
        .prepare(
          `INSERT INTO locks (world_id, holder, since, expires_at) VALUES (?, ?, ?, ?)
           ON CONFLICT(world_id) DO UPDATE SET holder = excluded.holder, since = excluded.since, expires_at = excluded.expires_at`,
        )
        .run(worldId, steamId, since, now + this.rules.leaseMs)
      return this.activeLock(worldId)!
    })
  }

  /** Releases the player's lease. Nothing to release is fine; someone else's lease is a 409. */
  releaseLock(worldId: string, steamId: string): void {
    const current = this.activeLock(worldId)
    if (current && current.holder !== steamId) throw new SyncError(409, 'lock_held', `${current.nick} is hosting this world`, { lock: current })
    this.db.prepare('DELETE FROM locks WHERE world_id = ?').run(worldId)
  }

  // ---- presence

  /** The caller and everyone who shares a world with them. */
  presence(steamId: string): { steamId: string; nick: string; appOnline: boolean; inGame: boolean }[] {
    const now = this.now()
    const rows = this.db
      .prepare(
        `SELECT DISTINCT p.steam_id, p.nick, p.last_seen_at,
           EXISTS (SELECT 1 FROM locks l WHERE l.holder = p.steam_id AND l.expires_at > ?) AS in_game
         FROM players p
         WHERE p.steam_id = ?
            OR p.steam_id IN (SELECT b.steam_id FROM members a JOIN members b ON a.world_id = b.world_id WHERE a.steam_id = ?)
         ORDER BY p.steam_id`,
      )
      .all(now, steamId, steamId) as { steam_id: string; nick: string; last_seen_at: number; in_game: number }[]
    return rows.map(row => ({
      steamId: row.steam_id,
      nick: row.nick,
      appOnline: now - row.last_seen_at <= this.rules.onlineMs,
      inGame: row.in_game === 1,
    }))
  }

  // ---- maintenance (cli.ts)

  /** Deletes the world and everything of it; returns the blob keys to delete from the store. */
  deleteWorld(worldId: string): string[] {
    const versions = this.db.prepare('SELECT version_id FROM versions WHERE world_id = ?').all(worldId) as { version_id: string }[]
    const removed = this.db.prepare('DELETE FROM worlds WHERE world_id = ?').run(worldId)
    if (removed.changes === 0) throw new SyncError(404, 'world_not_found', `no world ${worldId}`)
    return versions.map(v => `worlds/${worldId}/${v.version_id}`)
  }

  /** Deletes a player, their devices and memberships. Worlds they made stay (others may be members). */
  deletePlayer(steamId: string): { hadAvatar: boolean } {
    const hadAvatar = this.hasAvatar(steamId)
    const removed = this.tx(() => {
      this.db.prepare('DELETE FROM locks WHERE holder = ?').run(steamId)
      return this.db.prepare('DELETE FROM players WHERE steam_id = ?').run(steamId)
    })
    if (removed.changes === 0) throw new SyncError(404, 'player_not_found', `no player ${steamId}`)
    return { hadAvatar }
  }
}
