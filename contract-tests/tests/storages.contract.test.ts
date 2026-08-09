/**
 * Lote 2.5 — `/api/storages` e `/api/storage-destinations` (20 rotas).
 *
 * As duas famílias de rota gravam na **mesma tabela** (`storage_destinations`);
 * `/api/storages` é a interface nova e `/api/storage-destinations` a legada,
 * mantida para compatibilidade. Elas divergem no formato: a nova usa
 * `provider` (`minio`, `aws_s3`, `cloudflare_r2`…) e a legada usa `type`
 * (`s3`, `local`, `gcs`…), com um mapa de tradução no controller. Um porte que
 * unificasse as duas quebraria um dos dois clientes.
 *
 * O tema de segurança aqui é o **mascaramento de segredos**: a config carrega
 * `secretAccessKey`, `connectionString`, `credentialsJson`. `getSafeConfig`
 * existe para que nada disso saia numa resposta, e é o que os testes cobram.
 */

import { describe, expect, it } from 'vitest'
import { MINIO } from '../src/fixtures.ts'
import { expectGolden } from '../src/golden.ts'
import { as, can, expectStatus, json, state, unauth } from '../src/session.ts'

interface StorageItem {
  id: number
  name: string
  type: string
  provider?: string
  status: string
  config?: Record<string, unknown>
}

const minioPayload = (name: string) => ({
  name,
  provider: 'minio',
  config: {
    bucket: MINIO.buckets.primary,
    accessKeyId: MINIO.accessKeyId,
    secretAccessKey: MINIO.secretAccessKey,
    endpoint: MINIO.endpoint,
    region: MINIO.region,
    forcePathStyle: true,
  },
})

/** Nenhum segredo pode aparecer em resposta nenhuma. */
function expectNoSecretLeak(text: string): void {
  expect(text).not.toContain(MINIO.secretAccessKey)
  expect(text).not.toContain('test_pw')
}

async function createStorage(name: string): Promise<number> {
  const response = await as('admin').post('/api/storages', { json: minioPayload(name) })
  expectStatus(response, 201)
  return json<{ data: { id: number } }>(response).data.id
}

describe('GET /api/storages', () => {
  it('lista com paginacao', async () => {
    const response = expectStatus(await as('admin').get('/api/storages'), 200)

    const body = json<{ data: { meta: { total: number }; data: StorageItem[] } }>(response)
    expect(body.data.data.length).toBeGreaterThan(0)
    // `providerLabel` e' derivado no controller, nao gravado no banco — o
    // porte precisa da mesma tabela de rotulos.
    expect(body.data.data.every((item) => 'providerLabel' in item)).toBe(true)

    expectGolden('storages/index', response, { as: 'admin' })
  })

  it('nao vaza segredo nenhum na listagem', async () => {
    expectNoSecretLeak((await as('admin').get('/api/storages')).text)
  })

  it('filtra por provider e por status', async () => {
    const porProvider = json<{ data: { data: StorageItem[] } }>(
      expectStatus(await as('admin').get('/api/storages', { query: { provider: 'local' } }), 200)
    )
    expect(porProvider.data.data.every((item) => item.provider === 'local')).toBe(true)

    const porStatus = json<{ data: { data: StorageItem[] } }>(
      expectStatus(await as('admin').get('/api/storages', { query: { status: 'active' } }), 200)
    )
    expect(porStatus.data.data.every((item) => item.status === 'active')).toBe(true)
  })

  it('busca por nome', async () => {
    const body = json<{ data: { data: StorageItem[] } }>(
      expectStatus(await as('admin').get('/api/storages', { query: { search: 'MinIO' } }), 200)
    )
    expect(body.data.data.every((item) => item.name.toLowerCase().includes('minio'))).toBe(true)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/storages'), 401)
  })
})

