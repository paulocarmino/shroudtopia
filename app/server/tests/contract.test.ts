// app/contract.example.json replayed against the server: every request in it is sent as it is (with real ids where the
// example's are placeholders), and every response must have the example's shape: the same keys, the same JSON types,
// recursively (a null in the example allows a value of the type the contract gives that field).
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { bearer, callbackUrl, harness, ORIGIN, sha256, steamLogin, type Harness } from './helpers.ts'

const example = JSON.parse(readFileSync(resolve(import.meta.dirname, '../../contract.example.json'), 'utf8'))

type Json = null | boolean | number | string | Json[] | { [key: string]: Json }

/** Where `actual` differs in shape from `expected`, as a list of paths; empty when it matches. */
function shapeDiff(expected: Json, actual: unknown, path = '$', nullable: Record<string, Json> = {}): string[] {
  const kind = (v: unknown) => (v === null ? 'null' : Array.isArray(v) ? 'array' : typeof v)
  if (expected === null) {
    // null in the example: a null answer, or the shape the contract gives the field
    const shape = nullable[path.split('.').pop()!]
    if (actual === null) return []
    return shape === undefined ? [`${path}: expected null, got ${kind(actual)}`] : shapeDiff(shape, actual, path, nullable)
  }
  if (kind(expected) !== kind(actual)) {
    // a field the example shows null-able by contract (parent, endedAt...) may be null in a real answer
    if (actual === null && path.split('.').pop()! in nullable) return []
    return [`${path}: expected ${kind(expected)}, got ${kind(actual)}`]
  }
  if (Array.isArray(expected)) {
    const items = actual as unknown[]
    if (expected.length === 0) return []
    return items.flatMap((item, i) => shapeDiff(expected[0]!, item, `${path}[${i}]`, nullable))
  }
  if (typeof expected === 'object') {
    const object = actual as Record<string, unknown>
    return Object.entries(expected).flatMap(([key, value]) =>
      key in object ? shapeDiff(value, object[key], `${path}.${key}`, nullable) : [`${path}.${key}: missing`],
    )
  }
  return []
}

// what a null in the example stands for, from CONTRACT.md
const version = example.world.head
const NULLABLE: Record<string, Json> = {
  lock: example.lockTaken.response.lock,
  head: version,
  parent: 'ULID',
  endedAt: 'iso',
  startedAt: 'iso',
}

let h: Harness
let tokenA: string
let tokenB: string
let worldId: string
let v1: string
let v2: string

beforeAll(async () => {
  h = await harness()
})
afterAll(async () => {
  await h.close()
})

