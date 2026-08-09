/**
 * Rotas publicas — o primeiro contrato gravado.
 *
 * `GET /api/health` e' o marco de conclusao da Fase 1: e' o endpoint mais
 * simples que existe e, mesmo assim, exercita a cadeia inteira do harness —
 * subida do servidor, cliente HTTP, casamento com o baseline de rotas,
 * redacao, golden e cobertura.
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
})

describe('GET /api/auth/status', () => {
  it('reporta que o sistema ja tem usuarios depois do seed', async () => {
    const response = expectStatus(await unauth().get('/api/auth/status'), 200)

    const body = json<{ success: boolean; data: { hasUsers: boolean } }>(response)
    expect(body.success).toBe(true)
    expect(body.data.hasUsers).toBe(true)

    expectGolden('auth/status', response)
  })
})
