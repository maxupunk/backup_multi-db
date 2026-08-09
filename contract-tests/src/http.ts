/**
 * Cliente HTTP da suite de contrato (tarefa 1.3 do roadmap).
 *
 * Deliberadamente burro: monta a requisicao, mede, devolve a resposta crua e
 * anota a cobertura. Nao interpreta erro, nao lanca por status — um 500
 * inesperado tem que chegar ao teste como dado, nao como excecao, senao a
 * mensagem de falha perde o corpo da resposta, que e' justamente onde esta' a
 * explicacao.
 */

import { request as undiciRequest } from 'undici'
import { loadConfig } from './config.ts'
import { matchRoute } from './routes.ts'
import { recordCoverage } from './coverage.ts'

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE' | 'HEAD' | 'OPTIONS'

/** Metodos seguros de repetir apos falha de conexao. */
const IDEMPOTENT: ReadonlySet<string> = new Set(['GET', 'HEAD', 'OPTIONS', 'PUT', 'DELETE'])

export interface RequestOptions {
  /** Token bearer. Ausente = requisicao anonima. */
  token?: string | null
  /** Corpo JSON. Define `content-type: application/json`. */
  json?: unknown
  /** Corpo cru, para testar payload malformado. */
  body?: string | Buffer
  query?: Record<string, string | number | boolean | undefined>
  headers?: Record<string, string>
  /** Nao desserializa o corpo — use para download/binario. */
  raw?: boolean
  timeoutMs?: number
  /** Rotulo para o rastro de cobertura (default: nome do arquivo de teste). */
  test?: string
  /**
   * Nao contabiliza a chamada na cobertura de rotas.
   *
   * Usado pelo seed: ele bate em `POST /api/auth/register` sem afirmar nada
   * sobre a resposta. Contar isso como cobertura marcaria a rota como testada
   * quando ninguem a testou — exatamente o tipo de metrica que engana.
   */
  skipCoverage?: boolean
}

export interface ContractResponse {
  status: number
  headers: Record<string, string>
  contentType: string
  /** Corpo desserializado se JSON; string se texto; Buffer se `raw`. */
  body: unknown
  /** Corpo cru, sempre disponivel para a mensagem de erro. */
  text: string
  durationMs: number
  method: HttpMethod
  path: string
  /** Template do baseline que a URL casou, ou `null`. */
  route: string | null
}

function buildUrl(baseUrl: string, path: string, query?: RequestOptions['query']): string {
  const url = new URL(path.startsWith('/') ? path : `/${path}`, baseUrl)

  for (const [key, value] of Object.entries(query ?? {})) {
    if (value === undefined) continue
    url.searchParams.set(key, String(value))
  }

  return url.toString()
}

function normalizeHeaders(input: Record<string, string | string[] | undefined>): Record<string, string> {
  const result: Record<string, string> = {}

  for (const [key, value] of Object.entries(input)) {
    if (value === undefined) continue
    result[key.toLowerCase()] = Array.isArray(value) ? value.join(', ') : value
  }

  return result
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/** Erro de conexao/timeout, distinto de uma resposta HTTP de erro. */
function isTransport(error: unknown): boolean {
  const code = (error as { code?: string } | null)?.code
  return (
    code === 'ECONNREFUSED' ||
    code === 'ECONNRESET' ||
    code === 'EPIPE' ||
    code === 'UND_ERR_SOCKET' ||
    code === 'UND_ERR_CONNECT_TIMEOUT'
  )
}

export async function httpRequest(
  method: HttpMethod,
  path: string,
  options: RequestOptions = {}
): Promise<ContractResponse> {
  const config = loadConfig()
  const url = buildUrl(config.baseUrl, path, options.query)

  const headers: Record<string, string> = {
    accept: 'application/json',
    ...options.headers,
  }

  let body = options.body
  if (options.json !== undefined) {
    body = JSON.stringify(options.json)
    headers['content-type'] ??= 'application/json'
  }
  if (options.token) {
    headers['authorization'] = `Bearer ${options.token}`
  }

  // Repeticao so' para falha de transporte e so' em metodo idempotente. Repetir
  // um POST que ja' chegou ao servidor criaria o recurso duas vezes e o teste
  // seguinte veria um estado que ninguem pediu.
  const attempts = IDEMPOTENT.has(method) ? config.retries + 1 : 1

  let lastError: unknown
  for (let attempt = 1; attempt <= attempts; attempt++) {
    const startedAt = performance.now()

    try {
      const response = await undiciRequest(url, {
        method,
        headers,
        body,
        headersTimeout: options.timeoutMs ?? config.requestTimeoutMs,
        bodyTimeout: options.timeoutMs ?? config.requestTimeoutMs,
      })

      const durationMs = performance.now() - startedAt
      const responseHeaders = normalizeHeaders(response.headers)
      const contentType = responseHeaders['content-type'] ?? ''

      const buffer = Buffer.from(await response.body.arrayBuffer())
      const text = buffer.toString('utf8')

      let parsed: unknown
      if (options.raw) {
        parsed = buffer
      } else if (contentType.includes('application/json') && text !== '') {
        try {
          parsed = JSON.parse(text)
        } catch {
          // Content-type diz JSON mas o corpo nao e'. Isso e' um achado, nao um
          // acidente: entrega o texto para o teste poder afirmar sobre ele.
          parsed = text
        }
      } else {
        parsed = text === '' ? null : text
      }

      const pathname = new URL(url).pathname
      const route = matchRoute(method, pathname)

      if (!options.skipCoverage) {
        recordCoverage({
          key: route?.key ?? null,
          method,
          path: pathname,
          status: response.statusCode,
          test: options.test,
        })
      }

      return {
        status: response.statusCode,
        headers: responseHeaders,
        contentType,
        body: parsed,
        text,
        durationMs,
        method,
        path: pathname,
        route: route?.key ?? null,
      }
    } catch (error) {
      lastError = error
      if (attempt < attempts && isTransport(error)) {
        await sleep(config.retryDelayMs)
        continue
      }
      break
    }
  }

  throw new Error(`${method} ${url} falhou no transporte apos ${attempts} tentativa(s)`, {
    cause: lastError,
  })
}

/** Cliente com token fixo — o que `as(user)` e `unauth()` devolvem. */
export interface ContractClient {
  get(path: string, options?: RequestOptions): Promise<ContractResponse>
  post(path: string, options?: RequestOptions): Promise<ContractResponse>
  put(path: string, options?: RequestOptions): Promise<ContractResponse>
  patch(path: string, options?: RequestOptions): Promise<ContractResponse>
  delete(path: string, options?: RequestOptions): Promise<ContractResponse>
  request(method: HttpMethod, path: string, options?: RequestOptions): Promise<ContractResponse>
  readonly token: string | null
}

export function createClient(token: string | null): ContractClient {
  const send = (method: HttpMethod, path: string, options: RequestOptions = {}) =>
    httpRequest(method, path, { ...options, token: options.token ?? token })

  return {
    token,
    request: send,
    get: (path, options) => send('GET', path, options),
    post: (path, options) => send('POST', path, options),
    put: (path, options) => send('PUT', path, options),
    patch: (path, options) => send('PATCH', path, options),
    delete: (path, options) => send('DELETE', path, options),
  }
}

/** Resumo de uma resposta para mensagem de erro, com o corpo truncado. */
export function describeResponse(response: ContractResponse, limit = 800): string {
  const body = response.text.length > limit ? `${response.text.slice(0, limit)}…` : response.text
  return `${response.method} ${response.path} -> ${response.status} ${response.contentType}\n${body}`
}
