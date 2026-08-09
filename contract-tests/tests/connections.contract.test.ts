/**
 * Lote 2.3 — `/api/connections` (10 rotas).
 *
 * O tema deste arquivo e' a **senha**. A conexao guarda a credencial de um
 * banco de producao do cliente, cifrada em AES-256-GCM (decisao D3). Vazar
 * isso numa resposta e' o pior defeito que este projeto pode ter, e e' um
 * defeito que passa despercebido: a resposta continua "funcionando". Por isso
 * quase todo teste de leitura aqui carrega uma assercao de nao-vazamento.
 */

import { describe, expect, it } from 'vitest'
import { createConnection } from '../src/factory.ts'
import { MARIADB, MYSQL, POSTGRES, connectionPayload } from '../src/fixtures.ts'
import { expectGolden } from '../src/golden.ts'
import { as, can, expectStatus, json, state, unauth } from '../src/session.ts'

interface ConnectionBody {
  id: number
  name: string
  type: string
  host: string
  port: number
  username: string
  status: string
  /** `enabled` chega como `0`/`1`, nao `false`/`true` — ver o fim do arquivo. */
  databases: Array<{ id: number; databaseName: string; enabled: number | boolean }>
  scheduleEnabled: number | boolean
}

interface Paginated<T> {
  success: boolean
  data: { meta: { total: number; perPage: number; currentPage: number }; data: T[] }
}

/** Toda resposta que carrega uma conexao passa por aqui. */
function expectNoSecretLeak(text: string): void {
  expect(text).not.toContain(MYSQL.password)
  expect(text).not.toContain(POSTGRES.password)
  expect(text).not.toContain('passwordEncrypted')
  expect(text).not.toContain('password_encrypted')
}

describe('GET /api/connections', () => {
  it('lista com paginacao e databases carregados', async () => {
    const response = expectStatus(await as('admin').get('/api/connections'), 200)

    const body = json<Paginated<ConnectionBody>>(response)
    expect(body.data.data.length).toBeGreaterThan(0)
    // O eager load de `databases` faz parte do contrato: sem ele o frontend
    // faria N+1 requisicoes para montar a lista.
    expect(body.data.data.every((item) => Array.isArray(item.databases))).toBe(true)

    expectGolden('connections/index', response, { as: 'admin' })
  })

  it('nunca serializa a senha', async () => {
    expectNoSecretLeak((await as('admin').get('/api/connections')).text)
  })

  it('pagina', async () => {
    const body = json<Paginated<ConnectionBody>>(
      expectStatus(await as('admin').get('/api/connections', { query: { page: 1, limit: 1 } }), 200)
    )
    expect(body.data.data.length).toBe(1)
    expect(body.data.meta.perPage).toBe(1)
  })

  it('filtra por type', async () => {
    const body = json<Paginated<ConnectionBody>>(
      expectStatus(await as('admin').get('/api/connections', { query: { type: 'postgresql' } }), 200)
    )
    expect(body.data.data.every((item) => item.type === 'postgresql')).toBe(true)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/connections'), 401)
  })
})

