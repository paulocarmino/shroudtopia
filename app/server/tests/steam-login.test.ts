// The Steam login (CONTRACT.md, "Steam login") against a fake Steam OpenID provider on a real local port: the server
// really POSTs check_authentication to it, and it says is_valid:true only for assertions it issued, unchanged.
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { bearer, callbackUrl, harness, NS, ORIGIN, steamLogin, type Harness } from './helpers.ts'

const A = '76561198065863968'
const B = '76561198000000001'

let h: Harness
beforeEach(async () => {
  h = await harness()
})
afterEach(async () => {
  await h.close()
})

const start = async (nick = 'laranjo') => (await h.app.inject({ method: 'POST', url: '/api/auth/steam/start', payload: { nick } })).json().sessionId as string
const poll = (sessionId: string) => h.app.inject({ method: 'GET', url: `/api/auth/steam/poll/${sessionId}` })
const callback = (sessionId: string, query: Record<string, string>) => h.app.inject({ method: 'GET', url: callbackUrl(sessionId, query) })
const returnTo = (sessionId: string) => `${ORIGIN}/auth/steam/callback/${sessionId}`
const playerCount = () => (h.sync.db.prepare('SELECT COUNT(*) AS n FROM devices').get() as { n: number }).n

describe('redirect', () => {
  it('sends the browser to Steam with our return_to and realm', async () => {
    const sessionId = await start()
    const res = await h.app.inject({ method: 'GET', url: `/auth/steam/redirect/${sessionId}` })
    expect(res.statusCode).toBe(302)
    const location = new URL(res.headers.location as string)
    expect(`${location.origin}${location.pathname}`).toBe(h.steam.endpoint)
    expect(Object.fromEntries(location.searchParams)).toEqual({
      'openid.ns': NS,
      'openid.mode': 'checkid_setup',
      'openid.return_to': returnTo(sessionId),
      'openid.realm': `${ORIGIN}/`,
      'openid.identity': 'http://specs.openid.net/auth/2.0/identifier_select',
      'openid.claimed_id': 'http://specs.openid.net/auth/2.0/identifier_select',
    })
  })

  it('an unknown or expired session does not go to Steam', async () => {
    expect((await h.app.inject({ method: 'GET', url: '/auth/steam/redirect/01JB8S0000000000000000000S' })).statusCode).toBe(410)
    const sessionId = await start()
    h.advance(10 * 60_000)
    const res = await h.app.inject({ method: 'GET', url: `/auth/steam/redirect/${sessionId}` })
    expect(res.statusCode).toBe(410)
    expect(res.headers['content-type']).toContain('text/html')
  })
})

