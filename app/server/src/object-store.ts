// Where the bytes go: world files and avatars. In the cluster an S3 bucket in Garage, reached by the server alone with
// path-style addresses; in development a folder; in the tests memory (tests/memory-store.ts). No address of the store
// is ever given to the app: every byte passes through the routes.
import { createReadStream } from 'node:fs'
import { mkdir, rm, stat, writeFile } from 'node:fs/promises'
import { dirname, join, resolve, sep } from 'node:path'
import type { Readable } from 'node:stream'
import { DeleteObjectCommand, GetObjectCommand, PutObjectCommand, S3Client } from '@aws-sdk/client-s3'

export interface S3Settings {
  endpoint: string
  region: string
  bucket: string
  accessKeyId: string
  secretAccessKey: string
}

export interface ObjectStore {
  put(key: string, body: Buffer, contentType: string): Promise<void>
  /** null when there is no such object */
  get(key: string): Promise<{ body: Readable; size: number } | null>
  /** idempotent: an object that is not there is not an error */
  delete(key: string): Promise<void>
}

const missing = (error: unknown) => {
  const named = error as { name?: string; $metadata?: { httpStatusCode?: number } }
  return named?.name === 'NoSuchKey' || named?.name === 'NotFound' || named?.$metadata?.httpStatusCode === 404
}

/** S3 as Garage speaks it: path-style, its own region name, checksums only where S3 asks for them. */
export class S3ObjectStore implements ObjectStore {
  private readonly client: S3Client
  private readonly bucket: string

  constructor(settings: S3Settings) {
    this.bucket = settings.bucket
    this.client = new S3Client({
      endpoint: settings.endpoint,
      region: settings.region,
      forcePathStyle: true,
      credentials: { accessKeyId: settings.accessKeyId, secretAccessKey: settings.secretAccessKey },
      requestChecksumCalculation: 'WHEN_REQUIRED',
      responseChecksumValidation: 'WHEN_REQUIRED',
    })
  }

  async put(key: string, body: Buffer, contentType: string): Promise<void> {
    await this.client.send(new PutObjectCommand({ Bucket: this.bucket, Key: key, Body: body, ContentLength: body.length, ContentType: contentType }))
  }

  async get(key: string) {
    try {
      const found = await this.client.send(new GetObjectCommand({ Bucket: this.bucket, Key: key }))
      if (!found.Body) return null
      return { body: found.Body as Readable, size: Number(found.ContentLength ?? 0) }
    } catch (error) {
      if (missing(error)) return null
      throw error
    }
  }

  async delete(key: string): Promise<void> {
    await this.client.send(new DeleteObjectCommand({ Bucket: this.bucket, Key: key }))
  }
}

/** Development without Garage: one file per key under a folder. */
export class FolderObjectStore implements ObjectStore {
  private readonly root: string

  constructor(root: string) {
    this.root = resolve(root)
  }

  private path(key: string): string {
    const path = resolve(join(this.root, key))
    if (!path.startsWith(this.root + sep)) throw new Error(`bad key ${key}`)
    return path
  }

  async put(key: string, body: Buffer): Promise<void> {
    const path = this.path(key)
    await mkdir(dirname(path), { recursive: true })
    await writeFile(path, body)
  }

  async get(key: string) {
    const path = this.path(key)
    try {
      const info = await stat(path)
      return { body: createReadStream(path), size: info.size }
    } catch {
      return null
    }
  }

  async delete(key: string): Promise<void> {
    await rm(this.path(key), { force: true })
  }
}

export const worldFileKey = (worldId: string, versionId: string) => `worlds/${worldId}/${versionId}`
export const avatarKey = (steamId: string) => `avatars/${steamId}.png`
