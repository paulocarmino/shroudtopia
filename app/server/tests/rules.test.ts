// The rules of app/CONTRACT.md, through the real routes: parent/head/diverged, the lease, the invite.
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { bearer, device, harness, sha256, upload, type Harness } from './helpers.ts'

const A = '76561198065863968'
const B = '76561198000000001'

let h: Harness
let tokenA: string
let tokenB: string
let worldId: string

beforeEach(async () => {
  h = await harness()
  tokenA = await device(h, A, 'laranjo')
  tokenB = await device(h, B, 'rodrigao_br')
  const res = await h.app.inject({ method: 'POST', url: '/api/worlds', headers: bearer(tokenA), payload: { name: 'Mundo do bang' } })
  worldId = res.json().worldId
  const code = (await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/invites`, headers: bearer(tokenA) })).json().code
  await h.app.inject({ method: 'POST', url: '/api/invites/redeem', headers: bearer(tokenB), payload: { code } })
})

afterEach(async () => {
  await h.close()
})

const getWorld = async (token = tokenA) => (await h.app.inject({ method: 'GET', url: `/api/worlds/${worldId}`, headers: bearer(token) })).json()

describe('versions: parent, head, diverged', () => {
  it('the first upload with no parent becomes head', async () => {
    const bytes = Buffer.from('world v1')
    const res = await upload(h.app, tokenA, worldId, bytes, null)
    expect(res.statusCode).toBe(200)
    const body = res.json()
    expect(body.diverged).toBe(false)
    expect(body.head).toBe(body.version.versionId)
    expect(body.version).toMatchObject({ parent: null, uploadedBy: A, size: bytes.length, sha256: sha256(bytes) })
    const world = await getWorld()
    expect(world.head.versionId).toBe(body.version.versionId)
    expect(world.diverged).toBe(false)
    // the bytes are in the store under the version
    expect(h.store.objects.get(`worlds/${worldId}/${body.version.versionId}`)?.body.equals(bytes)).toBe(true)
  })

  it('parent == head moves head; a stale parent is a branch: head stays, diverged until resolved', async () => {
    const v1 = (await upload(h.app, tokenA, worldId, Buffer.from('v1'), null)).json().version.versionId
    const v2res = (await upload(h.app, tokenB, worldId, Buffer.from('v2'), v1)).json()
    expect(v2res).toMatchObject({ head: v2res.version.versionId, diverged: false })
    const v2 = v2res.version.versionId

    // A played offline from v1 too
    const stale = (await upload(h.app, tokenA, worldId, Buffer.from('v1-offline'), v1)).json()
    expect(stale.diverged).toBe(true)
    expect(stale.head).toBe(v2)
    expect(stale.version.parent).toBe(v1)

    let world = await getWorld(tokenB)
    expect(world.head.versionId).toBe(v2)
    expect(world.diverged).toBe(true)
    expect(world.history.map((v: { versionId: string }) => v.versionId)).toEqual([stale.version.versionId, v2, v1])

    // a later upload on head moves head but the world stays diverged: only resolve clears it
    const v3 = (await upload(h.app, tokenB, worldId, Buffer.from('v3'), v2)).json()
    expect(v3).toMatchObject({ head: v3.version.versionId, diverged: true })

    const resolved = await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/resolve`, headers: bearer(tokenA), payload: { versionId: stale.version.versionId } })
    expect(resolved.statusCode).toBe(200)
    world = resolved.json()
    expect(world.head.versionId).toBe(stale.version.versionId)
    expect(world.diverged).toBe(false)
    expect(world.history).toHaveLength(4)

    // after resolving, the chosen version is the parent that moves head
    const next = (await upload(h.app, tokenA, worldId, Buffer.from('v5'), stale.version.versionId)).json()
    expect(next).toMatchObject({ head: next.version.versionId, diverged: false })
  })

  it('two uploads racing on the same parent: one becomes head, the other a branch', async () => {
    const v1 = (await upload(h.app, tokenA, worldId, Buffer.from('v1'), null)).json().version.versionId
    const [a, b] = (await Promise.all([upload(h.app, tokenA, worldId, Buffer.from('a'), v1), upload(h.app, tokenB, worldId, Buffer.from('b'), v1)])).map(r => r.json())
    const heads = [a, b].filter(r => r.head === r.version.versionId)
    expect(heads).toHaveLength(1)
    expect(b.diverged || a.diverged).toBe(true)
    expect((await getWorld()).head.versionId).toBe(heads[0].version.versionId)
  })

  it('a second first-upload (parent null when there is a head) is a branch', async () => {
    const v1 = (await upload(h.app, tokenA, worldId, Buffer.from('v1'), null)).json().version.versionId
    const other = (await upload(h.app, tokenB, worldId, Buffer.from('b'), null)).json()
    expect(other).toMatchObject({ head: v1, diverged: true })
  })

  it('refuses an unknown parent, a wrong sha256 and stores nothing for them', async () => {
    const unknown = await upload(h.app, tokenA, worldId, Buffer.from('x'), '01JB7ZZZZZZZZZZZZZZZZZZZZZ')
    expect(unknown.statusCode).toBe(400)
    expect(unknown.json().error).toBe('unknown_parent')
    const bad = await h.app.inject({
      method: 'POST',
      url: `/api/worlds/${worldId}/versions`,
      headers: { ...bearer(tokenA), 'content-type': 'application/octet-stream', 'x-sha256': sha256(Buffer.from('other')) },
      payload: Buffer.from('x'),
    })
    expect(bad.statusCode).toBe(400)
    expect(bad.json().error).toBe('sha256_mismatch')
    expect(h.store.objects.size).toBe(0)
    expect((await getWorld()).history).toHaveLength(0)
  })

  it('keeps startedAt/endedAt; endedAt absent is an autosave (null)', async () => {
    const res = (await upload(h.app, tokenA, worldId, Buffer.from('v1'), null, { 'x-started-at': '2026-10-21T19:00:00.000Z' })).json()
    expect(res.version.startedAt).toBe('2026-10-21T19:00:00.000Z')
    expect(res.version.endedAt).toBeNull()
  })

  it('the file comes back byte for byte with its sha256', async () => {
    const bytes = Buffer.from(Array.from({ length: 300_000 }, (_, i) => (i * 7) % 256))
    const v = (await upload(h.app, tokenA, worldId, bytes, null)).json().version
    const file = await h.app.inject({ method: 'GET', url: `/api/worlds/${worldId}/versions/${v.versionId}/file`, headers: bearer(tokenB) })
    expect(file.statusCode).toBe(200)
    expect(file.headers['x-sha256']).toBe(sha256(bytes))
    expect(file.rawPayload.equals(bytes)).toBe(true)
  })

  it('accepts a world file larger than the default 1 MiB body limit', async () => {
    const bytes = Buffer.alloc(3 * 1024 * 1024, 7)
    const res = await upload(h.app, tokenA, worldId, bytes, null)
    expect(res.statusCode).toBe(200)
    expect(res.json().version.size).toBe(bytes.length)
  })
})

