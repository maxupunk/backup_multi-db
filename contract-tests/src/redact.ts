/**
 * Redacao de campos volateis para os golden files (parte da tarefa 1.6).
 *
 * Um golden so' serve se for estavel: gravar duas vezes seguidas tem que
 * produzir bytes identicos, senao `git diff` vira ruido e ninguem repara
 * quando o contrato muda de verdade. Timestamp, id incremental, duracao,
 * caminho temporario e token mudam a cada execucao — todos viram um marcador.
 *
 * A redacao e' por *chave* e por *formato do valor*, nas duas pontas, porque
 * nenhuma das duas sozinha pega tudo: `data.id` e' volatil pelo nome, e uma
 * data ISO dentro de uma string de mensagem e' volatil pelo formato.
 *
 * Redigir tambem tem funcao de seguranca: token e senha nunca vao para um
 * arquivo versionado.
 */

export const REDACTED = {
  id: '<id>',
  timestamp: '<timestamp>',
  duration: '<duration>',
  path: '<path>',
  secret: '<secret>',
  uuid: '<uuid>',
  /** Trecho que a comparacao ignora — gravado so' como marcador. */
  notCompared: '<nao-comparado>',
} as const

/**
 * Chaves cujo valor e' sempre volatil.
 *
 * Os padroes sao ancorados em maiuscula (`Id`, `At`) ou em `_`, nunca em
 * sufixo solto: `/.*id$/i` casaria com `valid` e `uuid`, e `/.*at$/i` com
 * `format` e `flat`. Redigir demais e' pior que redigir de menos — apaga a
 * diferenca que o golden existe para pegar.
 */
const VOLATILE_KEYS: Array<{ test: RegExp; replacement: string }> = [
  { test: /^(id|.*_id|.*[a-z0-9]Id)$/, replacement: REDACTED.id },
  {
    test: /^(timestamp|date|expiresIn|expires_in|.*_at|.*[a-z0-9]At)$/,
    replacement: REDACTED.timestamp,
  },
  {
    test: /^(duration|durationMs|duration_ms|durationSeconds|elapsed|elapsedMs|elapsed_ms|took|tookMs|took_ms|latency|latencyMs|latency_ms|uptimeSeconds)$/,
    replacement: REDACTED.duration,
  },
  {
    test: /^(token|accessToken|access_token|refreshToken|refresh_token|password|secret|.*[a-z0-9]Secret|.*_secret|hash|apiKey|api_key|credentialsJson)$/,
    replacement: REDACTED.secret,
  },
  {
    test: /^(path|filePath|file_path|fullPath|full_path|storagePath|storage_path|basePath|base_path|location)$/,
    replacement: REDACTED.path,
  },
]

const ISO_DATE = /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?$/
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const ABSOLUTE_PATH = /^(?:[A-Za-z]:[\\/]|\/)[^\s]*$/
const BEARER_TOKEN = /^oat_[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/

export interface RedactOptions {
  /**
   * Caminhos extras a redigir, com `*` no lugar do indice de array.
   * Ex.: `data.items.*.customName`.
   */
  extraPaths?: string[]
  /**
   * Caminhos a preservar mesmo que uma regra generica os pegue.
   * Ex.: `version` do /api/health, que faz parte do contrato.
   */
  keepPaths?: string[]
  /**
   * Caminhos que a comparacao ignora e que por isso viram um marcador unico.
   *
   * Sao os trechos que dependem da maquina: uso de CPU, memoria livre, uptime,
   * latencia de rede, espaco em disco. Gravar o valor real deles faria o
   * golden mudar a cada execucao — e um arquivo que muda sozinho para de
   * servir como registro de mudanca de contrato.
   */
  notComparedPaths?: string[]
}

function pathMatches(path: string, patterns: string[]): boolean {
  return patterns.some((pattern) => {
    const regex = new RegExp(
      '^' +
        pattern
          .split('.')
          .map((part) => (part === '*' ? '[^.]+' : part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')))
          .join('\\.') +
        '$'
    )
    return regex.test(path)
  })
}

function redactByKey(key: string): string | null {
  for (const rule of VOLATILE_KEYS) {
    if (rule.test.test(key)) return rule.replacement
  }
  return null
}

function redactByValue(value: string): string | null {
  if (ISO_DATE.test(value)) return REDACTED.timestamp
  if (UUID.test(value)) return REDACTED.uuid
  if (BEARER_TOKEN.test(value)) return REDACTED.secret
  if (ABSOLUTE_PATH.test(value)) return REDACTED.path
  return null
}

export function redact(value: unknown, options: RedactOptions = {}, path = ''): unknown {
  const { extraPaths = [], keepPaths = [], notComparedPaths = [] } = options

  if (path !== '' && pathMatches(path, keepPaths)) return value
  if (path !== '' && pathMatches(path, notComparedPaths)) return REDACTED.notCompared
  if (path !== '' && pathMatches(path, extraPaths)) return REDACTED.secret

  if (value === null || value === undefined) return null

  if (Array.isArray(value)) {
    return value.map((item, index) => redact(item, options, path === '' ? `${index}` : `${path}.${index}`))
  }

  if (typeof value === 'object') {
    const result: Record<string, unknown> = {}
    // Chaves ordenadas: a ordem de serializacao nao e' contrato, e um golden
    // cuja ordem de chaves oscila e' um golden que gera diff falso.
    for (const key of Object.keys(value as Record<string, unknown>).sort()) {
      const childPath = path === '' ? key : `${path}.${key}`
      const child = (value as Record<string, unknown>)[key]

      if (pathMatches(childPath, keepPaths)) {
        result[key] = child
        continue
      }

      const byKey = redactByKey(key)
      if (byKey !== null && (child === null || typeof child !== 'object')) {
        result[key] = byKey
        continue
      }

      result[key] = redact(child, options, childPath)
    }
    return result
  }

  if (typeof value === 'string') {
    return redactByValue(value) ?? value
  }

  return value
}

/**
 * Cabecalhos que entram no golden.
 *
 * Lista de bloqueio, nao de permissao: um cabecalho novo no back-roco tem que
 * aparecer como diferenca, e uma lista de permissao o esconderia.
 */
const VOLATILE_HEADERS = new Set([
  'date',
  'etag',
  'last-modified',
  'content-length',
  'connection',
  'keep-alive',
  'transfer-encoding',
  'x-ratelimit-remaining',
  'x-ratelimit-reset',
  'set-cookie',
  'x-request-id',
  'server',
])

export function redactHeaders(headers: Record<string, string>): Record<string, string> {
  const result: Record<string, string> = {}

  for (const key of Object.keys(headers).sort()) {
    const lower = key.toLowerCase()
    if (VOLATILE_HEADERS.has(lower)) continue

    // `x-ratelimit-limit` e' contrato (o limite configurado); o `remaining` e o
    // `reset` sao estado e ja' sairam acima.
    result[lower] = headers[key]!
  }

  return result
}
