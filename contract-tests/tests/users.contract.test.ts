/**
 * Lote 2.2 — `GET /api/users` e `PATCH /api/users/:id/status`.
 *
 * Sao as duas unicas rotas administrativas puras da API, e por isso o lugar
 * onde o contrato de autorizacao fica mais visivel: quem nao e' admin leva
 * 403, nao 404 nem 401.
 */

import { describe, expect, it } from 'vitest'
import { createPendingUser } from '../src/factory.ts'
import { expectGolden } from '../src/golden.ts'
import { as, expectStatus, json, state, unauth } from '../src/session.ts'

interface UsersPage {
  meta: { total: number; perPage: number; currentPage: number; lastPage: number }
  data: Array<{ id: number; email: string; fullName: string | null; isActive: boolean; isAdmin: boolean }>
}

describe('GET /api/users', () => {
  it('lista usuarios para o admin', async () => {
    const response = expectStatus(await as('admin').get('/api/users'), 200)

    const body = json<UsersPage>(response)
    expect(body.data.length).toBeGreaterThan(0)
    expect(body.meta.total).toBeGreaterThanOrEqual(3)

    expectGolden('users/index', response, { as: 'admin' })
  })

  it('nunca serializa o hash da senha', async () => {
    // A rota lista **todos** os usuarios. Um vazamento aqui expoe o hash de
    // todo mundo de uma vez, nao so' o de quem chamou.
    const response = await as('admin').get('/api/users')
    expect(response.text).not.toContain('$scrypt$')
    expect(response.text).not.toContain('password')
  })

  it('pagina', async () => {
    const body = json<UsersPage>(
      expectStatus(await as('admin').get('/api/users', { query: { page: 1, limit: 2 } }), 200)
    )

    expect(body.data.length).toBeLessThanOrEqual(2)
    expect(body.meta.perPage).toBe(2)
    expect(body.meta.currentPage).toBe(1)
  })

  it('a segunda pagina traz usuarios diferentes da primeira', async () => {
    // Um paginador quebrado que ignora o `page` passaria no teste acima e
    // falharia neste.
    const first = json<UsersPage>(
      await as('admin').get('/api/users', { query: { page: 1, limit: 1 } })
    )
    const second = json<UsersPage>(
      await as('admin').get('/api/users', { query: { page: 2, limit: 1 } })
    )

    expect(first.data[0]!.id).not.toBe(second.data[0]!.id)
  })

  it('filtra por status ativo', async () => {
    await createPendingUser('filtro-inativo@contract.test')

    const inativos = json<UsersPage>(
      expectStatus(await as('admin').get('/api/users', { query: { active: 'false', limit: 100 } }), 200)
    )
    expect(inativos.data.length).toBeGreaterThan(0)
    expect(inativos.data.every((user) => user.isActive === false)).toBe(true)

    const ativos = json<UsersPage>(
      expectStatus(await as('admin').get('/api/users', { query: { active: 'true', limit: 100 } }), 200)
    )
    expect(ativos.data.every((user) => user.isActive === true)).toBe(true)
  })

  it('nega para usuario comum', async () => {
    const response = expectStatus(await as('member').get('/api/users'), 403)
    expect(json<{ success: boolean }>(response).success).toBe(false)

    expectGolden('users/index-forbidden', response, { as: 'member' })
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/users'), 401)
  })
})

describe('PATCH /api/users/:id/status', () => {
  it('alterna o status do usuario', async () => {
    const user = await createPendingUser('toggle@contract.test')

    const ativado = expectStatus(await as('admin').patch(`/api/users/${user.id}/status`), 200)
    expect(json<{ data: { isActive: boolean } }>(ativado).data.isActive).toBe(true)

    expectGolden('users/toggle-status', ativado, { as: 'admin' })

    // Segundo toggle volta ao estado anterior. E' o que prova que a rota
    // alterna, e nao apenas ativa — o back-roco precisa reproduzir isso.
    const desativado = expectStatus(await as('admin').patch(`/api/users/${user.id}/status`), 200)
    expect(json<{ data: { isActive: boolean } }>(desativado).data.isActive).toBe(false)
  })

  it('impede o admin de desativar a si mesmo', async () => {
    // Sem essa trava, um admin desavisado se tranca para fora do sistema e nao
    // ha' outro caminho de recuperacao pela API.
    const response = expectStatus(
      await as('admin').patch(`/api/users/${state().users.admin.id}/status`),
      400
    )
    expect(json<{ success: boolean }>(response).success).toBe(false)

    expectGolden('users/toggle-status-self', response, { as: 'admin' })
  })

  it('responde 404 para usuario inexistente', async () => {
    expectStatus(await as('admin').patch('/api/users/99999999/status'), 404)
  })

  it('nega para usuario comum', async () => {
    const alvo = state().users.inactive.id
    expectStatus(await as('member').patch(`/api/users/${alvo}/status`), 403)
  })

  it('nega sem autenticacao', async () => {
    const alvo = state().users.inactive.id
    expectStatus(await unauth().patch(`/api/users/${alvo}/status`), 401)
  })

  it('a negativa por autorizacao nao altera nada', async () => {
    // Um 403 que ja' escreveu no banco e' pior que um 500: falha em silencio.
    const alvo = state().users.inactive.id
    const antes = json<UsersPage>(await as('admin').get('/api/users', { query: { limit: 100 } }))
    const estadoAntes = antes.data.find((user) => user.id === alvo)!.isActive

    await as('member').patch(`/api/users/${alvo}/status`)

    const depois = json<UsersPage>(await as('admin').get('/api/users', { query: { limit: 100 } }))
    expect(depois.data.find((user) => user.id === alvo)!.isActive).toBe(estadoAntes)
  })
})
