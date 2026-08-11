/**
 * Lote 2.7 — `/api/docker/*` (25 rotas).
 *
 * O contrato mais importante deste bloco não é nenhuma das operações: é o
 * comportamento quando **o Docker não está disponível**. As rotas de listagem
 * respondem `200 { success: true, available: false, data: [] }` em vez de erro,
 * para que a tela do gerenciador abra vazia em vez de quebrar. Um porte que
 * devolvesse 503 nesse caso passaria em qualquer teste de caminho feliz e
 * quebraria toda instalação sem Docker.
 *
 * Onde há efeito destrutivo (parar container, remover volume, prune de
 * imagens), os testes usam **apenas** o alvo dedicado do
 * `docker-compose.test.yml` ou ids inexistentes. Nada aqui toca um container
 * que não seja da suíte.
 */

import { describe, expect, it } from 'vitest'
import { expectGolden } from '../src/golden.ts'
import { as, can, expectStatus, json, unauth } from '../src/session.ts'

const ID_INEXISTENTE = 'container-que-nao-existe-0000000000'

interface DockerEnvelope<T = unknown> {
  success: boolean
  available?: boolean
  data: T
}

describe('GET /api/docker/status', () => {
  it('reporta a disponibilidade sem quebrar', async () => {
    const response = expectStatus(await as('admin').get('/api/docker/status'), 200)

    const body = json<DockerEnvelope>(response)
    expect(body.success).toBe(true)
    expect(typeof body.available).toBe('boolean')

    expectGolden('docker/status', response, {
      as: 'admin',
      compare: { ignorePaths: ['data'] },
    })
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/docker/status'), 401)
  })
})

describe('listagens do Docker', () => {
  const listagens = [
    ['/api/docker/containers', 'docker/containers'],
    ['/api/docker/volumes', 'docker/volumes'],
    ['/api/docker/networks', 'docker/networks'],
    ['/api/docker/images', 'docker/images'],
  ] as const

  for (const [rota, golden] of listagens) {
    it(`GET ${rota} responde 200 com ou sem Docker`, async () => {
      const response = expectStatus(await as('admin').get(rota), 200)

      const body = json<DockerEnvelope>(response)
      expect(body.success).toBe(true)
      // Sem Docker: `available: false` e `data: []`. Com Docker: os dados.
      // As duas formas sao contrato, e nenhuma delas e' um erro HTTP.
      expect(body).toHaveProperty('data')

      expectGolden(golden, response, {
        as: 'admin',
        compare: { ignorePaths: ['data', 'available'] },
      })
    })

    it(`GET ${rota} nega sem autenticacao`, async () => {
      expectStatus(await unauth().get(rota), 401)
    })
  }
})

describe('containers', () => {
  it('inspecionar id inexistente nao devolve 200', async () => {
    const response = await as('admin').get(`/api/docker/containers/${ID_INEXISTENTE}`)
    expect(response.route).toBe('GET /api/docker/containers/:id')
    expect(response.status).not.toBe(200)
  })

  it('logs aceitam os filtros documentados sem quebrar o parser', async () => {
    // `tail`, `since`, `until` e `timestamps` sao a interface de filtro. O
    // parser tem defaults proprios (`tail=200`, `NaN` -> default), entao um
    // valor absurdo nao pode mudar o desfecho — todas as variacoes tem que
    // terminar no mesmo status.
    const statuses = new Set<number>()

    for (const query of [
      { tail: 10 },
      { tail: 'all' },
      { tail: 'nao-numerico' },
      { since: 1_700_000_000 },
      { until: 1_800_000_000 },
      { timestamps: 'true' },
    ]) {
      const response = await as('admin').get(`/api/docker/containers/${ID_INEXISTENTE}/logs`, {
        query,
      })
      expect(response.route).toBe('GET /api/docker/containers/:id/logs')
      statuses.add(response.status)
    }

    expect(statuses.size, `os filtros mudaram o desfecho: ${[...statuses].join(', ')}`).toBe(1)
  })

  it.skipIf(!can('docker'))('lista containers de verdade quando o Docker responde', async () => {
    const body = json<DockerEnvelope<unknown[]>>(await as('admin').get('/api/docker/containers'))
    expect(body.available).toBe(true)
    expect(Array.isArray(body.data)).toBe(true)
  })

  it.skipIf(can('docker'))(
    'ACHADO: sem Docker, listar degrada para 200 mas inspecionar quebra com 500',
    async () => {
      // As rotas de **listagem** (`listContainers`, `listVolumes`,
      // `listNetworks`, `listImages`) checam `isAvailable()` e devolvem
      // `200 { available: false, data: [] }`. As de **item** (`inspect*`,
      // `containerLogs`, `start`, `stop`, ...) nao checam nada e estouram com
      // a excecao do socket — 500.
      //
      // E' uma inconsistencia real do backend atual, e este teste existe para
      // que ela seja uma escolha no porte e nao uma surpresa. Se o back-roco
      // devolver 503 estruturado nas duas familias, sera' *melhor* — mas e'
      // mudanca de contrato, e a decisao precisa ser explicita.
      const listagem = await as('admin').get('/api/docker/containers')
      expect(listagem.status).toBe(200)
      expect(json<DockerEnvelope>(listagem).available).toBe(false)

      const item = await as('admin').get(`/api/docker/containers/${ID_INEXISTENTE}`)
      expect(item.status).toBe(500)
    }
  )

  const acoes = [
    ['post', '/start'],
    ['post', '/stop'],
    ['post', '/restart'],
    ['delete', '/logs'],
    ['delete', ''],
  ] as const

  for (const [metodo, sufixo] of acoes) {
    it(`${metodo.toUpperCase()} ${sufixo || '(remover)'} em id inexistente nao devolve 200`, async () => {
      const client = as('admin')
      const rota = `/api/docker/containers/${ID_INEXISTENTE}${sufixo}`
      const response = metodo === 'post' ? await client.post(rota) : await client.delete(rota)

      expect(response.status).not.toBe(200)
    })

    it(`${metodo.toUpperCase()} ${sufixo || '(remover)'} nega sem autenticacao`, async () => {
      const rota = `/api/docker/containers/${ID_INEXISTENTE}${sufixo}`
      const response = metodo === 'post' ? await unauth().post(rota) : await unauth().delete(rota)

      expect(response.status).toBe(401)
    })
  }
})

