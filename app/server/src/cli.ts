// Maintenance by hand, inside the pod:
//   kubectl -n shroudtopia exec deploy/shroudtopia-sync -- node src/cli.ts worlds
//   kubectl -n shroudtopia exec deploy/shroudtopia-sync -- node src/cli.ts delete-world <worldId>
//   kubectl -n shroudtopia exec deploy/shroudtopia-sync -- node src/cli.ts delete-player <steamId>
// Deleting removes the rows and the blobs of the store.
import { join } from 'node:path'
import { loadConfig } from './config.ts'
import { avatarKey, FolderObjectStore, S3ObjectStore } from './object-store.ts'
import { Sync } from './sync.ts'

const [command, argument] = process.argv.slice(2)
const config = loadConfig()
const sync = new Sync(join(config.dataDir, 'shroudtopia.sqlite'))
const store = config.s3 ? new S3ObjectStore(config.s3) : new FolderObjectStore(join(config.dataDir, 'blobs'))

try {
  switch (command) {
    case 'worlds': {
      const rows = sync.db
        .prepare('SELECT w.world_id, w.name, (SELECT COUNT(*) FROM members m WHERE m.world_id = w.world_id) AS members, (SELECT COUNT(*) FROM versions v WHERE v.world_id = w.world_id) AS versions FROM worlds w ORDER BY w.created_at')
        .all()
      console.table(rows)
      break
    }
    case 'delete-world': {
      if (!argument) throw new Error('usage: delete-world <worldId>')
      const keys = sync.deleteWorld(argument)
      for (const key of keys) await store.delete(key)
      console.log(`deleted world ${argument} and ${keys.length} files`)
      break
    }
    case 'delete-player': {
      if (!argument) throw new Error('usage: delete-player <steamId>')
      const { hadAvatar } = sync.deletePlayer(argument)
      if (hadAvatar) await store.delete(avatarKey(argument))
      console.log(`deleted player ${argument}`)
      break
    }
    default:
      throw new Error('commands: worlds, delete-world <worldId>, delete-player <steamId>')
  }
} catch (error) {
  console.error((error as Error).message)
  process.exitCode = 1
} finally {
  sync.close()
}