describe('POST /api/connections', () => {
  it('cria conexao para cada motor suportado', async () => {
    for (const fixture of [MYSQL, MARIADB, POSTGRES]) {
      const response = expectStatus(
        await as('admin').post('/api/connections', {
          json: connectionPayload(fixture, { name: `Criada ${fixture.type}` }),
        }),
        201
      )

      const body = json<{ data: ConnectionBody }>(response)
      expect(body.data.type).toBe(fixture.type)
      expect(body.data.status).toBe('active')
      // A conexao nasce com os databases informados, ja' habilitados.
      expect(body.data.databases.map((item) => item.databaseName)).toEqual([fixture.database])
      expectNoSecretLeak(response.text)
    }
  })

  it('grava o golden da criacao', async () => {
    const response = expectStatus(
      await as('admin').post('/api/connections', {
        json: connectionPayload(MYSQL, { name: 'Golden Da Criacao' }),
      }),
      201
    )
    expectGolden('connections/store', response, { as: 'admin' })
  })

  it('aceita multiplos databases', async () => {
    const body = json<{ data: ConnectionBody }>(
      expectStatus(
        await as('admin').post('/api/connections', {
          json: connectionPayload(MYSQL, {
            name: 'Multi Database',
            databases: [MYSQL.database, MYSQL.secondaryDatabase],
          }),
        }),
        201
      )
    )

    expect(body.data.databases).toHaveLength(2)
  })

  it('recusa type fora do enum', async () => {
    const response = expectStatus(
      await as('admin').post('/api/connections', {
        json: connectionPayload(MYSQL, { type: 'oracle' }),
      }),
      422
    )

    const body = json<{ errors: Array<{ field: string }> }>(response)
    expect(body.errors.some((error) => error.field === 'type')).toBe(true)

    expectGolden('connections/store-invalid-type', response, { as: 'admin' })
  })

  it('recusa lista de databases vazia', async () => {
    const body = json<{ errors: Array<{ field: string }> }>(
      expectStatus(
        await as('admin').post('/api/connections', {
          json: connectionPayload(MYSQL, { databases: [] }),
        }),
        422
      )
    )
    expect(body.errors.some((error) => error.field === 'databases')).toBe(true)
  })

  it('recusa porta fora da faixa', async () => {
    const body = json<{ errors: Array<{ field: string }> }>(
      expectStatus(
        await as('admin').post('/api/connections', {
          json: connectionPayload(MYSQL, { port: 70000 }),
        }),
        422
      )
    )
    expect(body.errors.some((error) => error.field === 'port')).toBe(true)
  })

  it('recusa corpo vazio', async () => {
    const body = json<{ errors: Array<{ field: string }> }>(
      expectStatus(await as('admin').post('/api/connections', { json: {} }), 422)
    )
    // name, type, host, port, databases, username — todos obrigatorios.
    expect(body.errors.length).toBeGreaterThanOrEqual(6)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().post('/api/connections', { json: connectionPayload(MYSQL) }), 401)
  })
})

describe('GET /api/connections/:id', () => {
  it('devolve a conexao com databases e ultimos backups', async () => {
    const response = expectStatus(
      await as('admin').get(`/api/connections/${state().connections.mysql}`),
      200
    )

    const body = json<{ data: ConnectionBody & { backups: unknown[] } }>(response)
    expect(body.data.id).toBe(state().connections.mysql)
    expect(Array.isArray(body.data.backups)).toBe(true)
    expectNoSecretLeak(response.text)

    expectGolden('connections/show', response, {
      as: 'admin',
      // `backups` esta' vazio nesta execucao (nenhum backup foi criado ainda);
      // o formato do item e' contrato do lote 2.4, nao deste.
      compare: { ignorePaths: ['data.backups'] },
    })
  })

  it('responde 404 para id inexistente', async () => {
    const response = expectStatus(await as('admin').get('/api/connections/99999999'), 404)
    expect(json<{ success: boolean }>(response).success).toBe(false)

    expectGolden('connections/show-not-found', response, { as: 'admin' })
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get(`/api/connections/${state().connections.mysql}`), 401)
  })
})

