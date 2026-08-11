/**
 * Lote 2.2 — `/api/audit-logs`.
 *
 * A auditoria e' o unico lugar da API que registra o que **outros** endpoints
 * fizeram. Por isso o teste mais valioso deste arquivo nao e' nenhum dos de
 * listagem: e' o de efeito colateral, que cria uma conexao e cobra o registro
 * correspondente. Um porte que devolva as rotas de auditoria certinhas mas
 * pare de gravar os eventos passaria em todos os outros.
 */

import { describe, expect, it } from 'vitest'
import { createConnection } from '../src/factory.ts'
import { expectGolden } from '../src/golden.ts'
import { as, expectStatus, json, unauth } from '../src/session.ts'

interface AuditPage {
  success: boolean
  data: Array<{
    id: number
    action: string
    actionDescription: string
    entityType: string
    entityId: number | null
    entityName: string | null
    status: string
    createdAt: string
  }>
  meta: { total: number; perPage: number; currentPage: number }
}

describe('GET /api/audit-logs', () => {
  it('lista os registros para um usuario autenticado', async () => {
    // O seed cria duas conexoes, entao a auditoria nunca esta' vazia aqui —
    // e um array vazio seria reportado pelo matcher como `unverified-array`.
    const response = expectStatus(await as('admin').get('/api/audit-logs'), 200)

    const body = json<AuditPage>(response)
    expect(body.success).toBe(true)
    expect(body.data.length).toBeGreaterThan(0)

    expectGolden('audit-logs/index', response, { as: 'admin' })
  })

  it('ordena do mais recente para o mais antigo', async () => {
    const body = json<AuditPage>(await as('admin').get('/api/audit-logs', { query: { limit: 50 } }))
    const timestamps = body.data.map((log) => Date.parse(log.createdAt))

    for (let index = 1; index < timestamps.length; index++) {
      expect(timestamps[index - 1]!).toBeGreaterThanOrEqual(timestamps[index]!)
    }
  })

  it('filtra por action', async () => {
    const body = json<AuditPage>(
      expectStatus(
        await as('admin').get('/api/audit-logs', { query: { action: 'connection.created' } }),
        200
      )
    )

    expect(body.data.length).toBeGreaterThan(0)
    expect(body.data.every((log) => log.action === 'connection.created')).toBe(true)
  })

  it('filtra por entityType', async () => {
    const body = json<AuditPage>(
      expectStatus(await as('admin').get('/api/audit-logs', { query: { entityType: 'connection' } }), 200)
    )

    expect(body.data.every((log) => log.entityType === 'connection')).toBe(true)
  })

  it('filtra por status', async () => {
    const body = json<AuditPage>(
      expectStatus(await as('admin').get('/api/audit-logs', { query: { status: 'success' } }), 200)
    )

    expect(body.data.every((log) => log.status === 'success')).toBe(true)
  })

  it('devolve lista vazia para filtro que nao casa com nada', async () => {
    // Filtro sem resultado tem que dar 200 com lista vazia, nao 404: o
    // recurso "lista de logs" existe, apenas nao tem itens sob esse recorte.
    //
    // O filtro e' por `entityId` inexistente, e nao por uma `action` que
    // "ninguem usa": os testes do lote 2.6 alteram a politica de retencao e
    // geram `settings.updated`, entao qualquer action real pode aparecer
    // dependendo da ordem dos arquivos.
    const body = json<AuditPage>(
      expectStatus(await as('admin').get('/api/audit-logs', { query: { entityId: 99999999 } }), 200)
    )

    expect(body.data).toEqual([])
  })

  it('pagina e limita a 100 por pagina', async () => {
    const body = json<AuditPage>(
      expectStatus(await as('admin').get('/api/audit-logs', { query: { page: 1, limit: 5 } }), 200)
    )
    expect(body.meta.perPage).toBe(5)
    expect(body.data.length).toBeLessThanOrEqual(5)

    // O controller faz `Math.min(limit, 100)`. Pedir mais nao pode virar uma
    // varredura da tabela inteira.
    const teto = json<AuditPage>(
      expectStatus(await as('admin').get('/api/audit-logs', { query: { limit: 5000 } }), 200)
    )
    expect(teto.meta.perPage).toBe(100)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/audit-logs'), 401)
  })

  it('permite usuario comum', async () => {
    // A auditoria nao e' admin-only hoje. Se isso for mudar no porte, e' uma
    // decisao de produto — e este teste e' o que forca a conversa em vez de
    // deixar a mudanca passar despercebida.
    expectStatus(await as('member').get('/api/audit-logs'), 200)
  })
})

