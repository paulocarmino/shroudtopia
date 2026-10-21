// The server's settings. Everything comes from the environment; the defaults are the development ones. Nothing else
// in the server reads process.env.
import { isAbsolute, resolve } from 'node:path'
import type { S3Settings } from './object-store.ts'

/** app/server */
export const PACKAGE_DIR = resolve(import.meta.dirname, '..')

export interface Config {
  port: number
  host: string
  /** SQLite (metadata) lives here; without S3 the blobs do too (development only) */
  dataDir: string
  /** scheme://host[:port], no trailing slash: the Steam login's return_to and realm are built from it */
  publicOrigin: string
  /** how many proxies stand in front (1 behind Envoy) */
  trustProxy: number
  /** the commit the image was built from, or "dev" */
  build: string
  /** the blob store (Garage in the cluster); null: files under dataDir/blobs */
  s3: S3Settings | null
  log: boolean
}

type Env = Record<string, string | undefined>

function integer(env: Env, name: string, fallback: number, min: number, max: number): number {
  const raw = env[name]
  if (raw === undefined || raw === '') return fallback
  const value = Number(raw)
  if (!Number.isInteger(value) || value < min || value > max) throw new Error(`${name} must be an integer from ${min} to ${max}`)
  return value
}

/** The five SHROUDTOPIA_S3_* variables, all or none. */
function s3Settings(env: Env): S3Settings | null {
  const names = {
    endpoint: 'SHROUDTOPIA_S3_ENDPOINT',
    region: 'SHROUDTOPIA_S3_REGION',
    bucket: 'SHROUDTOPIA_S3_BUCKET',
    accessKeyId: 'SHROUDTOPIA_S3_ACCESS_KEY_ID',
    secretAccessKey: 'SHROUDTOPIA_S3_SECRET_ACCESS_KEY',
  } as const
  const given = Object.values(names).filter(name => (env[name] ?? '') !== '')
  if (given.length === 0) return null
  const missing = Object.values(names).filter(name => (env[name] ?? '') === '')
  if (missing.length) throw new Error(`the blob store needs all five SHROUDTOPIA_S3_* variables or none; missing: ${missing.join(', ')}`)
  let url: URL
  try {
    url = new URL(env[names.endpoint]!)
  } catch {
    throw new Error('SHROUDTOPIA_S3_ENDPOINT must be a URL')
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') throw new Error('SHROUDTOPIA_S3_ENDPOINT must be http or https')
  return {
    endpoint: url.origin + url.pathname.replace(/\/$/, ''),
    region: env[names.region]!,
    bucket: env[names.bucket]!,
    accessKeyId: env[names.accessKeyId]!,
    secretAccessKey: env[names.secretAccessKey]!,
  }
}

export function loadConfig(env: Env = process.env): Config {
  const dataDir = env.SHROUDTOPIA_DATA_DIR || 'data'
  const port = integer(env, 'PORT', 8800, 0, 65535)
  let publicOrigin: string
  try {
    const url = new URL(env.SHROUDTOPIA_PUBLIC_ORIGIN || `http://localhost:${port}`)
    if (url.protocol !== 'http:' && url.protocol !== 'https:') throw new Error()
    publicOrigin = url.origin
  } catch {
    throw new Error('SHROUDTOPIA_PUBLIC_ORIGIN must be an http(s) URL')
  }
  return {
    port,
    publicOrigin,
    host: env.HOST || '127.0.0.1',
    dataDir: isAbsolute(dataDir) ? dataDir : resolve(PACKAGE_DIR, dataDir),
    trustProxy: integer(env, 'SHROUDTOPIA_TRUST_PROXY', 0, 0, 8),
    build: env.SHROUDTOPIA_BUILD_COMMIT || 'dev',
    s3: s3Settings(env),
    log: env.SHROUDTOPIA_LOG !== '0',
  }
}
