// Steam login over OpenID 2.0 (app/CONTRACT.md, "Steam login"). Steam is the OpenID provider: the browser goes there
// with checkid_setup, comes back to our callback with a signed assertion, and we ask Steam itself whether that
// assertion is genuine (check_authentication) instead of checking signatures here. What we check ourselves, strictly:
// the assertion is for our exact return_to, from Steam's endpoint, about a Steam identity, signed over the fields we
// read, fresh, and never seen before (the nonce, in sync.ts).

export const STEAM_OPENID_ENDPOINT = 'https://steamcommunity.com/openid/login'
const NS = 'http://specs.openid.net/auth/2.0'
const IDENTIFIER_SELECT = 'http://specs.openid.net/auth/2.0/identifier_select'
const CLAIMED_ID = /^https:\/\/steamcommunity\.com\/openid\/id\/(\d{17})$/
/** what must be among `openid.signed`, or a field we rely on could have been added by anyone */
const MUST_BE_SIGNED = ['op_endpoint', 'claimed_id', 'identity', 'return_to', 'response_nonce', 'assoc_handle']
/** a nonce older (or further in the future) than this is refused */
const NONCE_WINDOW_MS = 10 * 60_000

export interface SteamSettings {
  /** the server's own public origin, e.g. https://shroudtopia-api.paulocarmino.com */
  origin: string
  /** Steam's OpenID endpoint; a fake one in the tests */
  endpoint: string
  /** how the check_authentication request is made (fetch; replaced in no test, the endpoint is) */
  fetch?: typeof fetch
}

export class SteamLoginError extends Error {
  readonly reason: string
  constructor(reason: string, message: string) {
    super(message)
    this.reason = reason
  }
}

export const returnTo = (origin: string, sessionId: string) => `${origin}/auth/steam/callback/${sessionId}`

/** Where /auth/steam/redirect/:sessionId sends the browser. */
export function steamLoginUrl(settings: SteamSettings, sessionId: string): string {
  const url = new URL(settings.endpoint)
  url.search = new URLSearchParams({
    'openid.ns': NS,
    'openid.mode': 'checkid_setup',
    'openid.return_to': returnTo(settings.origin, sessionId),
    'openid.realm': `${settings.origin}/`,
    'openid.identity': IDENTIFIER_SELECT,
    'openid.claimed_id': IDENTIFIER_SELECT,
  }).toString()
  return url.toString()
}

/**
 * Checks the assertion Steam's redirect carried to /auth/steam/callback/:sessionId and returns the steamId and the
 * nonce. Throws SteamLoginError for anything short of a valid, fresh assertion for this session.
 */
export async function verifySteamAssertion(
  settings: SteamSettings,
  sessionId: string,
  query: Record<string, unknown>,
  now: number,
): Promise<{ steamId: string; nonce: string }> {
  const params: Record<string, string> = {}
  for (const [key, value] of Object.entries(query)) {
    if (!key.startsWith('openid.')) continue
    if (typeof value !== 'string') throw new SteamLoginError('malformed', `${key} appears more than once`)
    params[key] = value
  }
  const get = (name: string) => params[`openid.${name}`]

  if (get('mode') === 'cancel') throw new SteamLoginError('cancelled', 'the login was cancelled on Steam')
  if (get('mode') !== 'id_res') throw new SteamLoginError('malformed', 'not an OpenID positive assertion')
  if (get('ns') !== NS) throw new SteamLoginError('malformed', 'not OpenID 2.0')
  if (get('op_endpoint') !== settings.endpoint) throw new SteamLoginError('wrong_provider', 'the assertion is not from Steam')
  if (get('return_to') !== returnTo(settings.origin, sessionId)) throw new SteamLoginError('wrong_return_to', 'the assertion is for another address')

  const claimed = get('claimed_id') ?? ''
  const match = CLAIMED_ID.exec(claimed)
  if (!match || get('identity') !== claimed) throw new SteamLoginError('wrong_identity', 'the assertion is not about a Steam account')

  const signed = (get('signed') ?? '').split(',')
  const unsigned = MUST_BE_SIGNED.filter(field => !signed.includes(field))
  if (unsigned.length) throw new SteamLoginError('unsigned', `not signed: ${unsigned.join(', ')}`)
  if (!get('sig')) throw new SteamLoginError('unsigned', 'no signature')

  const nonce = get('response_nonce') ?? ''
  const stamp = /^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ)/.exec(nonce)
  const at = stamp ? Date.parse(stamp[1]!) : Number.NaN
  if (Number.isNaN(at) || Math.abs(now - at) > NONCE_WINDOW_MS) throw new SteamLoginError('stale', 'the assertion is too old')

  // Steam says whether it signed this; every field goes back exactly as received, only the mode changes
  const body = new URLSearchParams({ ...params, 'openid.mode': 'check_authentication' })
  let text: string
  try {
    const response = await (settings.fetch ?? fetch)(settings.endpoint, {
      method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' },
      body: body.toString(),
      signal: AbortSignal.timeout(10_000),
    })
    if (!response.ok) throw new Error(`Steam answered ${response.status}`)
    text = await response.text()
  } catch (error) {
    throw new SteamLoginError('steam_unreachable', `could not ask Steam: ${(error as Error).message}`)
  }
  const lines = new Map(
    text
      .split('\n')
      .map(line => line.trim())
      .filter(Boolean)
      .map(line => [line.slice(0, line.indexOf(':')), line.slice(line.indexOf(':') + 1)] as const),
  )
  if (lines.get('ns') !== NS || lines.get('is_valid') !== 'true') throw new SteamLoginError('invalid', 'Steam did not confirm the login')

  return { steamId: match[1]!, nonce }
}

const page = (title: string, text: string) => `<!doctype html>
<html lang="pt-BR">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Shroudtopia</title>
<style>body{font-family:system-ui,sans-serif;max-width:32rem;margin:15vh auto;padding:0 1rem;line-height:1.5;color:#222;background:#fafafa}
@media (prefers-color-scheme:dark){body{color:#eee;background:#161616}}h1{font-size:1.4rem}</style></head>
<body><h1>${title}</h1><p>${text}</p></body>
</html>
`

export const donePage = () => page('Pronto, pode voltar para o app', 'Seu login na Steam foi confirmado. Já pode fechar esta aba.')

const REASONS: Record<string, string> = {
  cancelled: 'O login foi cancelado na Steam.',
  expired: 'Este login expirou ou já foi usado.',
  steam_unreachable: 'Não foi possível falar com a Steam agora.',
}

export const failPage = (reason: string) =>
  page('Não deu certo', `${REASONS[reason] ?? 'A Steam não confirmou este login.'} Volte para o app e tente de novo.`)
