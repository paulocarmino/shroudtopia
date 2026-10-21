// Identifiers: ULIDs for worlds and versions, device tokens, invite codes.
import { createHash, randomBytes, randomInt } from 'node:crypto'

const CROCKFORD = '0123456789ABCDEFGHJKMNPQRSTVWXYZ'

/** A ULID: 48 bits of milliseconds and 80 random bits, 26 Crockford base32 characters. */
export function ulid(now: number = Date.now()): string {
  let time = ''
  let t = now
  for (let i = 0; i < 10; i++) {
    time = CROCKFORD[t % 32] + time
    t = Math.floor(t / 32)
  }
  const bytes = randomBytes(16)
  let random = ''
  for (let i = 0; i < 16; i++) random += CROCKFORD[bytes[i]! % 32]
  return time + random
}

export const ULID = /^[0-9A-HJKMNP-TV-Z]{26}$/

/** The device token the app keeps: 256 random bits. Only its sha256 is stored. */
export function newToken(): string {
  return 'dev_' + randomBytes(32).toString('base64url')
}

export function hashToken(token: string): string {
  return createHash('sha256').update(token).digest('hex')
}

/** No 0/O, 1/I/L: the code is read aloud and typed by a friend. */
const CODE_ALPHABET = '23456789ABCDEFGHJKMNPQRSTUVWXYZ'

export function newInviteCode(): string {
  let code = ''
  for (let i = 0; i < 4; i++) code += CODE_ALPHABET[randomInt(CODE_ALPHABET.length)]
  return `ENSH-${code}`
}

/** "ensh-7kq2", " ENSH-7KQ2 " and "7KQ2" all name ENSH-7KQ2. */
export function normalizeInviteCode(text: string): string {
  const upper = text.trim().toUpperCase()
  return upper.startsWith('ENSH-') ? upper : `ENSH-${upper}`
}
