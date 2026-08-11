/**
 * Lote 2.1 — rotas publicas.
 *
 * `GET /api/health` foi o marco de conclusao da Fase 1: e' o endpoint mais
 * simples que existe e ainda assim exercita a cadeia inteira do harness —
 * subida do servidor, cliente HTTP, casamento com o baseline, redacao, golden
 * e cobertura.
 */

import { describe, expect, it } from 'vitest'
import { expectGolden } from '../src/golden.ts'
import { expectStatus, json, unauth } from '../src/session.ts'

describe('GET /api/health', () => {
  it('responde 200 sem autenticacao', async () => {
    const response = expectStatus(await unauth().get('/api/health'), 200)

    const body = json<{ status: string; timestamp: string; version: string }>(response)
    expect(body.status).toBe('ok')
    expect(body.version).toBe('1.0.0')
    // O timestamp e' redigido no golden, entao a unica coisa que resta afirmar
    // sobre ele e' que e' uma data valida — e isso o golden nao verifica.
    expect(Number.isNaN(Date.parse(body.timestamp))).toBe(false)

    expectGolden('health/get', response)
  })

  it('casa com a rota do baseline', async () => {
    // Se este casamento quebrar, a cobertura da Fase 2 passa a contar rotas
    // erradas em silencio. Vale uma assercao explicita.
    const response = await unauth().get('/api/health')
    expect(response.route).toBe('GET /api/health')
  })

  it('anuncia o limite global de requisicoes', async () => {
    // 600/min e' o `global` de `app/middleware/rate_limit_middleware.ts`. O
    // backend tem que expor o mesmo cabecalho com o mesmo numero — o
    // frontend nao usa, mas quem opera a API usa.
    const response = await unauth().get('/api/health')
    expect(response.headers['x-ratelimit-limit']).toBe('600')
    expect(response.headers['x-ratelimit-remaining']).toBeDefined()
    expect(response.headers['x-ratelimit-reset']).toBeDefined()
  })
})

describe('GET /api/auth/status', () => {
  it('reporta que o sistema ja tem usuarios depois do seed', async () => {
    const response = expectStatus(await unauth().get('/api/auth/status'), 200)

    const body = json<{ success: boolean; data: { hasUsers: boolean } }>(response)
    expect(body.success).toBe(true)
    expect(body.data.hasUsers).toBe(true)

    expectGolden('auth/status', response)
  })

  it('nao exige bootstrap token fora de producao', async () => {
    // `requiresBootstrapToken` so' e' `true` quando nao ha' usuario **e** a
    // aplicacao esta' em producao. Como o seed ja' criou usuarios, tem que ser
    // `false` nos dois ambientes — a assercao vale para as duas stacks.
    const body = json<{ data: { requiresBootstrapToken: boolean } }>(
      await unauth().get('/api/auth/status')
    )
    expect(body.data.requiresBootstrapToken).toBe(false)
  })
})

describe('GET /api/swagger', () => {
  it('serve a especificacao OpenAPI', async () => {
    const response = expectStatus(await unauth().get('/api/swagger'), 200)

    // Sem golden aqui de proposito: a spec e' um YAML de milhares de linhas e
    // chega como **string**, entao o golden guardaria o texto inteiro e
    // qualquer rota nova viraria um diff gigante sem informacao. O contrato
    // util e' `docs/openapi-baseline.yml`, gravado na Fase 0, que ja' e' o
    // alvo do `utoipa` (decisao D10).
    expect(response.text).toContain('openapi')
    expect(response.text).toContain('/api/health')
  })
})

describe('GET /api/docs', () => {
  it('serve a UI do Swagger', async () => {
    const response = expectStatus(await unauth().get('/api/docs'), 200)

    expect(response.text.toLowerCase()).toContain('<html')
    // A UI aponta para a spec; se esse caminho mudar, a pagina abre vazia.
    expect(response.text).toContain('/api/swagger')
  })
})
