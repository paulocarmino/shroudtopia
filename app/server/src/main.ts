// @shroudtopia/server: the sync server of the Shroudtopia app (app/CONTRACT.md). `pnpm dev` starts it on
// http://localhost:8800 with SQLite and the blobs in app/server/data.
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { buildServer } from './app.ts'
import { loadConfig } from './config.ts'
import { loadLanding } from './landing.ts'
import { FolderObjectStore, S3ObjectStore } from './object-store.ts'
import { STEAM_OPENID_ENDPOINT } from './steam.ts'
import { Sync } from './sync.ts'

const config = loadConfig()
mkdirSync(config.dataDir, { recursive: true })
const sync = new Sync(join(config.dataDir, 'shroudtopia.sqlite'))
const store = config.s3 ? new S3ObjectStore(config.s3) : new FolderObjectStore(join(config.dataDir, 'blobs'))
const app = await buildServer({
  sync,
  store,
  steam: { origin: config.publicOrigin, endpoint: STEAM_OPENID_ENDPOINT },
  ...(config.landing ? { landing: { host: config.landing.host, files: loadLanding(config.landing.dir) } } : {}),
  build: config.build,
  log: config.log,
  trustProxy: config.trustProxy,
})

let stopping = false
for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.on(signal, () => {
    if (stopping) return
    stopping = true
    void app.close().finally(() => {
      sync.close()
      process.exit(0)
    })
  })
}

await app.listen({ port: config.port, host: config.host })
console.log(`Shroudtopia sync ${config.build} on ${config.host}:${config.port} as ${config.publicOrigin}, data in ${config.dataDir}, blobs in ${config.s3 ? `s3 ${config.s3.bucket}` : 'the data folder'}${config.landing ? `, landing page for ${config.landing.host}` : ''}`)
