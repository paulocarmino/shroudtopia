// The two hostnames: the landing host serves app/landing (the real files) and nothing else; the API host never
// serves the landing page.
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { loadLanding } from '../src/landing.ts'
import { bearer, device, harness, type Harness } from './helpers.ts'

const DIR = resolve(import.meta.dirname, '../../landing')
const LANDING = 'shroudtopia.paulocarmino.com'
const API = 'shroudtopia-api.paulocarmino.com'

let h: Harness
let token: string
beforeAll(async () => {
  h = await harness({ landing: { host: LANDING, files: loadLanding(DIR) } })
  token = await device(h, '76561198065863968', 'laranjo')
})
afterAll(async () => {
  await h.close()
})

const get = (host: string, url: string, headers: Record<string, string> = {}) => h.app.inject({ method: 'GET', url, headers: { host, ...headers } })

describe('landing host', () => {
  it('/ is index.html, cached briefly', async () => {
    const res = await get(LANDING, '/')
    expect(res.statusCode).toBe(200)
    expect(res.headers['content-type']).toBe('text/html; charset=utf-8')
    expect(res.headers['cache-control']).toBe('public, max-age=300')
    expect(res.rawPayload.equals(readFileSync(resolve(DIR, 'index.html')))).toBe(true)
    expect((await get(`${LANDING.toUpperCase()}:443`, '/index.html')).statusCode).toBe(200)
  })

  it('every asset the page names is served, cached long, with its type', async () => {
    const html = readFileSync(resolve(DIR, 'index.html'), 'utf8')
    const assets = [...new Set([...html.matchAll(/(?:src|href)="(assets\/[^"]+)"/g)].map(m => m[1]!))]
    expect(assets.length).toBeGreaterThan(2)
    for (const asset of assets) {
      const res = await get(LANDING, `/${asset}`)
      expect(res.statusCode, asset).toBe(200)
      expect(res.headers['cache-control']).toBe('public, max-age=604800')
      expect(res.rawPayload.equals(readFileSync(resolve(DIR, asset)))).toBe(true)
    }
    expect((await get(LANDING, '/assets/shroudtopia-logo.svg')).headers['content-type']).toBe('image/svg+xml')
    expect((await get(LANDING, '/assets/mist-valley.jpg')).headers['content-type']).toBe('image/jpeg')
  })

  it('exposes no API route, whatever the token or method', async () => {
    for (const url of ['/api/worlds', '/api/presence', '/healthz', '/auth/steam/redirect/01JB8S0000000000000000000S', '/nope']) {
      const res = await get(LANDING, url, bearer(token))
      expect(res.statusCode, url).toBe(404)
      expect(res.body).toBe('Not found\n')
    }
    const post = await h.app.inject({ method: 'POST', url: '/api/auth/steam/start', headers: { host: LANDING }, payload: { nick: 'x' } })
    expect(post.statusCode).toBe(404)
    expect(h.sync.db.prepare('SELECT COUNT(*) AS n FROM login_sessions').get()).toEqual({ n: 0 })
  })

  it('X-Forwarded-Host cannot move a request from one host to the other', async () => {
    expect((await get(LANDING, '/api/worlds', { ...bearer(token), 'x-forwarded-host': API })).statusCode).toBe(404)
    expect((await get(API, '/', { 'x-forwarded-host': LANDING })).headers['content-type']).toContain('application/json')
  })
})

describe('API host', () => {
  it('does not serve the landing page', async () => {
    for (const url of ['/', '/index.html', '/assets/shroudtopia-logo.svg']) {
      const res = await get(API, url)
      expect(res.statusCode, url).toBe(404)
      expect(res.json().error).toBe('not_found')
    }
  })

  it('serves the API', async () => {
    expect((await get(API, '/healthz')).statusCode).toBe(200)
    expect((await get(API, '/api/worlds', bearer(token))).statusCode).toBe(200)
  })
})