describe('GET /api/audit-logs/stats', () => {
  it('devolve os agregados', async () => {
    const response = expectStatus(await as('admin').get('/api/audit-logs/stats'), 200)

    const body = json<{
      data: {
        total: number
        today: number
        lastWeek: number
        byStatus: { success: number; failure: number }
        byAction: Array<{ action: string; description: string; count: number }>
      }
    }>(response)

    expect(body.data.total).toBeGreaterThan(0)
    expect(body.data.byStatus.success + body.data.byStatus.failure).toBeLessThanOrEqual(
      body.data.total
    )
    expect(body.data.byAction.length).toBeGreaterThan(0)

    expectGolden('audit-logs/stats', response, { as: 'admin' })
  })

  it('nao e confundida com a rota de :id', async () => {
    // `/audit-logs/stats` e `/audit-logs/:id` competem pelo mesmo formato de
    // URL. No Adonis a ordem de registro resolve; num router que ordene
    // diferente, `stats` cairia no `show` e viraria 404.
    const response = await as('admin').get('/api/audit-logs/stats')
    expect(response.route).toBe('GET /api/audit-logs/stats')
    expect(response.status).toBe(200)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/audit-logs/stats'), 401)
  })
})

describe('GET /api/audit-logs/:id', () => {
  it('devolve um registro pelo id', async () => {
    const lista = json<AuditPage>(await as('admin').get('/api/audit-logs', { query: { limit: 1 } }))
    const alvo = lista.data[0]!

    const response = expectStatus(await as('admin').get(`/api/audit-logs/${alvo.id}`), 200)
    const body = json<{ data: { id: number; userAgent: string | null } }>(response)

    expect(body.data.id).toBe(alvo.id)
    // `userAgent` so' aparece no detalhe, nunca na listagem.
    expect(body.data).toHaveProperty('userAgent')

    expectGolden('audit-logs/show', response, { as: 'admin' })
  })

  it('responde 404 para id inexistente', async () => {
    const response = expectStatus(await as('admin').get('/api/audit-logs/99999999'), 404)
    expect(json<{ success: boolean }>(response).success).toBe(false)

    expectGolden('audit-logs/show-not-found', response, { as: 'admin' })
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/audit-logs/1'), 401)
  })
})

describe('efeito colateral: acoes geram registro de auditoria', () => {
  it('criar uma conexao grava connection.created com o nome da entidade', async () => {
    const connection = await createConnection({ name: 'Auditada Pelo Contrato' })

    const body = json<AuditPage>(
      await as('admin').get('/api/audit-logs', {
        query: { action: 'connection.created', limit: 50 },
      })
    )

    const registro = body.data.find((log) => log.entityId === connection.id)
    expect(registro, 'nenhum audit log para a conexao recem-criada').toBeDefined()
    expect(registro!.entityType).toBe('connection')
    expect(registro!.entityName).toBe(connection.name)
    expect(registro!.status).toBe('success')
    // A descricao legivel e' derivada da action pelo model, nao gravada no
    // banco. O backend precisa da mesma tabela de traducao.
    expect(registro!.actionDescription).toBeTruthy()
  })

  it('apagar uma conexao grava connection.deleted', async () => {
    const connection = await createConnection({ name: 'Auditada Ao Apagar' })
    expectStatus(await as('admin').delete(`/api/connections/${connection.id}`), 200)

    const body = json<AuditPage>(
      await as('admin').get('/api/audit-logs', {
        query: { action: 'connection.deleted', limit: 50 },
      })
    )

    expect(body.data.some((log) => log.entityId === connection.id)).toBe(true)
  })
})