describe('contract.example.json', () => {
  it('steamLoginStart, steamLoginPending, steamLoginDone', async () => {
    const start = await h.app.inject({ method: 'POST', url: '/api/auth/steam/start', payload: example.steamLoginStart.request })
    expect(start.statusCode).toBe(200)
    expect(shapeDiff(example.steamLoginStart.response, start.json())).toEqual([])
    const { sessionId, loginUrl, expiresAt } = start.json()
    expect(loginUrl).toBe(`${ORIGIN}/auth/steam/redirect/${sessionId}`)
    expect(expiresAt).toBe('2026-10-21T21:10:00.000Z')

    const pending = await h.app.inject({ method: 'GET', url: `/api/auth/steam/poll/${sessionId}` })
    expect(pending.statusCode).toBe(example.steamLoginPending.status)
    expect(pending.json()).toEqual(example.steamLoginPending.response)

    const steamId = example.steamLoginDone.response.player.steamId
    const query = h.steam.assert(steamId, `${ORIGIN}/auth/steam/callback/${sessionId}`, h.clock.now)
    expect((await h.app.inject({ method: 'GET', url: callbackUrl(sessionId, query) })).statusCode).toBe(200)

    const done = await h.app.inject({ method: 'GET', url: `/api/auth/steam/poll/${sessionId}` })
    expect(done.statusCode).toBe(200)
    expect(shapeDiff(example.steamLoginDone.response, done.json())).toEqual([])
    // the nick, full-width characters and all, is the one the app sent at the start
    expect(done.json().player).toEqual(example.steamLoginDone.response.player)
    expect(done.json().token).toMatch(/^dev_/)
    tokenA = done.json().token
    const other = example.world.members[1]
    tokenB = await steamLogin(h, other.steamId, other.nick)
  })

  it('devices', async () => {
    const res = await h.app.inject({ method: 'GET', url: '/api/devices', headers: bearer(tokenA) })
    expect(res.statusCode).toBe(200)
    expect(shapeDiff(example.devices.response, res.json())).toEqual([])
    expect(res.json().devices).toHaveLength(1)
    expect(res.json().devices[0].current).toBe(true)
  })

  it('world (POST /api/worlds, GET /api/worlds/:id, GET /api/worlds)', async () => {
    const res = await h.app.inject({ method: 'POST', url: '/api/worlds', headers: bearer(tokenA), payload: { name: example.world.name } })
    expect(res.statusCode).toBe(200)
    const world = res.json()
    expect(shapeDiff(example.world, world, '$', NULLABLE)).toEqual([])
    expect(world.worldId).toMatch(/^[0-9A-HJKMNP-TV-Z]{26}$/)
    expect(world).toMatchObject({ name: example.world.name, head: null, lock: null, diverged: false, history: [] })
    worldId = world.worldId
    const list = (await h.app.inject({ method: 'GET', url: '/api/worlds', headers: bearer(tokenA) })).json()
    expect(list.worlds).toHaveLength(1)
    expect(shapeDiff(example.world, list.worlds[0], '$', NULLABLE)).toEqual([])
  })

  it('createInvite and redeemInvite', async () => {
    const res = await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/invites`, headers: bearer(tokenA) })
    expect(res.statusCode).toBe(200)
    expect(shapeDiff(example.createInvite.response, res.json())).toEqual([])
    expect(res.json().code).toMatch(/^ENSH-[0-9A-Z]{4}$/)
    const redeemed = await h.app.inject({ method: 'POST', url: '/api/invites/redeem', headers: bearer(tokenB), payload: { ...example.redeemInvite.request, code: res.json().code } })
    expect(redeemed.statusCode).toBe(200)
    expect(shapeDiff(example.world, redeemed.json(), '$', NULLABLE)).toEqual([])
    expect(redeemed.json().members).toEqual(example.world.members)
  })

  it('upload (and uploadDiverged), with the example headers', async () => {
    const headers = example.uploadDiverged.requestHeaders
    const first = Buffer.from('first')
    const r1 = await h.app.inject({
      method: 'POST',
      url: `/api/worlds/${worldId}/versions`,
      headers: { ...bearer(tokenB), 'content-type': 'application/octet-stream', 'X-Sha256': sha256(first), 'X-Started-At': headers['X-Started-At'], 'X-Ended-At': headers['X-Ended-At'] },
      payload: first,
    })
    expect(r1.statusCode).toBe(200)
    v1 = r1.json().version.versionId
    expect(shapeDiff(version, r1.json().version, '$.version', NULLABLE)).toEqual([])
    const second = Buffer.from('second')
    v2 = (
      await h.app.inject({
        method: 'POST',
        url: `/api/worlds/${worldId}/versions`,
        headers: { ...bearer(tokenB), 'content-type': 'application/octet-stream', 'X-Parent': v1, 'X-Sha256': sha256(second) },
        payload: second,
      })
    ).json().version.versionId

    const stale = Buffer.from('stale')
    const res = await h.app.inject({
      method: 'POST',
      url: `/api/worlds/${worldId}/versions`,
      headers: { ...bearer(tokenA), 'content-type': 'application/octet-stream', ...headers, 'X-Parent': v1, 'X-Sha256': sha256(stale) },
      payload: stale,
    })
    expect(res.statusCode).toBe(200)
    expect(shapeDiff(example.uploadDiverged.response, res.json(), '$', NULLABLE)).toEqual([])
    expect(res.json()).toMatchObject({ diverged: true, head: v2 })
    expect(res.json().version).toMatchObject({ parent: v1, startedAt: headers['X-Started-At'], endedAt: headers['X-Ended-At'] })

    const world = (await h.app.inject({ method: 'GET', url: `/api/worlds/${worldId}`, headers: bearer(tokenA) })).json()
    expect(shapeDiff(example.world, world, '$', NULLABLE)).toEqual([])
    expect(world.head.versionId).toBe(v2)
  })

  it('lockTaken and lockBusy', async () => {
    const taken = await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenA) })
    expect(taken.statusCode).toBe(200)
    expect(shapeDiff(example.lockTaken.response, taken.json())).toEqual([])
    expect(taken.json().lock).toMatchObject({ holder: example.steamLoginDone.response.player.steamId, nick: example.steamLoginDone.response.player.nick })
    const busy = await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenB) })
    expect(busy.statusCode).toBe(example.lockBusy.status)
    expect(shapeDiff(example.lockBusy.response, busy.json())).toEqual([])
    expect(busy.json().error).toBe('lock_held')
  })

  it('presence', async () => {
    const res = await h.app.inject({ method: 'GET', url: '/api/presence', headers: bearer(tokenB) })
    expect(res.statusCode).toBe(200)
    expect(shapeDiff(example.presence.response, res.json())).toEqual([])
    expect(res.json().players).toHaveLength(2)
  })

  it('resolve answers a World; heartbeat and release', async () => {
    const res = await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/resolve`, headers: bearer(tokenA), payload: { versionId: v1 } })
    expect(res.statusCode).toBe(200)
    expect(shapeDiff(example.world, res.json(), '$', NULLABLE)).toEqual([])
    const beat = await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock/heartbeat`, headers: bearer(tokenA) })
    expect(shapeDiff(example.lockTaken.response, beat.json())).toEqual([])
    const release = await h.app.inject({ method: 'DELETE', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenA) })
    expect(release.statusCode).toBe(204)
  })

  it('errors are { error, message }', async () => {
    for (const res of [
      await h.app.inject({ method: 'GET', url: '/api/worlds' }),
      await h.app.inject({ method: 'GET', url: '/api/nope', headers: bearer(tokenA) }),
      await h.app.inject({ method: 'POST', url: '/api/auth/steam/start', payload: {} }),
      await h.app.inject({ method: 'POST', url: '/api/auth/steam/start', headers: { 'content-type': 'application/json' }, payload: '{bad' }),
    ]) {
      expect(res.statusCode).toBeGreaterThanOrEqual(400)
      expect(shapeDiff({ error: 'code', message: 'text' }, res.json())).toEqual([])
    }
  })
})
