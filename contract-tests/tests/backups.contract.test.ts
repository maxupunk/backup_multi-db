/**
 * Lote 2.4 — `/api/backups` (6 rotas).
 *
 * O caminho feliz depende de `mysqldump`/`pg_dump` no PATH do processo que
 * roda o backend. Onde eles faltam, os testes correspondentes são pulados — e
 * o globalSetup avisa em letras grandes, porque uma suíte verde que não gerou
 * nenhum backup diria muito pouco sobre estas rotas.
 *
 * O resto (404, 401, validação, filtros) é determinístico e roda sempre.
 */

import { describe, expect, it } from 'vitest'
import { createConnection } from '../src/factory.ts'
import { MYSQL } from '../src/fixtures.ts'
import { expectGolden } from '../src/golden.ts'
import { as, can, expectStatus, json, state, unauth } from '../src/session.ts'

interface BackupItem {
  id: number
  status: string
  fileName: string | null
  fileSize: number | null
  databaseName: string
  createdAt: string
}

const canDump = () => can('mysql') && can('mysqldump')

/** Cria um backup de verdade e devolve o id do primeiro gerado. */
async function createBackup(): Promise<number> {
  const connection = await createConnection({ name: `Backup Real ${Date.parse('2026-08-09')}` })

  const response = await as('admin').post(`/api/connections/${connection.id}/backup`)
  expectStatus(response, 200)

  const body = json<{ data: { backups: Array<{ backupId: number }> } }>(response)
  return body.data.backups[0]!.backupId
}

describe('GET /api/backups', () => {
  it('lista com paginacao', async () => {
    const response = expectStatus(await as('admin').get('/api/backups'), 200)
    expect(json<{ success: boolean }>(response).success).toBe(true)

    expectGolden('backups/index', response, {
      as: 'admin',
      // A lista pode estar vazia quando nao ha' `mysqldump`; o formato do item
      // e' fixado pelos testes de caminho feliz abaixo.
      compare: { ignorePaths: ['data', 'data.data'] },
    })
  })

  it('aceita filtros de status, conexao e database', async () => {
    const queries: Array<Record<string, string | number>> = [
      { status: 'completed' },
      { connectionId: state().connections.mysql ?? 0 },
      { databaseName: MYSQL.database },
      { page: 1, limit: 5 },
    ]

    for (const query of queries) {
      expectStatus(await as('admin').get('/api/backups', { query }), 200)
    }
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/backups'), 401)
  })
})

describe('GET /api/backups/:id', () => {
  it('responde 404 para id inexistente', async () => {
    const response = expectStatus(await as('admin').get('/api/backups/99999999'), 404)
    expect(json<{ success: boolean }>(response).success).toBe(false)

    expectGolden('backups/show-not-found', response, { as: 'admin' })
  })

  it.skipIf(!canDump())('devolve o backup recem-criado', async () => {
    const id = await createBackup()

    const response = expectStatus(await as('admin').get(`/api/backups/${id}`), 200)
    const body = json<{ data: BackupItem }>(response)

    expect(body.data.id).toBe(id)
    expect(body.data.status).toBe('completed')
    expect(body.data.fileName).toBeTruthy()

    expectGolden('backups/show', response, { as: 'admin' })
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/backups/1'), 401)
  })
})

describe('GET /api/backups/:id/download', () => {
  it('responde 404 para id inexistente', async () => {
    expectStatus(await as('admin').get('/api/backups/99999999/download'), 404)
  })

  it.skipIf(!canDump())('entrega o arquivo com Content-Disposition', async () => {
    const id = await createBackup()

    const response = await as('admin').get(`/api/backups/${id}/download`, { raw: true })

    expectStatus(response, 200)
    // Sem `Content-Disposition: attachment` o navegador tenta renderizar o
    // dump em vez de baixa-lo.
    expect(response.headers['content-disposition']).toContain('attachment')
    expect((response.body as Buffer).length).toBeGreaterThan(0)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/backups/1/download'), 401)
  })
})

describe('DELETE /api/backups/:id', () => {
  it('responde 404 para id inexistente', async () => {
    expectStatus(await as('admin').delete('/api/backups/99999999'), 404)
  })

  it.skipIf(!canDump())('remove o backup', async () => {
    const id = await createBackup()

    expectStatus(await as('admin').delete(`/api/backups/${id}`), 200)
    expectStatus(await as('admin').get(`/api/backups/${id}`), 404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().delete('/api/backups/1'), 401)
  })
})

describe('POST /api/backups/:id/restore', () => {
  it('responde 404 para id inexistente', async () => {
    expectStatus(
      await as('admin').post('/api/backups/99999999/restore', {
        json: { targetConnectionId: state().connections.mysql },
      }),
      404
    )
  })

  it.skipIf(!canDump())('recusa restore sem alvo valido', async () => {
    const id = await createBackup()

    const response = await as('admin').post(`/api/backups/${id}/restore`, {
      json: { targetConnectionId: 99999999 },
    })

    // Conexao de destino inexistente: 404 ou 422, mas nunca 200 — restaurar
    // "para lugar nenhum" tem que falhar alto.
    expect([404, 422]).toContain(response.status)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().post('/api/backups/1/restore', { json: {} }), 401)
  })
})

describe('POST /api/backups/import', () => {
  it('recusa requisicao sem arquivo', async () => {
    const response = await as('admin').post('/api/backups/import', { json: {} })

    expect(response.status).toBe(422)
    expectGolden('backups/import-no-file', response, { as: 'admin' })
  })

  it('recusa extensao invalida', async () => {
    // Multipart montado a mao: e' o unico endpoint da API que recebe arquivo,
    // e o `undici` nao precisa de nenhuma biblioteca extra para isso.
    const boundary = '----contract-boundary-0000'
    const body = [
      `--${boundary}`,
      'Content-Disposition: form-data; name="file"; filename="malicioso.exe"',
      'Content-Type: application/octet-stream',
      '',
      'MZ conteudo que nao e um dump',
      `--${boundary}--`,
      '',
    ].join('\r\n')

    const response = await as('admin').post('/api/backups/import', {
      body,
      headers: { 'content-type': `multipart/form-data; boundary=${boundary}` },
    })

    // A extensao e' a primeira barreira do import. Aceitar `.exe` aqui
    // significaria gravar um executavel na area de backups.
    expect(response.status).toBe(422)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().post('/api/backups/import', { json: {} }), 401)
  })
})
