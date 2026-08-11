/**
 * Inventario de rotas do baseline e casamento URL concreta -> template.
 *
 * O relatorio de cobertura (1.8) precisa saber que `GET /api/connections/12`
 * exercitou `GET /api/connections/:id`. Sem esse casamento a cobertura seria
 * apenas uma lista de URLs visitadas, que nunca cruzaria com o baseline.
 *
 * Fonte: `docs/routes-baseline.txt`, gravado na Fase 0 a partir do back-roco.
 * E' o inventario de referencia — se o arquivo sumir, a suite nao tem contra
 * o que medir cobertura e falha alto.
 */

import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { REPO_ROOT } from './config.ts'

export interface BaselineRoute {
  method: string
  /** Template com parametros, ex.: `/api/connections/:id`. */
  pattern: string
  /** Middlewares declarados na rota, ex.: `['rateLimit', 'auth']`. */
  middleware: string[]
  /** `METHOD pattern` — chave estavel usada em cobertura e relatorios. */
  key: string
}

const BASELINE_PATH = join(REPO_ROOT, 'docs', 'routes-baseline.txt')

/** `GET     /api/health   [rateLimit]` */
const LINE = /^([A-Z]+)\s+(\S+)(?:\s+\[([^\]]*)\])?\s*$/

export function parseBaseline(text: string): BaselineRoute[] {
  const routes: BaselineRoute[] = []

  for (const [index, rawLine] of text.split(/\r?\n/).entries()) {
    const line = rawLine.trim()
    if (line === '' || line.startsWith('#')) continue

    const match = LINE.exec(line)
    if (!match) {
      throw new Error(`routes-baseline.txt:${index + 1} nao casa com o formato esperado: ${line}`)
    }

    const [, method, pattern, middleware] = match
    routes.push({
      method: method!,
      pattern: pattern!,
      middleware: (middleware ?? '')
        .split(',')
        .map((item) => item.trim())
        .filter((item) => item !== ''),
      key: `${method} ${pattern}`,
    })
  }

  return routes
}

let cached: BaselineRoute[] | null = null

export function baselineRoutes(): BaselineRoute[] {
  if (cached) return cached

  let text: string
  try {
    text = readFileSync(BASELINE_PATH, 'utf8')
  } catch (cause) {
    throw new Error(
      `Nao consegui ler ${BASELINE_PATH}. Ele e' o inventario de rotas de referencia ` +
        `(Fase 0); sem ele nao ha' como medir cobertura. Regere a partir do back-roco.`,
      { cause }
    )
  }

  cached = parseBaseline(text)
  if (cached.length === 0) {
    throw new Error(`${BASELINE_PATH} esta vazio — o baseline de rotas nao pode ser vazio.`)
  }
  return cached
}

function segmentsOf(path: string): string[] {
  return path.split('/').filter((segment) => segment !== '')
}

function patternMatches(patternSegments: string[], pathSegments: string[]): boolean {
  const wildcardAt = patternSegments.indexOf('*')

  if (wildcardAt !== -1) {
    // `*` e' o catch-all da SPA (`GET /*`): engole todos os segmentos
    // restantes, entao o caminho so' precisa ser pelo menos tao longo quanto o
    // prefixo antes do curinga. Sem isso, `GET /*` nunca casaria com nada e
    // apareceria eternamente como "rota sem teste".
    if (pathSegments.length < wildcardAt) return false
    return patternSegments
      .slice(0, wildcardAt)
      .every((segment, index) => segment === pathSegments[index])
  }

  if (patternSegments.length !== pathSegments.length) return false

  return patternSegments.every((segment, index) => {
    const actual = pathSegments[index]!
    // Um parametro casa com qualquer segmento nao vazio.
    if (segment.startsWith(':')) return actual !== ''
    return segment === actual
  })
}

/**
 * Devolve o template do baseline que atende `method` + `pathname`.
 *
 * Quando mais de um template casa — `/api/storages/copy-jobs/:jobId` e
 * `/api/storages/:id/browse` tem o mesmo formato — vence o de mais segmentos
 * literais. Sem esse desempate a cobertura creditaria a rota errada.
 */
export function matchRoute(method: string, pathname: string): BaselineRoute | null {
  const upperMethod = method.toUpperCase()
  const pathSegments = segmentsOf(pathname.split('?')[0]!)

  let best: BaselineRoute | null = null
  let bestScore = -1

  for (const route of baselineRoutes()) {
    if (route.method !== upperMethod) continue

    const patternSegments = segmentsOf(route.pattern)
    if (!patternMatches(patternSegments, pathSegments)) continue

    const literalCount = patternSegments.filter((segment) => !segment.startsWith(':')).length
    if (literalCount > bestScore) {
      best = route
      bestScore = literalCount
    }
  }

  return best
}

/** Rotas que exigem `auth` — usado pelos testes de 401 e pelo relatorio. */
export function protectedRoutes(): BaselineRoute[] {
  return baselineRoutes().filter((route) => route.middleware.includes('auth'))
}