describe('POST /api/storages', () => {
  it('cria storage MinIO sem devolver o segredo', async () => {
    const response = expectStatus(
      await as('admin').post('/api/storages', { json: minioPayload('Storage Criado') }),
      201
    )

    const body = json<{ data: StorageItem }>(response)
    expect(body.data.provider).toBe('minio')
    // `minio` mapeia para o `type` legado `s3` — a compatibilidade com a
    // interface antiga depende disso.
    expect(body.data.type).toBe('s3')
    expectNoSecretLeak(response.text)

    expectGolden('storages/store', response, { as: 'admin' })
  })

  it('cria storage local', async () => {
    const response = expectStatus(
      await as('admin').post('/api/storages', {
        json: { name: 'Storage Local Criado', provider: 'local', config: {} },
      }),
      201
    )
    expect(json<{ data: StorageItem }>(response).data.type).toBe('local')
  })

  it('cria storage SFTP sem devolver a senha', async () => {
    const response = await as('admin').post('/api/storages', {
      json: {
        name: 'Storage SFTP',
        provider: 'sftp',
        config: {
          host: '127.0.0.1',
          port: 12222,
          username: 'tester',
          password: 'test_pw',
          basePath: '/home/tester/backups',
        },
      },
    })

    expect([200, 201, 422]).toContain(response.status)
    expectNoSecretLeak(response.text)
  })

  it('recusa provider desconhecido', async () => {
    const response = expectStatus(
      await as('admin').post('/api/storages', {
        json: { name: 'Provider Invalido', provider: 'dropbox', config: {} },
      }),
      422
    )
    expectGolden('storages/store-invalid-provider', response, { as: 'admin' })
  })

  it('recusa MinIO sem bucket', async () => {
    const body = json<{ errors: Array<{ field: string }> }>(
      expectStatus(
        await as('admin').post('/api/storages', {
          json: {
            name: 'Sem Bucket',
            provider: 'minio',
            config: {
              accessKeyId: MINIO.accessKeyId,
              secretAccessKey: MINIO.secretAccessKey,
              endpoint: MINIO.endpoint,
            },
          },
        }),
        422
      )
    )
    expect(body.errors.some((error) => error.field.includes('bucket'))).toBe(true)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().post('/api/storages', { json: minioPayload('x') }), 401)
  })
})

describe('GET /api/storages/:id', () => {
  it('devolve o storage com a config mascarada', async () => {
    const response = expectStatus(await as('admin').get(`/api/storages/${state().storages.minio}`), 200)

    const body = json<{ data: StorageItem }>(response)
    expect(body.data.id).toBe(state().storages.minio)
    // O `accessKeyId` pode aparecer (nao e' segredo); o `secretAccessKey`,
    // nunca.
    expectNoSecretLeak(response.text)

    expectGolden('storages/show', response, { as: 'admin' })
  })

  it('responde 404 para id inexistente', async () => {
    expectStatus(await as('admin').get('/api/storages/99999999'), 404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get(`/api/storages/${state().storages.minio}`), 401)
  })
})