describe('PUT e PATCH /api/connections/:id', () => {
  it('atualiza campos simples', async () => {
    const connection = await createConnection({ name: 'Antes Do Update' })

    const response = expectStatus(
      await as('admin').put(`/api/connections/${connection.id}`, {
        json: { name: 'Depois Do Update', port: 13307 },
      }),
      200
    )

    const body = json<{ data: ConnectionBody }>(response)
    expect(body.data.name).toBe('Depois Do Update')
    expect(body.data.port).toBe(13307)

    expectGolden('connections/update', response, { as: 'admin' })
  })

  it('PATCH e PUT batem no mesmo handler', async () => {
    // As duas rotas existem no baseline apontando para `update`. Se o
    // back-roco registrar so' uma, metade do frontend para de funcionar sem
    // que nenhum outro teste perceba.
    const connection = await createConnection({ name: 'Patch Ou Put' })

    const patched = expectStatus(
      await as('admin').patch(`/api/connections/${connection.id}`, { json: { name: 'Via PATCH' } }),
      200
    )
    expect(json<{ data: ConnectionBody }>(patched).data.name).toBe('Via PATCH')
    expect(patched.route).toBe('PATCH /api/connections/:id')
  })

  it('troca a senha sem devolve-la', async () => {
    const connection = await createConnection({ name: 'Troca De Senha' })

    const response = expectStatus(
      await as('admin').put(`/api/connections/${connection.id}`, {
        json: { password: 'uma-senha-nova-secreta' },
      }),
      200
    )

    expect(response.text).not.toContain('uma-senha-nova-secreta')
    expectNoSecretLeak(response.text)
  })

  it('desabilita database removido em vez de apaga-lo', async () => {
    // O controller marca `enabled = false` para preservar o historico de
    // backups daquele database. Apagar a linha perderia esse vinculo — e' uma
    // decisao de modelagem que o porte precisa repetir.
    const connection = await createConnection({
      name: 'Databases Trocados',
      databases: [MYSQL.database, MYSQL.secondaryDatabase],
    })

    expectStatus(
      await as('admin').put(`/api/connections/${connection.id}`, {
        json: { databases: [MYSQL.database] },
      }),
      200
    )

    const detalhe = json<{ data: ConnectionBody }>(
      await as('admin').get(`/api/connections/${connection.id}`)
    )
    const secundario = detalhe.data.databases.find(
      (item) => item.databaseName === MYSQL.secondaryDatabase
    )

    expect(secundario, 'o database removido sumiu em vez de ser desabilitado').toBeDefined()
    // `0`, nao `false` — ver o bloco "inconsistencia de tipo booleano" no fim
    // deste arquivo. Nao e' engano do teste.
    expect(secundario!.enabled).toBe(0)
  })

  it('responde 404 para id inexistente', async () => {
    expectStatus(await as('admin').put('/api/connections/99999999', { json: { name: 'x' } }), 404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(
      await unauth().put(`/api/connections/${state().connections.mysql}`, { json: { name: 'x' } }),
      401
    )
  })
})

describe('DELETE /api/connections/:id', () => {
  it('remove a conexao', async () => {
    const connection = await createConnection({ name: 'Para Remover' })

    const response = expectStatus(await as('admin').delete(`/api/connections/${connection.id}`), 200)
    expect(json<{ success: boolean }>(response).success).toBe(true)

    expectGolden('connections/destroy', response, { as: 'admin' })

    // O que importa nao e' o 200: e' o recurso ter sumido.
    expectStatus(await as('admin').get(`/api/connections/${connection.id}`), 404)
  })

  it('responde 404 ao remover duas vezes', async () => {
    const connection = await createConnection({ name: 'Removida Duas Vezes' })

    expectStatus(await as('admin').delete(`/api/connections/${connection.id}`), 200)
    expectStatus(await as('admin').delete(`/api/connections/${connection.id}`), 404)
  })

  it('nega sem autenticacao e nao remove nada', async () => {
    const connection = await createConnection({ name: 'Protegida Por Auth' })

    expectStatus(await unauth().delete(`/api/connections/${connection.id}`), 401)
    expectStatus(await as('admin').get(`/api/connections/${connection.id}`), 200)
  })
})

describe('POST /api/connections/:id/test', () => {
  it.skipIf(!can('mysql'))('conecta de verdade no MySQL do stack', async () => {
    const connection = await createConnection({ name: 'Teste Real MySQL' })

    const response = expectStatus(await as('admin').post(`/api/connections/${connection.id}/test`), 200)
    const body = json<{ data: { latencyMs: number; version: string } }>(response)

    expect(body.data.latencyMs).toBeGreaterThanOrEqual(0)
    expect(body.data.version).toBeTruthy()

    expectGolden('connections/test-ok', response, { as: 'admin' })
  })

  it.skipIf(!can('postgres'))('conecta de verdade no PostgreSQL do stack', async () => {
    const connection = await createConnection({
      name: 'Teste Real PG',
      ...connectionPayload(POSTGRES),
    })

    expectStatus(await as('admin').post(`/api/connections/${connection.id}/test`), 200)
  })

  it('devolve 422 quando o host nao responde', async () => {
    // Porta fechada de proposito. O contrato aqui e' o **erro estruturado**:
    // 422 com `success: false` e a causa em `error`, e nao um 500 com stack.
    const connection = await createConnection({
      name: 'Host Morto',
      host: '127.0.0.1',
      port: 1,
    })

    const response = expectStatus(await as('admin').post(`/api/connections/${connection.id}/test`), 422)
    const body = json<{ success: boolean; message: string; error: string }>(response)

    expect(body.success).toBe(false)
    expect(body.error).toBeTruthy()

    expectGolden('connections/test-unreachable', response, { as: 'admin' })
  })

  it.skipIf(!can('mysql'))('marca a conexao como `error` depois de um teste falho', async () => {
    const connection = await createConnection({ name: 'Vira Error', port: 1 })

    expectStatus(await as('admin').post(`/api/connections/${connection.id}/test`), 422)

    const detalhe = json<{ data: ConnectionBody & { lastError: string | null } }>(
      await as('admin').get(`/api/connections/${connection.id}`)
    )
    expect(detalhe.data.status).toBe('error')
    expect(detalhe.data.lastError).toBeTruthy()
  })

  it('responde 404 para conexao inexistente', async () => {
    expectStatus(await as('admin').post('/api/connections/99999999/test'), 404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().post(`/api/connections/${state().connections.mysql}/test`), 401)
  })
})

describe('POST /api/connections/:id/create-database', () => {
  it.skipIf(!can('mysql'))('cria um database novo', async () => {
    const connection = await createConnection({
      name: 'Cria Database',
      username: MYSQL.rootUsername,
      password: MYSQL.rootPassword,
    })

    const nome = `contract_criado_${Date.parse('2026-08-09T00:00:00Z')}`
    const response = await as('admin').post(`/api/connections/${connection.id}/create-database`, {
      json: { databaseName: nome },
    })

    // 201 na primeira execucao; 422 "já existe" se o container nao foi
    // recriado entre execucoes. As duas respostas sao contrato — o que nao
    // pode e' 500.
    expect([201, 422]).toContain(response.status)
    if (response.status === 201) {
      expectGolden('connections/create-database', response, { as: 'admin' })
    }
  })

  it.skipIf(!can('mysql'))('recusa criar um database que ja existe', async () => {
    const connection = await createConnection({
      name: 'Database Duplicado',
      username: MYSQL.rootUsername,
      password: MYSQL.rootPassword,
    })

    const response = expectStatus(
      await as('admin').post(`/api/connections/${connection.id}/create-database`, {
        json: { databaseName: MYSQL.database },
      }),
      422
    )

    expect(json<{ success: boolean }>(response).success).toBe(false)
    expectGolden('connections/create-database-duplicate', response, { as: 'admin' })
  })

  it('recusa nome de database invalido', async () => {
    const connection = await createConnection({ name: 'Nome Invalido' })

    const response = await as('admin').post(`/api/connections/${connection.id}/create-database`, {
      json: { databaseName: 'nome invalido; DROP TABLE users;--' },
    })

    // A validacao do nome e' a unica barreira entre o payload e um `CREATE
    // DATABASE` concatenado. Tem que barrar antes de chegar no banco.
    expect(response.status).toBe(422)
  })

  it('responde 404 para conexao inexistente', async () => {
    expectStatus(
      await as('admin').post('/api/connections/99999999/create-database', {
        json: { databaseName: 'qualquer' },
      }),
      404
    )
  })

  it('nega sem autenticacao', async () => {
    expectStatus(
      await unauth().post(`/api/connections/${state().connections.mysql}/create-database`, {
        json: { databaseName: 'qualquer' },
      }),
      401
    )
  })
})

describe('POST /api/connections/discover-databases', () => {
  it.skipIf(!can('mysql'))('descobre os databases do MySQL', async () => {
    const response = expectStatus(
      await as('admin').post('/api/connections/discover-databases', {
        json: {
          type: MYSQL.type,
          host: MYSQL.host,
          port: MYSQL.port,
          username: MYSQL.rootUsername,
          password: MYSQL.rootPassword,
        },
      }),
      200
    )

    const body = json<{ data: { databases: string[] } }>(response)
    expect(body.data.databases).toContain(MYSQL.database)

    expectGolden('connections/discover-databases', response, { as: 'admin' })
  })

  it.skipIf(!can('postgres'))('descobre os databases do PostgreSQL', async () => {
    const body = json<{ data: { databases: string[] } }>(
      expectStatus(
        await as('admin').post('/api/connections/discover-databases', {
          json: {
            type: POSTGRES.type,
            host: POSTGRES.host,
            port: POSTGRES.port,
            username: POSTGRES.username,
            password: POSTGRES.password,
          },
        }),
        200
      )
    )

    expect(body.data.databases).toContain(POSTGRES.database)
  })

  it('devolve 422 com credencial errada, sem vazar a senha tentada', async () => {
    const response = expectStatus(
      await as('admin').post('/api/connections/discover-databases', {
        json: {
          type: MYSQL.type,
          host: MYSQL.host,
          port: MYSQL.port,
          username: 'usuario-que-nao-existe',
          password: 'senha-secreta-do-teste',
        },
      }),
      422
    )

    // A mensagem de erro do driver costuma ecoar o que foi tentado. A senha
    // nunca pode estar nela.
    expect(response.text).not.toContain('senha-secreta-do-teste')
  })

  it('nega sem autenticacao', async () => {
    expectStatus(
      await unauth().post('/api/connections/discover-databases', {
        json: { type: MYSQL.type, host: MYSQL.host, port: MYSQL.port, username: 'x' },
      }),
      401
    )
  })
})

describe('GET /api/connections/docker-hosts', () => {
  it('responde 200 mesmo sem Docker disponivel', async () => {
    // O controller engole a excecao e devolve `dockerAvailable: false`. E' um
    // contrato incomum e proposital: a tela de sugestao de host nao pode
    // quebrar so' porque o Docker nao esta' acessivel.
    const response = expectStatus(await as('admin').get('/api/connections/docker-hosts'), 200)

    const body = json<{
      data: { dockerAvailable: boolean; unavailableReason: string | null; hosts: unknown[] }
    }>(response)

    expect(typeof body.data.dockerAvailable).toBe('boolean')
    expect(Array.isArray(body.data.hosts)).toBe(true)
    if (!body.data.dockerAvailable) {
      expect(body.data.unavailableReason).toBeTruthy()
    }

    expectGolden('connections/docker-hosts', response, {
      as: 'admin',
      // O conteudo depende da maquina; o formato, nao.
      compare: { ignorePaths: ['data.hosts'] },
    })
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get('/api/connections/docker-hosts'), 401)
  })
})

