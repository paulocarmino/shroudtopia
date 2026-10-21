// The HTTP side of app/CONTRACT.md: every route of its table, plus /healthz and the avatar read-back.
import { createHash } from 'node:crypto'
import Fastify, { type FastifyInstance, type FastifyReply, type FastifyRequest } from 'fastify'
import secureJson from 'secure-json-parse'
import { ULID, ulid } from './ids.ts'
import { avatarKey, worldFileKey, type ObjectStore } from './object-store.ts'
import { hostOf, type LandingFile } from './landing.ts'
import { donePage, failPage, SteamLoginError, steamLoginUrl, verifySteamAssertion, type SteamSettings } from './steam.ts'
import { SyncError, type Player, type Sync } from './sync.ts'

export interface ServerOptions {
  sync: Sync
  store: ObjectStore
  /** the public origin and Steam's OpenID endpoint */
  steam: SteamSettings
  /** the landing page's hostname and files; requests for that Host get those files and nothing else */
  landing?: { host: string; files: Map<string, LandingFile> }
  /** login sessions one address may start per window */
  loginStarts?: { max: number; windowMs: number }
  build?: string
  log?: boolean
  trustProxy?: number
  /** the largest world file accepted */
  maxWorldBytes?: number
  /** failed invite redeems a device may make per window before it is told to wait */
  redeemFailures?: { max: number; windowMs: number }
}

const MAX_AVATAR_BYTES = 512 * 1024
const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
const STEAM_ID = /^\d{17}$/
const SHA256 = /^[0-9a-f]{64}$/

declare module 'fastify' {
  interface FastifyRequest {
    player: Player
    token: string
    deviceId: string
  }
}

const fail = (status: number, code: string, message: string) => new SyncError(status, code, message)

function header(request: FastifyRequest, name: string): string | undefined {
  const value = request.headers[name]
  const text = Array.isArray(value) ? value[0] : value
  return text === undefined || text.trim() === '' ? undefined : text.trim()
}

function timeHeader(request: FastifyRequest, name: string): number | null {
  const text = header(request, name)
  if (text === undefined || text === 'null') return null
  const ms = Date.parse(text)
  if (Number.isNaN(ms)) throw fail(400, 'bad_header', `${name} must be an ISO 8601 time`)
  return ms
}

function stringField(body: unknown, name: string, max: number): string {
  const value = (body as Record<string, unknown> | null | undefined)?.[name]
  if (typeof value !== 'string' || value.trim() === '') throw fail(400, 'bad_request', `${name} is required`)
  if (value.length > max) throw fail(400, 'bad_request', `${name} is longer than ${max} characters`)
  return value
}