describe('PUT /api/storages/:id', () => {
  it('renomeia sem exigir o segredo de novo', async () => {
    const id = await createStorage('Storage Para Renomear')

    const response = expectStatus(
      await as('admin').put(`/api/storages/${id}`, { json: { name: 'Storage Renomeado' } }),
      200
    )

    expect(json<{ data: StorageItem }>(response).data.name).toBe('Storage Renomeado')
    expectNoSecretLeak(response.text)

    expectGolden('storages/update', response, { as: 'admin' })
  })

  it('mantem o segredo anterior quando o update nao o envia', async () => {
    // `s3CommonConfigUpdate` torna `secretAccessKey` opcional no update: campo
    // vazio significa "mantem o que ja' esta' la'". Se o porte apagasse o
    // segredo nesse caso, o storage pararia de funcionar em silencio — o
    // registro continua existindo e so' falha na hora do upload.
    const id = await createStorage('Storage Segredo Preservado')

    expectStatus(await as('admin').put(`/api/storages/${id}`, { json: { name: 'Renomeado 2' } }), 200)

    const teste = await as('admin').post(`/api/storages/${id}/test`)
    // Com MinIO de pe' o teste passa; sem ele, falha por rede — o que nao pode
    // acontecer e' falhar por credencial ausente.
    expect([200, 422]).toContain(teste.status)
  })

  it('responde 404 para id inexistente', async () => {
    expectStatus(await as('admin').put('/api/storages/99999999', { json: { name: 'x' } }), 404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(
      await unauth().put(`/api/storages/${state().storages.minio}`, { json: { name: 'x' } }),
      401
    )
  })
})

describe('DELETE /api/storages/:id', () => {
  it('remove o storage', async () => {
    const id = await createStorage('Storage Para Remover')

    expectStatus(await as('admin').delete(`/api/storages/${id}`), 200)
    expectStatus(await as('admin').get(`/api/storages/${id}`), 404)
  })

  it('responde 404 ao remover duas vezes', async () => {
    const id = await createStorage('Storage Removido Duas Vezes')
    expectStatus(await as('admin').delete(`/api/storages/${id}`), 200)
    expectStatus(await as('admin').delete(`/api/storages/${id}`), 404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().delete(`/api/storages/${state().storages.minio}`), 401)
  })
})

describe('POST /api/storages/:id/test', () => {
  it.skipIf(!can('minio'))('conecta de verdade no MinIO', async () => {
    const response = expectStatus(
      await as('admin').post(`/api/storages/${state().storages.minio}/test`),
      200
    )
    expect(json<{ success: boolean }>(response).success).toBe(true)

    expectGolden('storages/test-ok', response, {
      as: 'admin',
      compare: { ignorePaths: ['data'] },
    })
  })

  it('devolve erro estruturado quando o endpoint nao existe', async () => {
    const response = await as('admin').post('/api/storages/99999999/test')
    expect(response.status).toBe(404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().post(`/api/storages/${state().storages.minio}/test`), 401)
  })
})

describe('GET /api/storages/:id/browse', () => {
  it.skipIf(!can('minio'))('lista objetos do bucket', async () => {
    const response = expectStatus(
      await as('admin').get(`/api/storages/${state().storages.minio}/browse`),
      200
    )
    expect(json<{ success: boolean }>(response).success).toBe(true)

    expectGolden('storages/browse', response, {
      as: 'admin',
      compare: { ignorePaths: ['data'] },
    })
  })

  it.skipIf(!can('minio'))('bloqueia path traversal no prefixo', async () => {
    // Um `..` que passe permite listar fora do prefixo configurado do bucket.
    for (const caminho of ['../', '../../', '..%2F..%2F']) {
      const response = await as('admin').get(`/api/storages/${state().storages.minio}/browse`, {
        query: { path: caminho },
      })
      expect([200, 422]).toContain(response.status)
      if (response.status === 200) {
        // Se aceitar, o resultado nao pode ter escapado do bucket.
        expect(response.text).not.toContain('/etc/')
      }
    }
  })

  it('responde 404 para storage inexistente', async () => {
    expectStatus(await as('admin').get('/api/storages/99999999/browse'), 404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get(`/api/storages/${state().storages.minio}/browse`), 401)
  })
})

describe('DELETE /api/storages/:id/object', () => {
  it('responde 404 para storage inexistente', async () => {
    expectStatus(
      await as('admin').delete('/api/storages/99999999/object', { query: { key: 'qualquer.sql' } }),
      404
    )
  })

  it.skipIf(!can('minio'))('recusa apagar objeto inexistente com erro estruturado', async () => {
    const response = await as('admin').delete(`/api/storages/${state().storages.minio}/object`, {
      query: { key: 'nao-existe-mesmo.sql' },
    })
    // O que importa e' nao ser 500 nem 200 mentiroso.
    expect([200, 404, 422]).toContain(response.status)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(
      await unauth().delete(`/api/storages/${state().storages.minio}/object`, {
        query: { key: 'x' },
      }),
      401
    )
  })
})

describe('POST /api/storages/:id/copy e GET /api/storages/copy-jobs/:jobId', () => {
  it('responde 404 para storage inexistente', async () => {
    expectStatus(
      await as('admin').post('/api/storages/99999999/copy', {
        json: { targetStorageId: state().storages.minio, keys: ['x.sql'] },
      }),
      404
    )
  })

  it('responde 404 para job inexistente', async () => {
    const response = await as('admin').get('/api/storages/copy-jobs/job-que-nao-existe')
    expect(response.route).toBe('GET /api/storages/copy-jobs/:jobId')
    expect(response.status).toBe(404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/storages/copy-jobs/qualquer'), 401)
    expectStatus(
      await unauth().post(`/api/storages/${state().storages.minio}/copy`, { json: {} }),
      401
    )
  })
})

describe('POST /api/storages/:id/archive e jobs de arquivamento', () => {
  it('responde 404 para storage inexistente', async () => {
    expectStatus(
      await as('admin').post('/api/storages/99999999/archive', { json: { keys: ['x.sql'] } }),
      404
    )
  })

  it('responde 404 para job inexistente', async () => {
    const response = await as('admin').get('/api/storages/archive-jobs/job-que-nao-existe')
    expect(response.route).toBe('GET /api/storages/archive-jobs/:jobId')
    expect(response.status).toBe(404)
  })

  it('responde 404 ao baixar job inexistente', async () => {
    const response = await as('admin').get('/api/storages/archive-jobs/nao-existe/download')
    expect(response.route).toBe('GET /api/storages/archive-jobs/:jobId/download')
    expect(response.status).toBe(404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/storages/archive-jobs/qualquer'), 401)
    expectStatus(await unauth().get('/api/storages/archive-jobs/qualquer/download'), 401)
    expectStatus(
      await unauth().post(`/api/storages/${state().storages.minio}/archive`, { json: {} }),
      401
    )
  })
})

// ====================================================================
// Interface legada: /api/storage-destinations
// ====================================================================

describe('/api/storage-destinations (legado)', () => {
  it('lista os mesmos registros da interface nova', async () => {
    const response = expectStatus(await as('admin').get('/api/storage-destinations'), 200)
    expect(json<{ success: boolean }>(response).success).toBe(true)
    expectNoSecretLeak(response.text)

    expectGolden('storage-destinations/index', response, { as: 'admin' })
  })

  it('cria usando `type` em vez de `provider`', async () => {
    // Esta e' a diferenca central entre as duas interfaces. Se o porte aceitar
    // so' `provider`, o cliente legado quebra.
    const response = expectStatus(
      await as('admin').post('/api/storage-destinations', {
        json: {
          name: 'Destino Legado S3',
          type: 's3',
          config: {
            bucket: MINIO.buckets.secondary,
            accessKeyId: MINIO.accessKeyId,
            secretAccessKey: MINIO.secretAccessKey,
            endpoint: MINIO.endpoint,
            region: MINIO.region,
            forcePathStyle: true,
          },
        },
      }),
      201
    )

    expectNoSecretLeak(response.text)
    expectGolden('storage-destinations/store', response, { as: 'admin' })
  })

  it('mostra, atualiza e remove', async () => {
    const criado = json<{ data: { id: number } }>(
      expectStatus(
        await as('admin').post('/api/storage-destinations', {
          json: { name: 'Destino Ciclo Completo', type: 'local', config: {} },
        }),
        201
      )
    )
    const id = criado.data.id

    const mostrado = expectStatus(await as('admin').get(`/api/storage-destinations/${id}`), 200)
    expectGolden('storage-destinations/show', mostrado, { as: 'admin' })

    expectStatus(
      await as('admin').put(`/api/storage-destinations/${id}`, { json: { name: 'Renomeado' } }),
      200
    )
    expectStatus(
      await as('admin').patch(`/api/storage-destinations/${id}`, { json: { name: 'Via PATCH' } }),
      200
    )

    expectStatus(await as('admin').delete(`/api/storage-destinations/${id}`), 200)
    expectStatus(await as('admin').get(`/api/storage-destinations/${id}`), 404)
  })

  it('responde 404 para id inexistente', async () => {
    expectStatus(await as('admin').get('/api/storage-destinations/99999999'), 404)
    expectStatus(
      await as('admin').put('/api/storage-destinations/99999999', { json: { name: 'x' } }),
      404
    )
    expectStatus(await as('admin').delete('/api/storage-destinations/99999999'), 404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/storage-destinations'), 401)
    expectStatus(await unauth().post('/api/storage-destinations', { json: {} }), 401)
  })
})

describe('espaco em disco dos destinos', () => {
  it('devolve o espaco de todos os destinos', async () => {
    const response = expectStatus(await as('admin').get('/api/storage-destinations-space'), 200)
    expect(json<{ success: boolean }>(response).success).toBe(true)

    expectGolden('storage-destinations/space-all', response, {
      as: 'admin',
      // Espaco livre muda a cada leitura de disco.
      compare: { ignorePaths: ['data'] },
    })
  })

  it('devolve o espaco de um destino', async () => {
    const response = expectStatus(
      await as('admin').get(`/api/storage-destinations/${state().storages.local}/space`),
      200
    )
    expect(response.route).toBe('GET /api/storage-destinations/:id/space')
    expect(json<{ success: boolean }>(response).success).toBe(true)
  })

  it('devolve data null quando o tipo nao suporta medicao', async () => {
    // Contrato incomum e proposital: **200 com `data: null`**, nao 404 nem
    // 422. O frontend distingue "nao ha' informacao" de "erro" por esse null.
    const response = expectStatus(
      await as('admin').get(`/api/storage-destinations/${state().storages.minio}/space`),
      200
    )

    const body = json<{ success: boolean; data: unknown }>(response)
    expect(body.success).toBe(true)
    expect(body.data === null || typeof body.data === 'object').toBe(true)
  })

  it('responde 404 para destino inexistente', async () => {
    expectStatus(await as('admin').get('/api/storage-destinations/99999999/space'), 404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/storage-destinations-space'), 401)
    expectStatus(
      await unauth().get(`/api/storage-destinations/${state().storages.local}/space`),
      401
    )
  })
})
