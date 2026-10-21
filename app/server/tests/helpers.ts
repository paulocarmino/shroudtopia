// A real server (real SQLite in a temp folder, real Fastify, real routes) with a memory blob store and a clock the
// test moves.
import { createHash } from 'node:crypto'
import { mkdtempSync, rmSync } from 'node:fs'
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

export interface Harness {
  app: FastifyInstance
  sync: Sync
  store: MemoryStore
  clock: { now: number }
  advance(ms: number): void
  close(): Promise<void>
}

export async function harness(): Promise<Harness> {
  const dir = mkdtempSync(join(tmpdir(), 'shroudtopia-'))
  const clock = { now: Date.parse('2026-10-21T21:00:00.000Z') }
  const sync = new Sync(join(dir, 'test.sqlite'), { clock: () => clock.now })
  const store = new MemoryStore()
  const app = await buildServer({ sync, store, build: 'test' })
  return {
    app,
    sync,
    store,
    clock,
    advance: ms => {
      clock.now += ms
    },
    close: async () => {
      await app.close()
      sync.close()
      rmSync(dir, { recursive: true, force: true })
    },
  }
}

export const sha256 = (bytes: Buffer) => createHash('sha256').update(bytes).digest('hex')

export async function device(app: FastifyInstance, steamId: string, nick: string): Promise<string> {
  const res = await app.inject({ method: 'POST', url: '/api/devices', payload: { steamId, nick } })
  if (res.statusCode !== 200) throw new Error(`device: ${res.statusCode} ${res.body}`)
  return res.json().token
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
