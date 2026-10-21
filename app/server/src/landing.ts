// The landing page (app/landing: index.html and assets/, plain files) for its own hostname. gaia-iac has no static
// site pattern, so this server answers that one Host from memory and nothing else there: no /api route is reachable
// through it, and the API's hostname never serves these files.
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { extname, join, relative, sep } from 'node:path'

const TYPES: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.webp': 'image/webp',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
  '.txt': 'text/plain; charset=utf-8',
}

/** index.html is looked at again soon; assets (not content-hashed) for a week */
const PAGE_CACHE = 'public, max-age=300'
const ASSET_CACHE = 'public, max-age=604800'

export interface LandingFile {
  body: Buffer
  type: string
  cache: string
}

/** Every file under `dir`, by URL path ("/" is index.html). Read once, at boot. */
export function loadLanding(dir: string): Map<string, LandingFile> {
  const files = new Map<string, LandingFile>()
  const walk = (folder: string) => {
    for (const name of readdirSync(folder)) {
      const path = join(folder, name)
      if (statSync(path).isDirectory()) {
        walk(path)
        continue
      }
      const type = TYPES[extname(name).toLowerCase()]
      if (!type) continue
      const url = '/' + relative(dir, path).split(sep).join('/')
      files.set(url, { body: readFileSync(path), type, cache: url.startsWith('/assets/') ? ASSET_CACHE : PAGE_CACHE })
    }
  }
  walk(dir)
  const index = files.get('/index.html')
  if (!index) throw new Error(`no index.html in ${dir}`)
  files.set('/', index)
  return files
}

/** "Shroudtopia.PauloCarmino.com:443" → "shroudtopia.paulocarmino.com" */
export function hostOf(header: string | undefined): string {
  return (header ?? '').trim().toLowerCase().replace(/:\d+$/, '').replace(/\.$/, '')
}