describe('callback: what Steam vouches for, and nothing else', () => {
  it('a valid assertion: done page in pt-BR, Steam was asked once with the fields as received, token for that steamId', async () => {
    const sessionId = await start()
    const query = h.steam.assert(A, returnTo(sessionId), h.clock.now)
    const res = await callback(sessionId, query)
    expect(res.statusCode).toBe(200)
    expect(res.headers['content-type']).toContain('text/html')
    expect(res.body).toContain('Pronto, pode voltar para o app')

    expect(h.steam.checks).toHaveLength(1)
    const sent = Object.fromEntries(h.steam.checks[0]!)
    expect(sent).toEqual({ ...query, 'openid.mode': 'check_authentication' })

    const done = await poll(sessionId)
    expect(done.statusCode).toBe(200)
    expect(done.json().player).toEqual({ steamId: A, nick: 'laranjo' })
    const me = await h.app.inject({ method: 'GET', url: '/api/devices', headers: bearer(done.json().token) })
    expect(me.statusCode).toBe(200)
  })

  it('is_valid:false from Steam is refused', async () => {
    h.steam.refuseAll = true
    const sessionId = await start()
    const res = await callback(sessionId, h.steam.assert(A, returnTo(sessionId), h.clock.now))
    expect(res.statusCode).toBe(400)
    expect(res.body).toContain('Não deu certo')
    expect(h.steam.checks).toHaveLength(1)
    expect((await poll(sessionId)).statusCode).toBe(202)
    expect(playerCount()).toBe(0)
  })

  it('a tampered claimed_id (someone else\'s steamId) is refused: Steam does not confirm it', async () => {
    const sessionId = await start()
    const query = h.steam.assert(A, returnTo(sessionId), h.clock.now)
    const other = `https://steamcommunity.com/openid/id/${B}`
    const res = await callback(sessionId, { ...query, 'openid.claimed_id': other, 'openid.identity': other })
    expect(res.statusCode).toBe(400)
    expect(h.steam.checks).toHaveLength(1)
    expect((await poll(sessionId)).statusCode).toBe(202)
    expect(playerCount()).toBe(0)
  })

  it('claimed_id that is not a Steam identity, or differs from identity, is refused without asking Steam', async () => {
    const sessionId = await start()
    const query = h.steam.assert(A, returnTo(sessionId), h.clock.now)
    expect((await callback(sessionId, { ...query, 'openid.claimed_id': `https://evil.example/openid/id/${A}`, 'openid.identity': `https://evil.example/openid/id/${A}` })).statusCode).toBe(400)
    expect((await callback(sessionId, { ...query, 'openid.identity': `https://steamcommunity.com/openid/id/${B}` })).statusCode).toBe(400)
    expect(h.steam.checks).toHaveLength(0)
  })

  it('a wrong return_to is refused without asking Steam: another session\'s, or another host', async () => {
    const mine = await start()
    const theirs = await start()
    // a genuine assertion for another session, brought to mine
    const res = await callback(mine, h.steam.assert(A, returnTo(theirs), h.clock.now))
    expect(res.statusCode).toBe(400)
    const elsewhere = await callback(mine, h.steam.assert(A, `https://evil.example/auth/steam/callback/${mine}`, h.clock.now))
    expect(elsewhere.statusCode).toBe(400)
    expect(h.steam.checks).toHaveLength(0)
    expect((await poll(mine)).statusCode).toBe(202)
  })

  it('an assertion from another provider than Steam\'s endpoint is refused', async () => {
    const sessionId = await start()
    const query = h.steam.assert(A, returnTo(sessionId), h.clock.now)
    expect((await callback(sessionId, { ...query, 'openid.op_endpoint': 'https://evil.example/openid/login' })).statusCode).toBe(400)
    expect(h.steam.checks).toHaveLength(0)
  })

  it('a replayed nonce is refused even when Steam would confirm it', async () => {
    const first = await start()
    const nonce = `${new Date(h.clock.now).toISOString().slice(0, 19)}Zreplayme`
    expect((await callback(first, h.steam.assert(A, returnTo(first), h.clock.now, nonce))).statusCode).toBe(200)
    // a second session, an assertion Steam signed for it, but with the same nonce
    const second = await start()
    const res = await callback(second, h.steam.assert(A, returnTo(second), h.clock.now, nonce))
    expect(res.statusCode).toBe(400)
    expect((await poll(second)).statusCode).toBe(202)
  })

  it('the same callback twice: the second one finds the session done', async () => {
    const sessionId = await start()
    const query = h.steam.assert(A, returnTo(sessionId), h.clock.now)
    expect((await callback(sessionId, query)).statusCode).toBe(200)
    expect((await callback(sessionId, query)).statusCode).toBe(410)
    expect(h.steam.checks).toHaveLength(1)
  })

  it('a stale nonce (an old assertion) is refused', async () => {
    const sessionId = await start()
    const query = h.steam.assert(A, returnTo(sessionId), h.clock.now - 11 * 60_000)
    expect((await callback(sessionId, query)).statusCode).toBe(400)
  })

  it('unsigned fields and a cancel are refused', async () => {
    const sessionId = await start()
    const query = h.steam.assert(A, returnTo(sessionId), h.clock.now)
    expect((await callback(sessionId, { ...query, 'openid.signed': 'signed,op_endpoint,identity,return_to,response_nonce,assoc_handle' })).statusCode).toBe(400)
    const cancelled = await callback(sessionId, { 'openid.ns': NS, 'openid.mode': 'cancel' })
    expect(cancelled.statusCode).toBe(400)
    expect(cancelled.body).toContain('cancelado')
    expect(h.steam.checks).toHaveLength(0)
  })

  it('Steam unreachable is a failure, not a login', async () => {
    const sessionId = await start()
    const query = h.steam.assert(A, returnTo(sessionId), h.clock.now)
    await h.steam.stop()
    const res = await callback(sessionId, query)
    expect(res.statusCode).toBe(502)
    await h.steam.start()
  })
})