export async function buildServer(options: ServerOptions): Promise<FastifyInstance> {
  const { sync, store } = options
  const maxWorldBytes = options.maxWorldBytes ?? 64 * 1024 * 1024
  const redeemLimit = options.redeemFailures ?? { max: 10, windowMs: 10 * 60_000 }
  const failedRedeems = new Map<string, number[]>()

  const app = Fastify({
    logger: options.log ? { level: 'info' } : false,
    // the nearest n hops (Envoy) are trusted for X-Forwarded-For
    trustProxy: (_address: string, hop: number) => hop < (options.trustProxy ?? 0),
    bodyLimit: 64 * 1024,
  })

  // JSON with an empty body is an empty object (a POST with a JSON content type and nothing in it)
  app.removeContentTypeParser('application/json')
  app.addContentTypeParser('application/json', { parseAs: 'string' }, (_request, body, done) => {
    const text = (body as string).trim()
    if (text === '') return done(null, {})
    try {
      done(null, secureJson.parse(text))
    } catch {
      done(fail(400, 'bad_json', 'the body is not valid JSON'), undefined)
    }
  })
  // bytes: world files and avatars, whatever content type the client names
  app.addContentTypeParser('*', { parseAs: 'buffer' }, (_request, body, done) => done(null, body))

  app.setErrorHandler((error: unknown, request, reply) => {
    if (error instanceof SyncError) {
      return reply.status(error.status).send({ error: error.code, message: error.message, ...error.body })
    }
    const fastifyError = error as { statusCode?: number; code?: string; message?: string }
    const status = fastifyError.statusCode ?? 500
    if (status >= 500) request.log.error(error)
    const code = status === 413 ? 'too_large' : status >= 500 ? 'internal' : 'bad_request'
    return reply.status(status).send({ error: code, message: status >= 500 ? 'internal error' : (fastifyError.message ?? 'bad request') })
  })
  // The landing host is answered here, before any route: its files, or a plain 404. The raw Host header decides
  // (what Envoy routed on), never X-Forwarded-Host, which a client could write.
  const landing = options.landing
  if (landing) {
    const landingHost = hostOf(landing.host)
    app.addHook('onRequest', async (request, reply) => {
      if (hostOf(request.headers.host) !== landingHost) return
      const path = request.url.split('?')[0]!
      const file = request.method === 'GET' || request.method === 'HEAD' ? landing.files.get(path) : undefined
      if (!file) {
        return reply.status(404).header('content-type', 'text/plain; charset=utf-8').header('cache-control', 'no-store').send('Not found\n')
      }
      return reply.header('content-type', file.type).header('cache-control', file.cache).header('x-content-type-options', 'nosniff').send(file.body)
    })
  }

  app.setNotFoundHandler((request, reply) => reply.status(404).send({ error: 'not_found', message: `no route ${request.method} ${request.url}` }))

  const authenticate = async (request: FastifyRequest) => {
    const auth = header(request, 'authorization')
    const token = auth?.startsWith('Bearer ') ? auth.slice(7).trim() : undefined
    const found = token ? sync.authenticate(token) : null
    if (!token || !found) throw fail(401, 'unauthorized', 'a valid device token is required (Authorization: Bearer <token>)')
    request.player = found.player
    request.deviceId = found.deviceId
    request.token = token
  }

  const worldParam = (request: FastifyRequest): string => {
    const { id } = request.params as { id: string }
    if (!ULID.test(id)) throw fail(404, 'world_not_found', 'no such world, or you are not a member of it')
    sync.requireMember(id, request.player.steamId)
    return id
  }

  // the probes call this every few seconds: not logged
  app.get('/healthz', { logLevel: 'silent' }, async () => ({ ok: true, build: options.build ?? 'dev' }))

  // ---- Steam login: the only way to a device token (CONTRACT.md, "Steam login")

  const startLimit = options.loginStarts ?? { max: 30, windowMs: 10 * 60_000 }
  const starts = new Map<string, number[]>()

  app.post('/api/auth/steam/start', async request => {
    const now = Date.now()
    const recent = (starts.get(request.ip) ?? []).filter(at => now - at < startLimit.windowMs)
    if (recent.length >= startLimit.max) throw fail(429, 'too_many_attempts', 'too many logins started, wait a few minutes')
    recent.push(now)
    starts.set(request.ip, recent)
    const nick = stringField(request.body, 'nick', 64)
    const { sessionId, expiresAt } = sync.startLogin(nick)
    return { sessionId, loginUrl: `${options.steam.origin}/auth/steam/redirect/${sessionId}`, expiresAt: new Date(expiresAt).toISOString() }
  })

  const html = (reply: FastifyReply, status: number, body: string) =>
    reply.status(status).header('content-type', 'text/html; charset=utf-8').header('cache-control', 'no-store').send(body)

  app.get('/auth/steam/redirect/:sessionId', async (request, reply) => {
    const { sessionId } = request.params as { sessionId: string }
    if (!ULID.test(sessionId) || !sync.pendingLogin(sessionId)) return html(reply, 410, failPage('expired'))
    return reply.header('cache-control', 'no-store').redirect(steamLoginUrl(options.steam, sessionId), 302)
  })

  app.get('/auth/steam/callback/:sessionId', async (request, reply) => {
    const { sessionId } = request.params as { sessionId: string }
    if (!ULID.test(sessionId) || !sync.pendingLogin(sessionId)) return html(reply, 410, failPage('expired'))
    try {
      const { steamId, nonce } = await verifySteamAssertion(options.steam, sessionId, request.query as Record<string, unknown>, sync.now())
      sync.completeLogin(sessionId, steamId, nonce)
    } catch (error) {
      if (error instanceof SteamLoginError) {
        request.log.warn({ reason: error.reason, detail: error.message }, 'steam login refused')
        return html(reply, error.reason === 'steam_unreachable' ? 502 : 400, failPage(error.reason))
      }
      if (error instanceof SyncError) {
        request.log.warn({ reason: error.code }, 'steam login refused')
        return html(reply, error.status, failPage(error.code))
      }
      throw error
    }
    return html(reply, 200, donePage())
  })

  app.get('/api/auth/steam/poll/:sessionId', async (request, reply) => {
    const { sessionId } = request.params as { sessionId: string }
    if (!ULID.test(sessionId)) throw fail(404, 'login_not_found', 'no such login session')
    const result = sync.pollLogin(sessionId)
    return 'status' in result ? reply.status(202).send(result) : result
  })

  app.register(async authed => {
    authed.addHook('onRequest', authenticate)

    // ---- devices and me

    authed.get('/api/devices', async request => ({ devices: sync.devices(request.player.steamId, request.deviceId) }))

    authed.delete('/api/devices/:deviceId', async (request, reply) => {
      const { deviceId } = request.params as { deviceId: string }
      sync.revokeDevice(request.player.steamId, deviceId === 'current' ? request.deviceId : deviceId)
      return reply.status(204).send()
    })

    authed.patch('/api/me', async request => ({ player: sync.setNick(request.player.steamId, stringField(request.body, 'nick', 64)) }))

    authed.put('/api/me/avatar', { bodyLimit: MAX_AVATAR_BYTES }, async (request, reply) => {
      const body = request.body
      if (!Buffer.isBuffer(body) || body.length < PNG_SIGNATURE.length || !body.subarray(0, 8).equals(PNG_SIGNATURE)) {
        throw fail(400, 'not_png', 'the avatar must be a PNG')
      }
      await store.put(avatarKey(request.player.steamId), body, 'image/png')
      sync.setAvatar(request.player.steamId, true)
      return reply.status(204).send()
    })

    authed.get('/api/players/:steamId/avatar', async (request, reply) => {
      const { steamId } = request.params as { steamId: string }
      if (!STEAM_ID.test(steamId) || !sync.canSee(request.player.steamId, steamId) || !sync.hasAvatar(steamId)) {
        throw fail(404, 'avatar_not_found', 'no avatar for this player')
      }
      const found = await store.get(avatarKey(steamId))
      if (!found) throw fail(404, 'avatar_not_found', 'no avatar for this player')
      return reply.header('content-type', 'image/png').header('content-length', found.size).header('cache-control', 'private, max-age=300').send(found.body)
    })

    // ---- worlds

    authed.get('/api/worlds', async request => ({ worlds: sync.listWorlds(request.player.steamId) }))

    authed.post('/api/worlds', async request => sync.createWorld(request.player.steamId, stringField(request.body, 'name', 80).trim()))

    authed.get('/api/worlds/:id', async request => sync.world(worldParam(request)))

    authed.post('/api/worlds/:id/invites', async request => sync.createInvite(worldParam(request), request.player.steamId))

    authed.post('/api/invites/redeem', async request => {
      const key = request.token
      const now = Date.now()
      const recent = (failedRedeems.get(key) ?? []).filter(at => now - at < redeemLimit.windowMs)
      if (recent.length >= redeemLimit.max) throw fail(429, 'too_many_attempts', 'too many wrong invite codes, wait a few minutes')
      const code = stringField(request.body, 'code', 16)
      try {
        const world = sync.redeemInvite(code, request.player.steamId)
        failedRedeems.delete(key)
        return world
      } catch (error) {
        if (error instanceof SyncError && error.status !== 429) {
          recent.push(now)
          failedRedeems.set(key, recent)
        }
        throw error
      }
    })

    authed.post('/api/worlds/:id/resolve', async request => {
      const worldId = worldParam(request)
      return sync.resolve(worldId, stringField(request.body, 'versionId', 26))
    })

    // ---- versions

    authed.post('/api/worlds/:id/versions', { bodyLimit: maxWorldBytes }, async request => {
      const worldId = worldParam(request)
      const body = request.body
      if (!Buffer.isBuffer(body) || body.length === 0) throw fail(400, 'empty_body', 'the body must be the world file')
      const parentText = header(request, 'x-parent')
      const parent = parentText === undefined || parentText === 'null' ? null : parentText
      if (parent !== null && !ULID.test(parent)) throw fail(400, 'unknown_parent', 'X-Parent is not a version of this world')
      const claimed = header(request, 'x-sha256')?.toLowerCase()
      if (!claimed || !SHA256.test(claimed)) throw fail(400, 'bad_header', 'X-Sha256 must be the file sha256 as 64 hex characters')
      const sha256 = createHash('sha256').update(body).digest('hex')
      if (sha256 !== claimed) throw fail(400, 'sha256_mismatch', `the body's sha256 is ${sha256}, not the X-Sha256 sent`)
      const startedAt = timeHeader(request, 'x-started-at')
      const endedAt = timeHeader(request, 'x-ended-at')
      // fail before storing bytes for an upload that will be refused
      if (parent !== null && !sync.hasVersion(worldId, parent)) throw fail(400, 'unknown_parent', 'X-Parent is not a version of this world')

      const versionId = ulid()
      const key = worldFileKey(worldId, versionId)
      await store.put(key, body, 'application/octet-stream')
      try {
        return sync.acceptVersion(worldId, request.player.steamId, { versionId, parent, size: body.length, sha256, startedAt, endedAt })
      } catch (error) {
        await store.delete(key).catch(() => {})
        throw error
      }
    })

    authed.get('/api/worlds/:id/versions/:vid/file', async (request: FastifyRequest, reply: FastifyReply) => {
      const worldId = worldParam(request)
      const { vid } = request.params as { vid: string }
      if (!ULID.test(vid)) throw fail(404, 'version_not_found', 'no such version in this world')
      const version = sync.version(worldId, vid)
      const found = await store.get(worldFileKey(worldId, vid))
      if (!found) throw fail(404, 'file_missing', 'the version is recorded but its file is not in the store')
      return reply
        .header('content-type', 'application/octet-stream')
        .header('content-length', found.size)
        .header('x-sha256', version.sha256)
        .header('x-version-id', version.versionId)
        .send(found.body)
    })

    // ---- the host lock

    authed.post('/api/worlds/:id/lock', async request => ({ lock: sync.takeLock(worldParam(request), request.player.steamId) }))

    authed.post('/api/worlds/:id/lock/heartbeat', async request => ({ lock: sync.takeLock(worldParam(request), request.player.steamId) }))

    authed.delete('/api/worlds/:id/lock', async (request, reply) => {
      sync.releaseLock(worldParam(request), request.player.steamId)
      return reply.status(204).send()
    })

    // ---- presence

    authed.get('/api/presence', async request => ({ players: sync.presence(request.player.steamId) }))
  })

  await app.ready()
  return app
}