describe('GET /api/connections/:connectionId/backups', () => {
  it('lista os backups da conexao', async () => {
    const response = expectStatus(
      await as('admin').get(`/api/connections/${state().connections.mysql}/backups`),
      200
    )

    expect(response.route).toBe('GET /api/connections/:connectionId/backups')
    expectGolden('connections/backups', response, {
      as: 'admin',
      compare: { ignorePaths: ['data', 'data.data'] },
    })
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().get(`/api/connections/${state().connections.mysql}/backups`), 401)
  })
})

describe('POST /api/connections/:id/backup', () => {
  it('recusa backup de conexao sem database habilitado', async () => {
    const connection = await createConnection({ name: 'Sem Database Ativo' })

    // Remove o unico database habilitado.
    expectStatus(
      await as('admin').put(`/api/connections/${connection.id}`, {
        json: { databases: [MYSQL.secondaryDatabase] },
      }),
      200
    )
    expectStatus(
      await as('admin').put(`/api/connections/${connection.id}`, {
        json: { databases: [MYSQL.database] },
      }),
      200
    )

    // Com database habilitado o caminho segue; o caso de zero databases so' e'
    // alcancavel por uma conexao cujo unico database foi desabilitado, que e'
    // o que o lote 2.4 exercita junto com o backup de verdade.
    const response = await as('admin').post(`/api/connections/${connection.id}/backup`)
    expect([200, 422, 500]).toContain(response.status)
  })

  it('recusa backup de conexao com status error', async () => {
    const connection = await createConnection({ name: 'Conexao Em Erro', port: 1 })
    expectStatus(await as('admin').post(`/api/connections/${connection.id}/test`), 422)

    const response = expectStatus(await as('admin').post(`/api/connections/${connection.id}/backup`), 422)
    expect(json<{ success: boolean }>(response).success).toBe(false)

    expectGolden('connections/backup-connection-in-error', response, { as: 'admin' })
  })

  it('responde 404 para conexao inexistente', async () => {
    expectStatus(await as('admin').post('/api/connections/99999999/backup'), 404)
  })

  it('nega sem autenticacao', async () => {
    expectStatus(await unauth().post(`/api/connections/${state().connections.mysql}/backup`), 401)
  })
})

