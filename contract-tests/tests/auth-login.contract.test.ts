/**
 * Lote 2.1 — `POST /api/auth/login`.
 *
 * Orcamento de requisicoes: o limiter de `auth` da' 5 por minuto por
 * IP+e-mail, e o seed ja' gastou 1 de `member@contract.test`. Os testes abaixo
 * que usam esse e-mail cabem nas 4 restantes; os demais usam e-mails proprios.
 */

import { describe, expect, it } from 'vitest'
import { expectGolden } from '../src/golden.ts'
import { expectStatus, json, state, unauth } from '../src/session.ts'

interface LoginBody {
  success: boolean
  data: {
    type: string
    token: string
    user: { id: number; email: string; fullName: string | null; isActive: boolean; isAdmin: boolean }
  }
}

describe('POST /api/auth/login — caminho feliz', () => {
  it('devolve token bearer e o usuario', async () => {
    const user = state().users.admin
    const response = expectStatus(
      await unauth().post('/api/auth/login', { json: { email: user.email, password: user.password } }),
      200
    )

    const body = json<LoginBody>(response)
    expect(body.success).toBe(true)
    expect(body.data.type).toBe('bearer')
    expect(body.data.user.email).toBe(user.email)
    expect(body.data.user.isAdmin).toBe(true)

    // Formato do token opaco (`oat_<base64url(id)>.<base64url(secret)>`).
    // A decisao D1 preservou esse formato para compatibilidade de sessoes.
    expect(body.data.token).toMatch(/^oat_[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/)

    expectGolden('auth/login-ok', response)
  })

  it('emite um token novo a cada login, sem invalidar o anterior', async () => {
    const user = state().users.admin
    const first = json<LoginBody>(
      expectStatus(
        await unauth().post('/api/auth/login', {
          json: { email: user.email, password: user.password },
        }),
        200
      )
    )

    expect(first.data.token).not.toBe(user.token)

    // O token do seed continua valendo. Isso e' contrato de verdade: se logar
    // derrubasse a sessao anterior, abrir a aplicacao em duas abas quebraria.
    const response = await unauth().get('/api/auth/me', { token: user.token })
    expect(response.status).toBe(200)
  })

  it('nao devolve a senha nem o hash em lugar nenhum', async () => {
    // Usa o `member` e nao o `admin` de proposito: cada login consome uma das
    // 5 fichas por minuto do e-mail, e os dois testes acima ja' gastaram duas
    // do admin. Espalhar mantem folga para quem vier depois.
    const user = state().users.member
    const response = await unauth().post('/api/auth/login', {
      json: { email: user.email, password: user.password },
    })

    expect(response.text).not.toContain(user.password)
    expect(response.text).not.toContain('$scrypt$')
    expect(response.text).not.toContain('password')
  })
})

describe('POST /api/auth/login — credenciais invalidas', () => {
  it('recusa senha errada com 400 e o shape de erro do framework', async () => {
    const response = expectStatus(
      await unauth().post('/api/auth/login', {
        json: { email: 'senha-errada@contract.test', password: 'senha-que-nao-e-a-certa' },
      }),
      400
    )

    // Repare: **400**, nao 401, e o corpo vem no shape `{ errors: [...] }` do
    // framework, sem o `success` que os controllers usam. Sao duas familias de
    // erro convivendo na mesma API — ver a nota sobre a decisao D9 no roadmap.
    const body = json<{ errors: Array<{ message: string }> }>(response)
    expect(body.errors.length).toBeGreaterThan(0)

    expectGolden('auth/login-invalid-credentials', response)
  })

  it('responde igual para usuario inexistente e senha errada', async () => {
    // Diferenciar os dois casos entrega a um atacante quais e-mails existem.
    // O `verifyCredentials` do Adonis ja' unifica; o back-roco tem que
    // unificar tambem, e este teste e' o que trava esse comportamento.
    const inexistente = await unauth().post('/api/auth/login', {
      json: { email: 'ninguem@contract.test', password: 'qualquer-senha' },
    })
    const senhaErrada = await unauth().post('/api/auth/login', {
      json: { email: 'outro-inexistente@contract.test', password: 'qualquer-senha' },
    })

    expect(inexistente.status).toBe(senhaErrada.status)
    expect(json<{ errors?: unknown }>(inexistente)).toEqual(json(senhaErrada))
  })

  it('recusa usuario inativo mesmo com a senha certa', async () => {
    const user = state().users.inactive
    const response = expectStatus(
      await unauth().post('/api/auth/login', {
        json: { email: user.email, password: user.password },
      }),
      401
    )

    const body = json<{ success: boolean; message: string }>(response)
    expect(body.success).toBe(false)

    expectGolden('auth/login-inactive', response)
  })

  it('recusa payload sem e-mail', async () => {
    const response = expectStatus(await unauth().post('/api/auth/login', { json: {} }), 422)

    const body = json<{ errors: Array<{ field: string }> }>(response)
    expect(body.errors.map((error) => error.field).sort()).toEqual(['email', 'password'])
  })
})

describe('POST /api/auth/login — rate limit', () => {
  it('bloqueia na sexta tentativa do mesmo e-mail', async () => {
    const payload = { email: 'login-flood@contract.test', password: 'qualquer-senha' }
    const statuses: number[] = []

    for (let attempt = 0; attempt < 6; attempt++) {
      const response = await unauth().post('/api/auth/login', { json: payload })
      statuses.push(response.status)
    }

    expect(statuses[5]).toBe(429)
    // As cinco primeiras falham por credencial, nao por limite — se alguma ja'
    // viesse 429, o limite estaria menor que o configurado.
    expect(statuses.slice(0, 5).every((status) => status !== 429)).toBe(true)
  })

  it('devolve 429 com o corpo e os cabecalhos de limite', async () => {
    const payload = { email: 'login-flood-2@contract.test', password: 'qualquer-senha' }
    let blocked = null

    for (let attempt = 0; attempt < 6; attempt++) {
      const response = await unauth().post('/api/auth/login', { json: payload })
      if (response.status === 429) {
        blocked = response
        break
      }
    }

    expect(blocked, 'o limiter nao bloqueou em 6 tentativas').not.toBeNull()
    expect(blocked!.headers['retry-after']).toBeDefined()
    expectGolden('auth/login-rate-limited', blocked!, {
      assertHeaders: ['x-ratelimit-limit'],
    })
  })
})