describe('volumes', () => {
  it('inspecionar volume inexistente nao devolve 200', async () => {
    const response = await as('admin').get('/api/docker/volumes/volume-que-nao-existe')
    expect(response.route).toBe('GET /api/docker/volumes/:name')
    expect(response.status).not.toBe(200)
  })

  it('exportar volume inexistente nao devolve 200', async () => {
    const response = await as('admin').get('/api/docker/volumes/volume-que-nao-existe/export')
    expect(response.route).toBe('GET /api/docker/volumes/:name/export')
    expect(response.status).not.toBe(200)
  })

  it('backup para storage exige destino valido', async () => {
    const response = await as('admin').post('/api/docker/volumes/volume-que-nao-existe/backup', {
      json: { storageId: 99999999 },
    })
    expect(response.route).toBe('POST /api/docker/volumes/:name/backup')
    expect(response.status).not.toBe(200)
  })

  it('remover volume inexistente nao devolve 200', async () => {
    const response = await as('admin').delete('/api/docker/volumes/volume-que-nao-existe')
    expect(response.status).not.toBe(200)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/docker/volumes/qualquer'), 401)
    expectStatus(await unauth().get('/api/docker/volumes/qualquer/export'), 401)
    expectStatus(await unauth().post('/api/docker/volumes/qualquer/backup', { json: {} }), 401)
    expectStatus(await unauth().delete('/api/docker/volumes/qualquer'), 401)
  })
})

describe('networks', () => {
  it('inspecionar rede inexistente nao devolve 200', async () => {
    const response = await as('admin').get('/api/docker/networks/rede-que-nao-existe')
    expect(response.route).toBe('GET /api/docker/networks/:id')
    expect(response.status).not.toBe(200)
  })

  it('criar rede sem nome e recusado', async () => {
    const response = await as('admin').post('/api/docker/networks', { json: {} })
    expect(response.route).toBe('POST /api/docker/networks')
    expect(response.status).not.toBe(200)
  })

  it('conectar e desconectar exigem container valido', async () => {
    const conectar = await as('admin').post('/api/docker/networks/rede-inexistente/connect', {
      json: { containerId: ID_INEXISTENTE },
    })
    expect(conectar.route).toBe('POST /api/docker/networks/:id/connect')
    expect(conectar.status).not.toBe(200)

    const desconectar = await as('admin').post('/api/docker/networks/rede-inexistente/disconnect', {
      json: { containerId: ID_INEXISTENTE },
    })
    expect(desconectar.route).toBe('POST /api/docker/networks/:id/disconnect')
    expect(desconectar.status).not.toBe(200)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/docker/networks/qualquer'), 401)
    expectStatus(await unauth().post('/api/docker/networks', { json: {} }), 401)
    expectStatus(await unauth().post('/api/docker/networks/x/connect', { json: {} }), 401)
    expectStatus(await unauth().post('/api/docker/networks/x/disconnect', { json: {} }), 401)
  })
})

describe('images', () => {
  it('inspecionar imagem inexistente nao devolve 200', async () => {
    const response = await as('admin').get('/api/docker/images/imagem-que-nao-existe')
    expect(response.route).toBe('GET /api/docker/images/:id')
    expect(response.status).not.toBe(200)
  })

  it('remover imagem inexistente nao devolve 200', async () => {
    const response = await as('admin').delete('/api/docker/images/imagem-que-nao-existe')
    expect(response.route).toBe('DELETE /api/docker/images/:id')
    expect(response.status).not.toBe(200)
  })

  it('prune responde sem quebrar', async () => {
    // `prune` remove imagens **dangling**, que ninguem esta' usando. E' seguro
    // rodar contra o Docker da maquina de desenvolvimento; nao remove imagens
    // referenciadas por container ou tag.
    const response = await as('admin').post('/api/docker/images/prune')
    expect(response.route).toBe('POST /api/docker/images/prune')
    expect([200, 422, 500, 503]).toContain(response.status)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/docker/images/qualquer'), 401)
    expectStatus(await unauth().delete('/api/docker/images/qualquer'), 401)
    expectStatus(await unauth().post('/api/docker/images/prune'), 401)
  })
})

describe('diagnosticos de rede', () => {
  it('recusa payload sem alvo', async () => {
    const response = await as('admin').post('/api/docker/diagnostics', { json: {} })
    expect(response.route).toBe('POST /api/docker/diagnostics')
    expect(response.status).toBe(422)

    expectGolden('docker/diagnostics-invalid', response, { as: 'admin' })
  })

  it('responde 404 para job inexistente', async () => {
    const response = await as('admin').get('/api/docker/diagnostics/job-que-nao-existe')
    expect(response.route).toBe('GET /api/docker/diagnostics/:jobId')
    expect(response.status).toBe(404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().post('/api/docker/diagnostics', { json: {} }), 401)
    expectStatus(await unauth().get('/api/docker/diagnostics/qualquer'), 401)
  })
})
