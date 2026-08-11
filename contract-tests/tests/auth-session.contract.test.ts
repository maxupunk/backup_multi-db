/**
 * Lote 2.1 — `GET /api/auth/me` e `POST /api/auth/logout`.
 *
 * O logout revoga o token que o apresentou. Por isso nenhum teste aqui usa os
 * tokens do seed: revogar um deles derrubaria todos os arquivos de teste que
 * rodarem depois, e a ordem entre arquivos do vitest nao e' contrato.
 * Cada caso destrutivo cria o proprio usuario descartavel.
 */

import { describe, expect, it } from 'vitest'
import { createActivatedUser } from '../src/factory.ts'
import { expectGolden } from '../src/golden.ts'
import { as, expectStatus, json, state, unauth, withBogusToken } from '../src/session.ts'

describe('GET /api/auth/me', () => {
  it('devolve o usuario do token', async () => {
    const response = expectStatus(await as('admin').get('/api/auth/me'), 200)

    const body = json<{
      success: boolean
      data: { id: number; email: string; isAdmin: boolean; isActive: boolean; createdAt: string }
    }>(response)

    expect(body.data.email).toBe(state().users.admin.email)
    expect(body.data.isAdmin).toBe(true)

    expectGolden('auth/me-admin', response, { as: 'admin' })
  })

  it('nao vaza o hash da senha', async () => {
    // `password` e' `serializeAs: null` no model. Se o backend esquecer o
    // equivalente, o hash scrypt de todo mundo passa a sair no `/me`.
    const response = await as('admin').get('/api/auth/me')
    expect(response.text).not.toContain('$scrypt$')
    expect(response.text).not.toContain('password')
  })

  it('recusa requisicao sem token', async () => {
    expectStatus(await unauth().get('/api/auth/me'), 401)
  })

  it('recusa token com formato invalido', async () => {
    const response = await unauth().get('/api/auth/me', { token: 'isso-nao-e-um-token' })
    expect(response.status).toBe(401)
  })

  it('recusa token bem-formado que nao existe', async () => {
    // 401 e nao 500: o formato e' valido (`oat_<id>.<secret>`), so' o registro
    // nao existe. Confundir "malformado" com "inexistente" e' o erro classico
    // dessa camada, e o backend vai ter que acertar os dois.
    expectStatus(await withBogusToken().get('/api/auth/me'), 401)
  })

  it('ACEITA o token de um usuario ja desativado — comportamento atual', async () => {
    const user = await createActivatedUser('desativado@contract.test')

    // Segundo toggle: volta o usuario para inativo com a sessao ja' aberta.
    expectStatus(await as('admin').patch(`/api/users/${user.id}/status`), 200)

    const response = expectStatus(await unauth().get('/api/auth/me', { token: user.token }), 200)

    // ATENCAO — isto NAO e' o comportamento que a maioria esperaria, e o teste
    // existe para deixar isso explicito em vez de descoberto depois.
    //
    // O `auth` do Adonis valida o **token**, nao o `isActive` do usuario.
    // Desativar alguem no `PATCH /api/users/:id/status` nao revoga as sessoes
    // dele: quem ja' estava logado continua logado ate' o token expirar
    // (`AUTH_ACCESS_TOKEN_EXPIRES_IN`, 7 dias por padrao). So' o **login** novo
    // e' barrado.
    //
    // O teste fixa o comportamento real para que o porte o reproduza. Se o
    // time decidir que desativar deve derrubar a sessao, e' mudanca de produto
    // — mude aqui primeiro, e a Fase 2 vira a especificacao da correcao.
    const body = json<{ data: { isActive: boolean } }>(response)
    expect(body.data.isActive).toBe(false)

    expectGolden('auth/me-deactivated', response)
  })
})

describe('POST /api/auth/logout', () => {
  it('revoga o token que o apresentou', async () => {
    const user = await createActivatedUser('logout@contract.test')

    const response = expectStatus(
      await unauth().post('/api/auth/logout', { token: user.token }),
      200
    )
    expect(json<{ success: boolean }>(response).success).toBe(true)

    expectGolden('auth/logout', response)

    // O que de fato importa: depois do logout o token para de valer.
    const after = await unauth().get('/api/auth/me', { token: user.token })
    expect(after.status).toBe(401)
  })

  it('nao derruba as outras sessoes do mesmo usuario', async () => {
    const user = await createActivatedUser('logout-multi@contract.test')

    const second = json<{ data: { token: string } }>(
      expectStatus(
        await unauth().post('/api/auth/login', {
          json: { email: user.email, password: user.password },
        }),
        200
      )
    )

    expectStatus(await unauth().post('/api/auth/logout', { token: user.token }), 200)

    // Revogar uma sessao nao pode derrubar as outras: e' a diferenca entre
    // "sair deste navegador" e "sair de todos".
    expectStatus(await unauth().get('/api/auth/me', { token: second.data.token }), 200)
  })

  it('recusa logout sem token', async () => {
    expectStatus(await unauth().post('/api/auth/logout'), 401)
  })

  it('recusa o mesmo token duas vezes', async () => {
    const user = await createActivatedUser('logout-duplo@contract.test')

    expectStatus(await unauth().post('/api/auth/logout', { token: user.token }), 200)
    expectStatus(await unauth().post('/api/auth/logout', { token: user.token }), 401)
  })
})