describe('the host lock', () => {
  it('one holder; the other gets 409 with the holder; heartbeat renews; release frees', async () => {
    const taken = await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenA) })
    expect(taken.statusCode).toBe(200)
    expect(taken.json().lock).toMatchObject({ holder: A, nick: 'laranjo', since: '2026-10-21T21:00:00.000Z', expiresAt: '2026-10-21T21:05:00.000Z' })

    const busy = await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenB) })
    expect(busy.statusCode).toBe(409)
    expect(busy.json().lock).toMatchObject({ holder: A, nick: 'laranjo' })

    h.advance(60_000)
    const beat = await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock/heartbeat`, headers: bearer(tokenA) })
    expect(beat.statusCode).toBe(200)
    expect(beat.json().lock).toMatchObject({ since: '2026-10-21T21:00:00.000Z', expiresAt: '2026-10-21T21:06:00.000Z' })

    expect((await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock/heartbeat`, headers: bearer(tokenB) })).statusCode).toBe(409)
    expect((await h.app.inject({ method: 'DELETE', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenB) })).statusCode).toBe(409)

    expect((await getWorld(tokenB)).lock).toMatchObject({ holder: A })
    const released = await h.app.inject({ method: 'DELETE', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenA) })
    expect(released.statusCode).toBe(204)
    expect((await getWorld()).lock).toBeNull()
    expect((await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenB) })).json().lock.holder).toBe(B)
  })

  it('an expired lease is free (the PC died mid-session)', async () => {
    await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenA) })
    h.advance(5 * 60_000 - 1)
    expect((await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenB) })).statusCode).toBe(409)
    h.advance(1)
    expect((await getWorld()).lock).toBeNull()
    const taken = await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenB) })
    expect(taken.statusCode).toBe(200)
    expect(taken.json().lock).toMatchObject({ holder: B, since: '2026-10-21T21:05:00.000Z' })
    // the old holder's heartbeat now meets the new holder
    expect((await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock/heartbeat`, headers: bearer(tokenA) })).statusCode).toBe(409)
  })

  it('a heartbeat after the lease lapsed takes it again when nobody else did', async () => {
    await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenA) })
    h.advance(10 * 60_000)
    const beat = await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock/heartbeat`, headers: bearer(tokenA) })
    expect(beat.statusCode).toBe(200)
    expect(beat.json().lock).toMatchObject({ holder: A, since: '2026-10-21T21:10:00.000Z' })
  })

  it('releasing a free lock is fine', async () => {
    expect((await h.app.inject({ method: 'DELETE', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenA) })).statusCode).toBe(204)
  })

  it('presence: inGame follows the lease, appOnline the last request', async () => {
    await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenB) })
    let players = (await h.app.inject({ method: 'GET', url: '/api/presence', headers: bearer(tokenA) })).json().players
    expect(players).toEqual([
      { steamId: B, nick: 'rodrigao_br', appOnline: true, inGame: true },
      { steamId: A, nick: 'laranjo', appOnline: true, inGame: false },
    ])
    h.advance(3 * 60_000)
    players = (await h.app.inject({ method: 'GET', url: '/api/presence', headers: bearer(tokenA) })).json().players
    expect(players.find((p: { steamId: string }) => p.steamId === B)).toMatchObject({ appOnline: false, inGame: true })
    expect(players.find((p: { steamId: string }) => p.steamId === A)).toMatchObject({ appOnline: true })
    h.advance(3 * 60_000)
    players = (await h.app.inject({ method: 'GET', url: '/api/presence', headers: bearer(tokenA) })).json().players
    expect(players.find((p: { steamId: string }) => p.steamId === B)).toMatchObject({ inGame: false })
  })
})

