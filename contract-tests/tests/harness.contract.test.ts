/**
 * Prova de que o harness esta' realmente montado.
 *
 * Nao sao testes de endpoint (esses sao a Fase 2). Sao os testes que garantem
 * que o resto da suite mede o que diz medir: que `as('admin')` de fato
 * autentica, que `unauth()` de fato nao autentica, e que os papeis semeados
 * tem os privilegios que a Fase 2 vai assumir que eles tem.
 *
 * Sem isto, uma falha de seed apareceria la' na frente como dezenas de testes
 * de negocio quebrados, e o tempo iria embora procurando no lugar errado.
 */

import { describe, expect, it } from 'vitest'
import { expectGolden } from '../src/golden.ts'
import { as, expectStatus, json, state, unauth, withBogusToken } from '../src/session.ts'

describe('autenticacao do harness', () => {
  it('recusa rota protegida sem token', async () => {
    const response = await unauth().get('/api/auth/me')
    expect(response.status).toBe(401)
  })

  it('recusa token bem-formado que nao existe no banco', async () => {
    // 401 e nao 500: o formato do token e' valido (`oat_<id>.<secret>`), so' o
    // registro nao existe. Confundir "malformado" com "inexistente" e' o erro
    // classico dessa camada, e o back-roco vai ter que acertar os dois.
    const response = await withBogusToken().get('/api/auth/me')
    expect(response.status).toBe(401)
  })

  it('aceita o token do admin semeado', async () => {
    const response = expectStatus(await as('admin').get('/api/auth/me'), 200)

    const body = json<{ data: { email: string; isAdmin: boolean; isActive: boolean } }>(response)
    expect(body.data.email).toBe(state().users.admin.email)
    expect(body.data.isAdmin).toBe(true)
    expect(body.data.isActive).toBe(true)

    expectGolden('auth/me-admin', response, { as: 'admin' })
  })

  it('aceita o token do usuario comum, que nao e admin', async () => {
    const body = json<{ data: { isAdmin: boolean; isActive: boolean } }>(
      expectStatus(await as('member').get('/api/auth/me'), 200)
    )

    expect(body.data.isAdmin).toBe(false)
    expect(body.data.isActive).toBe(true)
  })
})

describe('papeis semeados', () => {
  it('da ao admin acesso a rota administrativa', async () => {
    expectStatus(await as('admin').get('/api/users'), 200)
  })

  it('nega ao usuario comum a mesma rota', async () => {
    // E' isto que torna os testes de 403 da Fase 2 possiveis: se o `member`
    // fosse admin por engano do seed, todo teste de autorizacao passaria a
    // medir nada.
    const response = await as('member').get('/api/users')
    expect(response.status).toBe(403)
  })

  it('impede o usuario inativo de logar', async () => {
    const user = state().users.inactive
    const response = await unauth().post('/api/auth/login', {
      json: { email: user.email, password: user.password },
    })

    expect(response.status).toBe(401)
    expect(json<{ success: boolean }>(response).success).toBe(false)
  })
})

describe('seed', () => {
  it('deixou os recursos que a Fase 2 vai consumir', async () => {
    const seeded = state()

    expect(seeded.users.admin.id).toBeTypeOf('number')
    expect(seeded.users.member.id).toBeTypeOf('number')
    expect(seeded.users.inactive.id).toBeTypeOf('number')
    expect(seeded.connections.mysql).toBeTypeOf('number')
    expect(seeded.connections.postgres).toBeTypeOf('number')
    expect(seeded.storages.local).toBeTypeOf('number')
    expect(seeded.storages.minio).toBeTypeOf('number')
  })

  it('as conexoes semeadas aparecem na listagem', async () => {
    const body = json<{ data: { data: Array<{ id: number }> } }>(
      expectStatus(await as('admin').get('/api/connections'), 200)
    )

    const ids = body.data.data.map((item) => item.id)
    expect(ids).toContain(state().connections.mysql)
    expect(ids).toContain(state().connections.postgres)
  })
})