describe('poll', () => {
  it('202 pending, then the token exactly once, then 410', async () => {
    const sessionId = await start()
    const pending = await poll(sessionId)
    expect(pending.statusCode).toBe(202)
    expect(pending.json()).toEqual({ status: 'pending' })
    await callback(sessionId, h.steam.assert(A, returnTo(sessionId), h.clock.now))
    const done = await poll(sessionId)
    expect(done.statusCode).toBe(200)
    expect(done.json().token).toMatch(/^dev_/)
    const again = await poll(sessionId)
    expect(again.statusCode).toBe(410)
    expect(playerCount()).toBe(1)
  })

  it('expires after 10 minutes, verified or not', async () => {
    const waiting = await start()
    h.advance(9 * 60_000)
    expect((await poll(waiting)).statusCode).toBe(202)
    h.advance(60_000)
    const late = await poll(waiting)
    expect(late.statusCode).toBe(410)
    expect(late.json().error).toBe('expired')

    const verified = await start()
    await callback(verified, h.steam.assert(A, returnTo(verified), h.clock.now))
    h.advance(10 * 60_000)
    expect((await poll(verified)).statusCode).toBe(410)
    expect(playerCount()).toBe(0)
    // and a callback after expiry does not complete it
    const old = await start()
    h.advance(10 * 60_000)
    expect((await callback(old, h.steam.assert(A, returnTo(old), h.clock.now))).statusCode).toBe(410)
  })

  it('an unknown session is 404', async () => {
    expect((await poll('01JB8S0000000000000000000S')).statusCode).toBe(404)
  })
})

describe('devices, revoke, logout, nick', () => {
  it('lists the player\'s devices; revoking one kills its token; current is logout; others\' devices are 404', async () => {
    const pc = await steamLogin(h, A, 'laranjo')
    h.advance(1000)
    const laptop = await steamLogin(h, A, 'laranjo')
    const friend = await steamLogin(h, B, 'rodrigao_br')

    const list = (await h.app.inject({ method: 'GET', url: '/api/devices', headers: bearer(pc) })).json().devices
    expect(list).toHaveLength(2)
    expect(list.map((d: { current: boolean }) => d.current)).toEqual([true, false])
    const laptopId = list[1].deviceId

    // B cannot revoke A's device
    expect((await h.app.inject({ method: 'DELETE', url: `/api/devices/${laptopId}`, headers: bearer(friend) })).statusCode).toBe(404)
    expect((await h.app.inject({ method: 'GET', url: '/api/worlds', headers: bearer(laptop) })).statusCode).toBe(200)

    expect((await h.app.inject({ method: 'DELETE', url: `/api/devices/${laptopId}`, headers: bearer(pc) })).statusCode).toBe(204)
    expect((await h.app.inject({ method: 'GET', url: '/api/worlds', headers: bearer(laptop) })).statusCode).toBe(401)

    expect((await h.app.inject({ method: 'DELETE', url: '/api/devices/current', headers: bearer(pc) })).statusCode).toBe(204)
    expect((await h.app.inject({ method: 'GET', url: '/api/devices', headers: bearer(pc) })).statusCode).toBe(401)
    expect((await h.app.inject({ method: 'GET', url: '/api/devices', headers: bearer(friend) })).statusCode).toBe(200)
  })

  it('PATCH /api/me changes the nick the others see', async () => {
    const a = await steamLogin(h, A, 'laranjo')
    const res = await h.app.inject({ method: 'PATCH', url: '/api/me', headers: bearer(a), payload: { nick: 'laranjo 2' } })
    expect(res.statusCode).toBe(200)
    expect(res.json()).toEqual({ player: { steamId: A, nick: 'laranjo 2' } })
    const world = (await h.app.inject({ method: 'POST', url: '/api/worlds', headers: bearer(a), payload: { name: 'x' } })).json()
    expect(world.members).toEqual([{ steamId: A, nick: 'laranjo 2' }])
  })

  it('the old POST /api/devices is gone', async () => {
    const res = await h.app.inject({ method: 'POST', url: '/api/devices', payload: { steamId: A, nick: 'x' } })
    expect(res.statusCode).toBe(404)
    expect(playerCount()).toBe(0)
  })
})