describe('invites', () => {
  const C = '76561198000000002'

  it('ENSH-XXXX, 24 h, one use', async () => {
    const invite = (await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/invites`, headers: bearer(tokenA) })).json()
    expect(invite.code).toMatch(/^ENSH-[2-9A-HJKMNP-Z]{4}$/)
    expect(invite.expiresAt).toBe('2026-10-22T21:00:00.000Z')

    const tokenC = await device(h, C, 'terceiro')
    const tokenD = await device(h, '76561198000000003', 'quarto')
    const redeemed = await h.app.inject({ method: 'POST', url: '/api/invites/redeem', headers: bearer(tokenC), payload: { code: invite.code.toLowerCase() } })
    expect(redeemed.statusCode).toBe(200)
    expect(redeemed.json().members.map((m: { steamId: string }) => m.steamId)).toEqual([A, B, C])

    const again = await h.app.inject({ method: 'POST', url: '/api/invites/redeem', headers: bearer(tokenD), payload: { code: invite.code } })
    expect(again.statusCode).toBe(410)
    expect(again.json().error).toBe('invite_used')
    expect((await h.app.inject({ method: 'GET', url: `/api/worlds/${worldId}`, headers: bearer(tokenD) })).statusCode).toBe(404)
  })

  it('expires after 24 h', async () => {
    const invite = (await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/invites`, headers: bearer(tokenA) })).json()
    const tokenC = await device(h, C, 'terceiro')
    h.advance(24 * 60 * 60_000)
    const late = await h.app.inject({ method: 'POST', url: '/api/invites/redeem', headers: bearer(tokenC), payload: { code: invite.code } })
    expect(late.statusCode).toBe(410)
    expect(late.json().error).toBe('invite_expired')
    expect((await h.app.inject({ method: 'GET', url: '/api/worlds', headers: bearer(tokenC) })).json().worlds).toEqual([])
  })

  it('a member redeeming gets the world and does not spend the code', async () => {
    const invite = (await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/invites`, headers: bearer(tokenA) })).json()
    expect((await h.app.inject({ method: 'POST', url: '/api/invites/redeem', headers: bearer(tokenB), payload: { code: invite.code } })).statusCode).toBe(200)
    const tokenC = await device(h, C, 'terceiro')
    expect((await h.app.inject({ method: 'POST', url: '/api/invites/redeem', headers: bearer(tokenC), payload: { code: invite.code } })).statusCode).toBe(200)
  })

  it('unknown codes are 404 and too many of them are 429', async () => {
    const tokenC = await device(h, C, 'terceiro')
    for (let i = 0; i < 10; i++) {
      const res = await h.app.inject({ method: 'POST', url: '/api/invites/redeem', headers: bearer(tokenC), payload: { code: 'ENSH-0000' } })
      expect(res.statusCode).toBe(404)
    }
    const blocked = await h.app.inject({ method: 'POST', url: '/api/invites/redeem', headers: bearer(tokenC), payload: { code: 'ENSH-0000' } })
    expect(blocked.statusCode).toBe(429)
  })
})

describe('access', () => {
  it('no token is 401; a non-member does not see the world', async () => {
    const none = await h.app.inject({ method: 'GET', url: '/api/worlds' })
    expect(none.statusCode).toBe(401)
    expect(none.json()).toMatchObject({ error: 'unauthorized' })
    const stranger = await device(h, '76561198000000009', 'estranho')
    const res = await h.app.inject({ method: 'GET', url: `/api/worlds/${worldId}`, headers: bearer(stranger) })
    expect(res.statusCode).toBe(404)
    expect((await upload(h.app, stranger, worldId, Buffer.from('x'), null)).statusCode).toBe(404)
    expect((await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(stranger) })).statusCode).toBe(404)
  })

  it('avatar: PNG only, readable by co-members, not by strangers', async () => {
    const png = Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), Buffer.from('rest of png')])
    expect((await h.app.inject({ method: 'PUT', url: '/api/me/avatar', headers: { ...bearer(tokenA), 'content-type': 'image/png' }, payload: Buffer.from('GIF89a') })).statusCode).toBe(400)
    expect((await h.app.inject({ method: 'PUT', url: '/api/me/avatar', headers: { ...bearer(tokenA), 'content-type': 'image/png' }, payload: png })).statusCode).toBe(204)
    expect(h.store.objects.get(`avatars/${A}.png`)?.body.equals(png)).toBe(true)
    const got = await h.app.inject({ method: 'GET', url: `/api/players/${A}/avatar`, headers: bearer(tokenB) })
    expect(got.statusCode).toBe(200)
    expect(got.rawPayload.equals(png)).toBe(true)
    const stranger = await device(h, '76561198000000009', 'estranho')
    expect((await h.app.inject({ method: 'GET', url: `/api/players/${A}/avatar`, headers: bearer(stranger) })).statusCode).toBe(404)
  })
})

describe('deleting a world', () => {
  it('any member deletes it for everyone, with its files', async () => {
    const v1 = (await upload(h.app, tokenA, worldId, Buffer.from('v1'), null)).json().version.versionId
    const res = await h.app.inject({ method: 'DELETE', url: `/api/worlds/${worldId}`, headers: bearer(tokenB) })
    expect(res.statusCode).toBe(204)
    expect(h.store.objects.has(`worlds/${worldId}/${v1}`)).toBe(false)
    for (const token of [tokenA, tokenB]) {
      expect((await h.app.inject({ method: 'GET', url: `/api/worlds/${worldId}`, headers: bearer(token) })).statusCode).toBe(404)
      expect((await h.app.inject({ method: 'GET', url: '/api/worlds', headers: bearer(token) })).json().worlds).toEqual([])
    }
  })

  it('a non-member gets 404; someone else hosting is 409; the host may delete', async () => {
    const tokenC = await device(h, '76561198000000002', 'curioso')
    expect((await h.app.inject({ method: 'DELETE', url: `/api/worlds/${worldId}`, headers: bearer(tokenC) })).statusCode).toBe(404)
    await h.app.inject({ method: 'POST', url: `/api/worlds/${worldId}/lock`, headers: bearer(tokenA) })
    const busy = await h.app.inject({ method: 'DELETE', url: `/api/worlds/${worldId}`, headers: bearer(tokenB) })
    expect(busy.statusCode).toBe(409)
    expect(busy.json().error).toBe('lock_held')
    expect((await h.app.inject({ method: 'DELETE', url: `/api/worlds/${worldId}`, headers: bearer(tokenA) })).statusCode).toBe(204)
  })
})
