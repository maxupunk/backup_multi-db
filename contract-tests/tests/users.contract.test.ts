/**
 * Lote 2.2 — `GET /api/users` e `PATCH /api/users/:id/status`.
 *
 * Sao as duas unicas rotas administrativas puras da API, e por isso o lugar
 * onde o contrato de autorizacao fica mais visivel: quem nao e' admin leva
 * 403, nao 404 nem 401.
 */

import { describe, expect, it } from 'vitest'
import { loadConfig } from '../src/config.ts'
import { createPendingUser } from '../src/factory.ts'
import { expectGolden } from '../src/golden.ts'
import { as, expectStatus, json, state, unauth } from '../src/session.ts'

const TARGET = loadConfig().target

interface UsersPage {
  meta: { total: number; perPage: number; currentPage: number; lastPage: number }
  data: Array<{
    id: number
    email: string
    fullName: string | null
    isActive: boolean
    isAdmin: boolean
    createdAt: string
  }>
}

describe('GET /api/users', () => {
  it('lista usuarios para o admin', async () => {
    const response = expectStatus(await as('admin').get('/api/users'), 200)

    const body = json<UsersPage>(response)
    expect(body.data.length).toBeGreaterThan(0)
    expect(body.meta.total).toBeGreaterThanOrEqual(3)

    // `notComparedPaths` tira a **lista** do corpo gravado, mas o `shape`
    // continua sendo comparado — o formato do item de usuario e' contrato.
    // O motivo esta' no ACHADO de ordenacao no fim deste arquivo: a ordem dos
    // usuarios nao e' estavel, entao gravar a lista faria o golden mudar
    // sozinho a cada execucao.
    expectGolden('users/index', response, { as: 'admin', notComparedPaths: ['data'] })
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
    // alterna, e nao apenas ativa — o backend precisa reproduzir isso.
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

/**
 * ACHADO — `GET /api/users` ordena sem critério de desempate.
 *
 * A query é `orderBy('createdAt', 'desc')` e o `created_at` do SQLite tem
 * resolução de segundos. Usuários criados dentro do mesmo segundo — o caso
 * normal num cadastro em lote, ou nesta própria suíte — ficam empatados, e o
 * SQLite devolve empates em ordem arbitrária, que pode mudar entre consultas
 * idênticas.
 *
 * A consequência é a **paginação sem garantia**: se a ordem mudar entre a
 * consulta da página 1 e a da página 2, um usuário pode aparecer nas duas ou
 * em nenhuma, e quem varre a lista paginando processa alguém duas vezes e pula
 * outro.
 *
 * Hoje a varredura funciona — o segundo teste abaixo prova isso — porque o
 * SQLite tende a devolver empates na ordem do rowid. Mas isso é acidente de
 * implementação, não garantia: outro motor, outro plano de query ou um índice
 * novo mudam o resultado sem aviso. O primeiro teste prova que os empates
 * existem de verdade; o segundo é o canário que grita se o acidente parar de
 * ser favorável.
 *
 * A correção é um desempate estável (`orderBy('createdAt','desc').orderBy('id','desc')`).
 * O backend não deveria copiar o defeito sem que a escolha seja consciente.
 *
 * O mesmo padrão aparece em outras listagens paginadas do projeto; vale
 * revisar todas quando esta for decidida.
 */
describe('ACHADO: ordenacao de /api/users nao tem desempate', () => {
  it.skipIf(TARGET === 'roco')('varios usuarios compartilham o mesmo createdAt', async () => {
    const body = json<UsersPage>(
      await as('admin').get('/api/users', { query: { limit: 100 } })
    )

    const porTimestamp = new Map<string, number>()
    for (const user of body.data) {
      porTimestamp.set(user.createdAt, (porTimestamp.get(user.createdAt) ?? 0) + 1)
    }

    const maiorEmpate = Math.max(...porTimestamp.values())
    expect(
      maiorEmpate,
      'nenhum empate de createdAt: o achado pode ter sido corrigido — revise este teste'
    ).toBeGreaterThan(1)
  })

  it.skipIf(TARGET !== 'roco')(
    'backend ordena com desempate estavel (sem empates de createdAt)',
    async () => {
      const body = json<UsersPage>(
        await as('admin').get('/api/users', { query: { limit: 100 } })
      )

      const porTimestamp = new Map<string, number>()
      for (const user of body.data) {
        porTimestamp.set(user.createdAt, (porTimestamp.get(user.createdAt) ?? 0) + 1)
      }

      const maiorEmpate = Math.max(...porTimestamp.values())
      expect(
        maiorEmpate,
        'houve empate de createdAt: o desempate por id nao esta funcionando'
      ).toBe(1)
    }
  )

  it('a lista completa nao perde nem duplica usuarios ao paginar', async () => {
    // Este e' o efeito que realmente importa. Hoje passa na maioria das vezes;
    // se um dia falhar de forma intermitente, e' o achado se manifestando, e
    // nao um teste ruim.
    const primeira = json<UsersPage>(
      await as('admin').get('/api/users', { query: { page: 1, limit: 5 } })
    )

    // `lastPage` tem que vir da **mesma** paginacao que se esta' varrendo. Ler
    // de uma consulta com outro `limit` daria 1 e a varredura pararia na
    // primeira pagina, inventando uma falha que nao existe.
    const paginados: number[] = [...primeira.data.map((user) => user.id)]
    for (let page = 2; page <= primeira.meta.lastPage; page++) {
      const parte = json<UsersPage>(
        await as('admin').get('/api/users', { query: { page, limit: 5 } })
      )
      paginados.push(...parte.data.map((user) => user.id))
    }

    expect(new Set(paginados).size).toBe(primeira.meta.total)
  })
})
