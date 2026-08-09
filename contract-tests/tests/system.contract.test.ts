/**
 * Lote 2.6 — `/api/stats` e `/api/system/*` (10 rotas).
 *
 * Duas coisas dominam este arquivo.
 *
 * A primeira: os diagnósticos são **admin-only** por um motivo forte, e o
 * comentário do próprio controller o explica melhor que qualquer teste — um
 * heap snapshot carrega o heap inteiro do processo, o que inclui senhas de
 * banco já descriptografadas, credenciais de storage, tokens de sessão e a
 * chave de criptografia da aplicação. É material mais sensível que um backup.
 *
 * A segunda: o nome do artefato vem da URL e vira caminho de arquivo. Se
 * `resolvePath` deixar escapar do diretório, o endpoint entrega qualquer
 * arquivo do servidor. Os testes de path traversal aqui não são cerimônia.
 */

import { describe, expect, it } from 'vitest'
import { expectGolden } from '../src/golden.ts'
import { as, expectStatus, json, unauth } from '../src/session.ts'

describe('GET /api/stats', () => {
  it('agrega conexoes, backups e espaco', async () => {
    const response = expectStatus(await as('admin').get('/api/stats'), 200)

    const body = json<{
      data: {
        connections: { total: number; active: number }
        backups: { total: number; today: number }
        recentBackups: unknown[]
        storageSpaces: unknown[]
        system: unknown
      }
    }>(response)

    expect(body.data.connections.total).toBeGreaterThan(0)
    expect(body.data.connections.active).toBeLessThanOrEqual(body.data.connections.total)
    expect(Array.isArray(body.data.recentBackups)).toBe(true)

    expectGolden('system/stats', response, {
      as: 'admin',
      // `system` e `storageSpaces` dependem da maquina (CPU, disco, memoria);
      // o formato deles e' contrato, o conteudo nao. `recentBackups` esta'
      // vazio aqui — o lote 2.4 e' quem fixa o formato do item.
      compare: { ignorePaths: ['data.system', 'data.storageSpaces', 'data.recentBackups'] },
    })
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/stats'), 401)
  })
})

describe('GET /api/system/status', () => {
  it('devolve o panorama do sistema', async () => {
    const response = expectStatus(await as('admin').get('/api/system/status'), 200)
    expect(json<{ success: boolean; data: unknown }>(response).success).toBe(true)

    expectGolden('system/status', response, {
      as: 'admin',
      compare: { ignorePaths: ['data'] },
    })
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/system/status'), 401)
  })
})

describe('GET /api/system/diagnostics', () => {
  it('lista os artefatos para o admin', async () => {
    const response = expectStatus(await as('admin').get('/api/system/diagnostics'), 200)

    const body = json<{
      data: { directory: string; directoryExists: boolean; files: unknown[] }
    }>(response)

    expect(typeof body.data.directory).toBe('string')
    expect(Array.isArray(body.data.files)).toBe(true)

    expectGolden('system/diagnostics', response, {
      as: 'admin',
      compare: { ignorePaths: ['data.files'] },
    })
  })

  it('nega para usuario comum', async () => {
    const response = expectStatus(await as('member').get('/api/system/diagnostics'), 403)
    expect(json<{ success: boolean }>(response).success).toBe(false)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/system/diagnostics'), 401)
  })
})

describe('GET /api/system/diagnostics/:name/download', () => {
  it('responde 404 para artefato inexistente', async () => {
    const response = expectStatus(
      await as('admin').get('/api/system/diagnostics/nao-existe.heapsnapshot/download'),
      404
    )
    expect(json<{ success: boolean }>(response).success).toBe(false)

    expectGolden('system/diagnostics-download-not-found', response, { as: 'admin' })
  })

  it('bloqueia path traversal', async () => {
    // O nome vem da URL e vira caminho. `resolvePath` tem que recusar tudo que
    // sair do diretorio de diagnosticos — devolvendo 404, sem confirmar nem
    // desmentir a existencia do alvo.
    for (const nome of ['..%2F..%2Fpackage.json', '..%5C..%5Cpackage.json', '%2Fetc%2Fpasswd']) {
      const response = await as('admin').get(`/api/system/diagnostics/${nome}/download`)
      expect(response.status, `traversal aceito com \`${nome}\``).not.toBe(200)
      expect(response.text).not.toContain('"dependencies"')
    }
  })

  it('nega para usuario comum antes de olhar o arquivo', async () => {
    expectStatus(await as('member').get('/api/system/diagnostics/qualquer/download'), 403)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/system/diagnostics/qualquer/download'), 401)
  })
})