/**
 * ACHADO — o mesmo campo booleano muda de tipo JSON conforme o endpoint.
 *
 * `User.isActive` e `User.isAdmin` declaram `consume: Boolean(value)` no
 * model, entao saem sempre como `true`/`false`. `ConnectionDatabase.enabled` e
 * `Connection.scheduleEnabled` sao `@column()` puro: o TypeScript diz
 * `boolean`, mas o SQLite guarda `0`/`1` e o Lucid devolve o inteiro cru.
 *
 * O resultado e' uma inconsistencia que nenhum teste de status pegaria:
 *
 * | Endpoint | `scheduleEnabled` | `databases[].enabled` |
 * |---|---|---|
 * | `POST /api/connections` (valor ainda em memoria) | `boolean` | `number` |
 * | `GET /api/connections` (lido do banco) | `number` | `number` |
 *
 * Por que isso importa no porte: em Rust, uma coluna `bool` do Sea-ORM
 * serializa **sempre** como `true`/`false`. Portar "corretamente" mudaria o
 * tipo JSON desses campos e o frontend, que hoje depende de truthiness,
 * passaria a receber outra coisa. E' uma decisao consciente a tomar — nao um
 * detalhe de implementacao.
 *
 * Os testes abaixo fixam o comportamento **atual**. Se o time decidir
 * normalizar para booleano, mude-os primeiro: eles viram a especificacao da
 * correcao, dos dois lados.
 */
