/**
 * Lote 2.1 — `POST /api/auth/register`.
 *
 * Cada caso usa um e-mail proprio. Nao e' capricho: o limiter de `auth` e' de
 * 5 req/min chaveado por **IP + e-mail**
 * (`app/middleware/rate_limit_middleware.ts`), entao e-mails distintos mantem
 * os casos independentes — e o teste de 429 pode estourar o limite do e-mail
 * dele sem derrubar os vizinhos.
 */

import { describe, expect, it } from 'vitest'
import { expectGolden } from '../src/golden.ts'
import { expectStatus, json, unauth } from '../src/session.ts'

const PASSWORD = 'contract-pass-123'

interface ValidationError {
  errors: Array<{ message: string; field: string; rule: string }>
}

describe('POST /api/auth/register — caminho feliz', () => {
  it('cria usuario pendente de aprovacao, sem token', async () => {
    const response = expectStatus(
      await unauth().post('/api/auth/register', {
        json: { email: 'novo@contract.test', password: PASSWORD, fullName: 'Novo Usuario' },
      }),
      201
    )

    const body = json<{ success: boolean; message: string; data?: unknown }>(response)
    expect(body.success).toBe(true)
    // Do segundo usuario em diante o cadastro nasce inativo. Devolver token
    // aqui seria uma falha de seguranca, nao so' de contrato: daria sessao a
    // quem ainda nao foi aprovado.
    expect(body.data).toBeUndefined()

    expectGolden('auth/register-pending', response)
  })

  it('nao consegue logar antes da aprovacao', async () => {
    // Fecha o ciclo do teste acima: prova que "pendente" significa mesmo
    // pendente, e nao apenas uma mensagem diferente.
    const response = await unauth().post('/api/auth/login', {
      json: { email: 'novo@contract.test', password: PASSWORD },
    })

    expect(response.status).toBe(401)
  })
})

describe('POST /api/auth/register — payload invalido', () => {
  it('recusa e-mail ja cadastrado', async () => {
    const response = expectStatus(
      await unauth().post('/api/auth/register', {
        json: { email: 'admin@contract.test', password: PASSWORD },
      }),
      422
    )

    const body = json<ValidationError>(response)
    expect(body.errors.some((error) => error.field === 'email')).toBe(true)

    expectGolden('auth/register-duplicate-email', response)
  })

  it('recusa senha curta demais', async () => {
    const body = json<ValidationError>(
      expectStatus(
        await unauth().post('/api/auth/register', {
          json: { email: 'senha-curta@contract.test', password: 'curta' },
        }),
        422
      )
    )

    const error = body.errors.find((item) => item.field === 'password')
    expect(error).toBeDefined()
    expect(error!.rule).toBe('minLength')
  })

  it('recusa e-mail malformado', async () => {
    const body = json<ValidationError>(
      expectStatus(
        await unauth().post('/api/auth/register', {
          json: { email: 'isso-nao-e-email', password: PASSWORD },
        }),
        422
      )
    )

    expect(body.errors.some((error) => error.field === 'email' && error.rule === 'email')).toBe(true)
  })

  it('recusa corpo vazio apontando os dois campos obrigatorios', async () => {
    const body = json<ValidationError>(
      expectStatus(await unauth().post('/api/auth/register', { json: {} }), 422)
    )

    const fields = body.errors.map((error) => error.field).sort()
    expect(fields).toEqual(['email', 'password'])
  })

  it('recusa JSON malformado', async () => {
    // Corpo que nem chega ao validador. O interessante e' que a resposta
    // continue sendo JSON — o `force_json_response` tem que valer tambem
    // quando o erro acontece antes do controller.
    const response = await unauth().post('/api/auth/register', {
      body: '{"email":',
      headers: { 'content-type': 'application/json' },
    })

    expect(response.status).toBeGreaterThanOrEqual(400)
    expect(response.contentType).toContain('application/json')
  })
})

describe('POST /api/auth/register — rate limit', () => {
  it('bloqueia na sexta tentativa do mesmo e-mail', async () => {
    // Payload invalido de proposito: o middleware de rate limit roda **antes**
    // do validador, entao o contador sobe sem que nenhum usuario seja criado.
    // Assim o teste nao deixa lixo no banco para os vizinhos.
    const payload = { email: 'flood@contract.test', password: 'curta' }
    const statuses: number[] = []

    for (let attempt = 0; attempt < 6; attempt++) {
      const response = await unauth().post('/api/auth/register', { json: payload })
      statuses.push(response.status)
    }

    // O limite de `auth` e' 5/min: as cinco primeiras passam pelo limiter e
    // morrem na validacao; a sexta nem chega la'.
    expect(statuses.slice(0, 5)).toEqual([422, 422, 422, 422, 422])
    expect(statuses[5]).toBe(429)
  })

  it('nao afeta outro e-mail', async () => {
    // A chave do limiter inclui o e-mail. Se algum dia virar so' o IP, este
    // teste quebra — e toda a suite passaria a depender da ordem de execucao.
    const response = await unauth().post('/api/auth/register', {
      json: { email: 'ileso@contract.test', password: PASSWORD },
    })

    expect(response.status).toBe(201)
  })
})
