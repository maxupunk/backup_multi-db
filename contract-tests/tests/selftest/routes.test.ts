import { describe, expect, it } from 'vitest'
import { baselineRoutes, matchRoute, parseBaseline, protectedRoutes } from '../../src/routes.ts'

describe('parseBaseline', () => {
  it('le metodo, rota e middlewares', () => {
    const [route] = parseBaseline('POST    /api/auth/login   [rateLimit,rateLimit]')
    expect(route).toMatchObject({
      method: 'POST',
      pattern: '/api/auth/login',
      middleware: ['rateLimit', 'rateLimit'],
      key: 'POST /api/auth/login',
    })
  })

  it('aceita rota sem middleware', () => {
    expect(parseBaseline('GET /api/health')[0]).toMatchObject({ middleware: [] })
  })

  it('reprova linha fora do formato em vez de ignora-la', () => {
    // Ignorar em silencio faria a rota sumir do baseline e a cobertura
    // reportar 100% sem tê-la testado.
    expect(() => parseBaseline('isso nao e uma rota')).toThrow(/nao casa com o formato/)
  })
})

describe('matchRoute', () => {
  it('casa rota literal', () => {
    expect(matchRoute('GET', '/api/health')?.key).toBe('GET /api/health')
  })

  it('casa parametro', () => {
    expect(matchRoute('GET', '/api/connections/12')?.key).toBe('GET /api/connections/:id')
  })

  it('respeita o metodo', () => {
    expect(matchRoute('DELETE', '/api/connections/12')?.key).toBe('DELETE /api/connections/:id')
    expect(matchRoute('PATCH', '/api/health')).toBeNull()
  })

  it('desempata a favor da rota com mais segmentos literais', () => {
    // `/api/storages/copy-jobs/:jobId` e `/api/storages/:id/browse` tem o
    // mesmo numero de segmentos. Sem o desempate, a cobertura creditaria a
    // rota errada e uma das duas ficaria eternamente "sem teste".
    expect(matchRoute('GET', '/api/storages/copy-jobs/abc')?.key).toBe(
      'GET /api/storages/copy-jobs/:jobId'
    )
    expect(matchRoute('GET', '/api/storages/42/browse')?.key).toBe('GET /api/storages/:id/browse')
  })

  it('ignora query string', () => {
    expect(matchRoute('GET', '/api/connections?page=2')?.key).toBe('GET /api/connections')
  })

  it('devolve null para caminho desconhecido', () => {
    // Em POST, porque em GET o catch-all `/*` casa com tudo — e isso reflete o
    // servidor de verdade, nao uma falha do casador (ver o achado sobre o
    // catch-all em `tests/global.contract.test.ts`).
    expect(matchRoute('POST', '/api/nao-existe')).toBeNull()
    expect(matchRoute('GET', '/api/nao-existe')?.pattern).toBe('/*')
  })

  it('nao casa com numero de segmentos diferente', () => {
    expect(matchRoute('DELETE', '/api/connections/12/nao-existe')).toBeNull()
  })
})

describe('baseline do repositorio', () => {
  it('carrega as 87 rotas de /api gravadas na Fase 0', () => {
    const apiRoutes = baselineRoutes().filter((route) => route.pattern.startsWith('/api/'))
    expect(apiRoutes.length).toBe(87)
  })

  it('toda rota do baseline casa com o proprio template', () => {
    // Prova que o casador cobre o inventario inteiro, e nao apenas os casos
    // escolhidos a mao acima.
    for (const route of baselineRoutes()) {
      const concrete = route.pattern.replace(/:([A-Za-z0-9_]+)/g, 'valor-$1')
      expect(matchRoute(route.method, concrete), `${route.key} nao casou`).not.toBeNull()
    }
  })

  it('separa as rotas protegidas', () => {
    const withAuth = protectedRoutes()
    expect(withAuth.length).toBeGreaterThan(70)
    expect(withAuth.every((route) => route.middleware.includes('auth'))).toBe(true)
  })
})

describe('catch-all da SPA', () => {
  it('`*` engole qualquer numero de segmentos', () => {
    // `GET /*` e' o fallback que serve o index.html. Sem tratar o curinga, a
    // rota nunca casaria e ficaria eternamente listada como "sem teste".
    expect(matchRoute('GET', '/qualquer')?.pattern).toBe('/*')
    expect(matchRoute('GET', '/uma/rota/do/frontend')?.pattern).toBe('/*')
  })

  it('nao rouba rotas mais especificas', () => {
    // O desempate por segmentos literais tem que continuar valendo: `/*` tem
    // zero literais e perde para qualquer rota de verdade.
    expect(matchRoute('GET', '/api/health')?.pattern).toBe('/api/health')
    expect(matchRoute('GET', '/api/connections/7')?.pattern).toBe('/api/connections/:id')
  })

  it('nao vale para outros metodos', () => {
    // O catch-all e' so' GET; POST num caminho desconhecido nao casa com nada.
    expect(matchRoute('POST', '/rota-inexistente')).toBeNull()
  })
})