describe('DELETE /api/system/diagnostics/:name', () => {
  it('responde 404 para artefato inexistente', async () => {
    expectStatus(await as('admin').delete('/api/system/diagnostics/nao-existe.heapsnapshot'), 404)
  })

  it('bloqueia path traversal', async () => {
    // Aqui o risco e' pior que no download: um traversal que passe **apaga**
    // um arquivo do servidor.
    for (const nome of ['..%2F..%2Fpackage.json', '..%5C..%5Cpackage.json']) {
      const response = await as('admin').delete(`/api/system/diagnostics/${nome}`)
      expect(response.status, `traversal aceito com \`${nome}\``).toBe(404)
    }
  })

  it('nega para usuario comum', async () => {
    expectStatus(await as('member').delete('/api/system/diagnostics/qualquer'), 403)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().delete('/api/system/diagnostics/qualquer'), 401)
  })
})

describe('GET /api/system/containers/resources', () => {
  it('responde 200 mesmo sem Docker', async () => {
    const response = expectStatus(await as('admin').get('/api/system/containers/resources'), 200)
    expect(json<{ success: boolean }>(response).success).toBe(true)

    expectGolden('system/containers-resources', response, {
      as: 'admin',
      compare: { ignorePaths: ['data'] },
    })
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/system/containers/resources'), 401)
  })
})

describe('GET /api/system/resources/history', () => {
  it('aceita rangeHours', async () => {
    const response = expectStatus(
      await as('admin').get('/api/system/resources/history', { query: { rangeHours: 1 } }),
      200
    )
    expect(json<{ success: boolean }>(response).success).toBe(true)

    expectGolden('system/resources-history', response, {
      as: 'admin',
      compare: { ignorePaths: ['data'] },
    })
  })

  it('cai no default quando rangeHours nao e numerico', async () => {
    // `Number('abc')` e' NaN e o controller volta para 24. Um porte que
    // deixasse o NaN passar para a query geraria erro de banco.
    expectStatus(
      await as('admin').get('/api/system/resources/history', { query: { rangeHours: 'abc' } }),
      200
    )
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/system/resources/history'), 401)
  })
})

describe('GET e PUT /api/system/backup-retention', () => {
  it('devolve a politica atual com o cron padrao', async () => {
    const response = expectStatus(await as('admin').get('/api/system/backup-retention'), 200)

    const body = json<{
      data: {
        daily: number
        weekly: number
        monthly: number
        yearly: number
        pruneCron: string
        defaultPruneCron: string
      }
    }>(response)

    expect(body.data.defaultPruneCron).toBeTruthy()
    expect(typeof body.data.daily).toBe('number')

    expectGolden('system/backup-retention', response, { as: 'admin' })
  })

  it('atualiza a politica GFS', async () => {
    const response = expectStatus(
      await as('admin').put('/api/system/backup-retention', {
        json: { daily: 7, weekly: 4, monthly: 6, yearly: 2, pruneCron: '0 3 * * *' },
      }),
      200
    )

    const body = json<{ data: { daily: number; pruneCron: string } }>(response)
    expect(body.data.daily).toBe(7)
    expect(body.data.pruneCron).toBe('0 3 * * *')

    expectGolden('system/backup-retention-update', response, { as: 'admin' })

    // A politica tem que persistir, nao so' voltar na resposta.
    const relido = json<{ data: { daily: number } }>(
      await as('admin').get('/api/system/backup-retention')
    )
    expect(relido.data.daily).toBe(7)
  })

  it('recusa expressao cron invalida', async () => {
    const response = expectStatus(
      await as('admin').put('/api/system/backup-retention', {
        json: { daily: 7, weekly: 4, monthly: 6, yearly: 2, pruneCron: 'isso nao e cron' },
      }),
      422
    )
    expect(json<{ success: boolean }>(response).success).toBe(false)

    expectGolden('system/backup-retention-invalid-cron', response, { as: 'admin' })
  })

  it('recusa valores fora da faixa', async () => {
    const body = json<{ errors: Array<{ field: string }> }>(
      expectStatus(
        await as('admin').put('/api/system/backup-retention', {
          json: { daily: -1, weekly: 4, monthly: 6, yearly: 2, pruneCron: '0 3 * * *' },
        }),
        422
      )
    )
    expect(body.errors.some((error) => error.field === 'daily')).toBe(true)
  })

  it('exige todos os campos', async () => {
    const body = json<{ errors: Array<{ field: string }> }>(
      expectStatus(await as('admin').put('/api/system/backup-retention', { json: {} }), 422)
    )
    expect(body.errors.map((error) => error.field).sort()).toEqual([
      'daily',
      'monthly',
      'pruneCron',
      'weekly',
      'yearly',
    ])
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/system/backup-retention'), 401)
    expectStatus(await unauth().put('/api/system/backup-retention', { json: {} }), 401)
  })
})

describe('POST /api/system/backup-retention/run', () => {
  it('executa o prune', async () => {
    const response = expectStatus(await as('admin').post('/api/system/backup-retention/run'), 200)
    expect(json<{ success: boolean }>(response).success).toBe(true)

    expectGolden('system/backup-retention-run', response, {
      as: 'admin',
      compare: { ignorePaths: ['data'] },
    })
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().post('/api/system/backup-retention/run'), 401)
  })
})