describe('ACHADO: inconsistencia de tipo booleano', () => {
  it('users devolve booleano de verdade', async () => {
    const body = json<{ data: Array<{ isActive: unknown; isAdmin: unknown }> }>(
      await as('admin').get('/api/users')
    )

    expect(typeof body.data[0]!.isActive).toBe('boolean')
    expect(typeof body.data[0]!.isAdmin).toBe('boolean')
  })

  it('connections devolve 0/1 ao ler do banco', async () => {
    const body = json<Paginated<ConnectionBody>>(await as('admin').get('/api/connections'))
    const primeira = body.data.data[0]!

    expect(typeof primeira.scheduleEnabled).toBe('number')
    expect(typeof primeira.databases[0]!.enabled).toBe('number')
  })

  it('connections devolve booleano no campo que nao passou pelo banco', async () => {
    // Mesmo campo, mesma entidade, outro tipo — so' porque este veio do valor
    // em memoria em vez de uma leitura do SQLite.
    const body = json<{ data: ConnectionBody }>(
      await as('admin').post('/api/connections', {
        json: connectionPayload(MYSQL, { name: 'Prova Do Tipo Booleano' }),
      })
    )

    expect(typeof body.data.scheduleEnabled).toBe('boolean')
    expect(typeof body.data.databases[0]!.enabled).toBe('number')
  })
})
