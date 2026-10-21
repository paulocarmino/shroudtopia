// A real server (real SQLite in a temp folder, real Fastify, real routes) with a memory blob store, a clock the test
// moves, and a fake Steam OpenID provider listening on a real local port.
import { createHash, randomBytes } from 'node:crypto'
import { mkdtempSync, rmSync } from 'node:fs'
import { createServer, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Readable } from 'node:stream'
import type { FastifyInstance } from 'fastify'
import { buildServer } from '../src/app.ts'
import type { ObjectStore } from '../src/object-store.ts'
import { Sync } from '../src/sync.ts'

export class MemoryStore implements ObjectStore {
  readonly objects = new Map<string, { body: Buffer; contentType: string }>()
  async put(key: string, body: Buffer, contentType: string) {
    this.objects.set(key, { body: Buffer.from(body), contentType })
  }
  async get(key: string) {
    const found = this.objects.get(key)
    return found ? { body: Readable.from([found.body]), size: found.body.length } : null
  }
  async delete(key: string) {
    this.objects.delete(key)
  }
}

export const NS = 'http://specs.openid.net/auth/2.0'
const SIGNED = 'signed,op_endpoint,claimed_id,identity,return_to,response_nonce,assoc_handle'

/**
 * Steam, as far as the server can tell: it "signs" assertions by remembering them under their sig, and answers
 * check_authentication with is_valid:true only for an assertion it issued, every signed field unchanged.
 */
export class FakeSteam {
  readonly server: Server
  endpoint = ''
  /** the bodies of every check_authentication received */
  readonly checks: URLSearchParams[] = []
  /** true: answer is_valid:false whatever is asked */
  refuseAll = false
  private readonly issued = new Map<string, Record<string, string>>()

  constructor() {
    this.server = createServer((request, response) => {
      let body = ''
      request.on('data', chunk => (body += chunk))
      request.on('end', () => {
        const form = new URLSearchParams(body)
        this.checks.push(form)
        const original = this.issued.get(form.get('openid.sig') ?? '')
        const valid =
          !this.refuseAll &&
          request.method === 'POST' &&
          form.get('openid.mode') === 'check_authentication' &&
          original !== undefined &&
          SIGNED.split(',').every(field => field === 'signed' || form.get(`openid.${field}`) === original[`openid.${field}`]) &&
          form.get('openid.signed') === original['openid.signed']
        response.setHeader('content-type', 'text/plain')
        response.end(`ns:${NS}\nis_valid:${valid}\n`)
      })
    })
  }

  async start(): Promise<void> {
    await new Promise<void>(resolve => this.server.listen(0, '127.0.0.1', resolve))
    this.endpoint = `http://127.0.0.1:${(this.server.address() as AddressInfo).port}/openid/login`
  }

  async stop(): Promise<void> {
    await new Promise<void>(resolve => this.server.close(() => resolve()))
  }

  /** The query Steam would send the browser back with, for this steamId and return_to. */
  assert(steamId: string, returnTo: string, now: number, nonce?: string): Record<string, string> {
    const id = `https://steamcommunity.com/openid/id/${steamId}`
    const sig = randomBytes(20).toString('base64')
    const params = {
      'openid.ns': NS,
      'openid.mode': 'id_res',
      'openid.op_endpoint': this.endpoint,
      'openid.claimed_id': id,
      'openid.identity': id,
      'openid.return_to': returnTo,
      'openid.response_nonce': nonce ?? `${new Date(now).toISOString().slice(0, 19)}Z${randomBytes(6).toString('hex')}`,
      'openid.assoc_handle': '1234567890',
      'openid.signed': SIGNED,
      'openid.sig': sig,
    }
    this.issued.set(sig, params)
    return params
  }
}

export const ORIGIN = 'https://shroudtopia.test'

export interface Harness {
  app: FastifyInstance
  sync: Sync
  store: MemoryStore
  steam: FakeSteam
  clock: { now: number }
  advance(ms: number): void
  close(): Promise<void>
}

export async function harness(): Promise<Harness> {
  const dir = mkdtempSync(join(tmpdir(), 'shroudtopia-'))
  const clock = { now: Date.parse('2026-10-21T21:00:00.000Z') }
  const sync = new Sync(join(dir, 'test.sqlite'), { clock: () => clock.now })
  const store = new MemoryStore()
  const steam = new FakeSteam()
  await steam.start()
  const app = await buildServer({ sync, store, steam: { origin: ORIGIN, endpoint: steam.endpoint }, build: 'test' })
  return {
    app,
    sync,
    store,
    steam,
    clock,
    advance: ms => {
      clock.now += ms
    },
    close: async () => {
      await app.close()
      await steam.stop()
      sync.close()
      rmSync(dir, { recursive: true, force: true })
    },
  }
}

export const sha256 = (bytes: Buffer) => createHash('sha256').update(bytes).digest('hex')

/** The callback URL with Steam's query, as the browser would request it. */
export const callbackUrl = (sessionId: string, query: Record<string, string>) =>
  `/auth/steam/callback/${sessionId}?${new URLSearchParams(query).toString()}`

/** The whole Steam login, as the app and the browser do it; returns the token. */
export async function steamLogin(h: Harness, steamId: string, nick: string): Promise<string> {
  const start = await h.app.inject({ method: 'POST', url: '/api/auth/steam/start', payload: { nick } })
  const { sessionId } = start.json()
  const query = h.steam.assert(steamId, `${ORIGIN}/auth/steam/callback/${sessionId}`, h.clock.now)
  const callback = await h.app.inject({ method: 'GET', url: callbackUrl(sessionId, query) })
  if (callback.statusCode !== 200) throw new Error(`callback: ${callback.statusCode} ${callback.body}`)
  const poll = await h.app.inject({ method: 'GET', url: `/api/auth/steam/poll/${sessionId}` })
  if (poll.statusCode !== 200) throw new Error(`poll: ${poll.statusCode} ${poll.body}`)
  return poll.json().token
}

/** A device token for a steamId, made directly (the rules tests do not need to log in through Steam each time). */
export async function device(h: Harness, steamId: string, nick: string): Promise<string> {
  return h.sync.issueDevice(steamId, nick).token
}

export const bearer = (token: string) => ({ authorization: `Bearer ${token}` })

export async function upload(
  app: FastifyInstance,
  token: string,
  worldId: string,
  bytes: Buffer,
  parent: string | null,
  extra: Record<string, string> = {},
) {
  return app.inject({
    method: 'POST',
    url: `/api/worlds/${worldId}/versions`,
    headers: {
      ...bearer(token),
      'content-type': 'application/octet-stream',
      'x-sha256': sha256(bytes),
      ...(parent ? { 'x-parent': parent } : {}),
      ...extra,
    },
    payload: bytes,
  })
}
