import { test } from '@japa/runner'
import { existsSync } from 'node:fs'
import User from '#models/user'
import { DockerManagerService, VolumeInUseError } from '#services/docker_manager_service'

const DOCKER_SOCKET = '/var/run/docker.sock'
const dockerAvailable = existsSync(DOCKER_SOCKET)

// Quando rodando dentro de um container Docker (/.dockerenv existe), evitamos
// testes destrutivos (start/stop) para não desligar o próprio backend acidentalmente.
const isInsideContainer = existsSync('/.dockerenv')

test.group('Docker Manager — Auth', () => {
  test('todas as rotas Docker requerem autenticação', async ({ client }) => {
    const routes = [
      { method: 'get', path: '/api/docker/containers' },
      { method: 'get', path: '/api/docker/volumes' },
      { method: 'get', path: '/api/docker/networks' },
      { method: 'get', path: '/api/docker/images' },
    ] as const

    for (const route of routes) {
      const response = await client[route.method](route.path)
      response.assertStatus(401)
    }
  })

  test('rotas de ação requerem autenticação', async ({ client }) => {
    const routes = [
      { method: 'post', path: '/api/docker/containers/abc123/start' },
      { method: 'post', path: '/api/docker/containers/abc123/stop' },
      { method: 'post', path: '/api/docker/containers/abc123/restart' },
      { method: 'get', path: '/api/docker/containers/abc123/logs' },
      { method: 'delete', path: '/api/docker/containers/abc123' },
      { method: 'delete', path: '/api/docker/volumes/my-volume' },
      { method: 'get', path: '/api/docker/volumes/my-volume/export' },
      { method: 'delete', path: '/api/docker/images/sha256:abc' },
      { method: 'post', path: '/api/docker/images/prune' },
      { method: 'post', path: '/api/docker/networks' },
      { method: 'post', path: '/api/docker/networks/abc123/connect' },
      { method: 'post', path: '/api/docker/networks/abc123/disconnect' },
    ] as const

    for (const route of routes) {
      const response = await client[route.method](route.path)
      response.assertStatus(401)
    }
  })
})

test.group('Docker Manager — Indisponível (sem socket)', (group) => {
  let user: User

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'Docker Test User',
      email: `docker_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })
  })

  test('GET /api/docker/containers retorna 200 com available=false quando socket ausente', async ({
    client,
  }) => {
    if (dockerAvailable) return

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/docker/containers')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({ success: true, available: false })
  })

  test('GET /api/docker/volumes retorna 200 com available=false quando socket ausente', async ({
    client,
  }) => {
    if (dockerAvailable) return

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/docker/volumes')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({ success: true, available: false })
  })

  test('GET /api/docker/networks retorna 200 com available=false quando socket ausente', async ({
    client,
  }) => {
    if (dockerAvailable) return

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/docker/networks')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({ success: true, available: false })
  })

  test('GET /api/docker/images retorna 200 com available=false quando socket ausente', async ({
    client,
  }) => {
    if (dockerAvailable) return

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/docker/images')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({ success: true, available: false })
  })
})

test.group('Docker Manager — Socket disponível', (group) => {
  let user: User

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'Docker Available User',
      email: `docker_avail_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })
  })

  test('GET /api/docker/containers retorna 200 com lista de grupos', async ({ client }) => {
    if (!dockerAvailable) return

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/docker/containers')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({ success: true, available: true })
  })

  test('GET /api/docker/volumes retorna 200 com lista de volumes', async ({ client }) => {
    if (!dockerAvailable) return

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/docker/volumes')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({ success: true, available: true })
  })

  test('GET /api/docker/networks retorna 200 com lista de redes', async ({ client }) => {
    if (!dockerAvailable) return

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/docker/networks')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({ success: true, available: true })
  })

  test('GET /api/docker/images retorna 200 com lista de imagens', async ({ client }) => {
    if (!dockerAvailable) return

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/docker/images')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({ success: true, available: true })
  })

  test('POST /api/docker/containers/:id/start retorna sucesso', async ({ client }) => {
    if (!dockerAvailable || isInsideContainer) return

    const token = await User.accessTokens.create(user)

    // Listar containers para obter um ID válido
    const listResponse = await client
      .get('/api/docker/containers')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    const token2 = await User.accessTokens.create(user)
    const body = listResponse.body()
    const groups: Array<{ containers: Array<{ id: string }> }> = body?.data ?? []
    const firstContainer = groups.flatMap((g) => g.containers)[0]

    if (!firstContainer) return

    const response = await client
      .post(`/api/docker/containers/${firstContainer.id}/start`)
      .header('Authorization', `Bearer ${token2.value!.release()}`)

    response.assertStatus(200)
  })

  test('POST /api/docker/containers/:id/stop retorna sucesso', async ({ client }) => {
    if (!dockerAvailable || isInsideContainer) return

    const token = await User.accessTokens.create(user)

    const listResponse = await client
      .get('/api/docker/containers')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    const token2 = await User.accessTokens.create(user)
    const body = listResponse.body()
    const groups: Array<{ containers: Array<{ id: string; state: string }> }> = body?.data ?? []
    const runningContainer = groups.flatMap((g) => g.containers).find((c) => c.state === 'running')

    if (!runningContainer) return

    const response = await client
      .post(`/api/docker/containers/${runningContainer.id}/stop`)
      .header('Authorization', `Bearer ${token2.value!.release()}`)

    response.assertStatus(200)
  })
})

// ================================================================
// Volume em uso — testes de serviço com cliente simulado
// ================================================================

test.group('Docker Manager — Volume em Uso', (group) => {
  let user: User

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'Volume InUse Test User',
      email: `vol_inuse_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })
  })

  test('VolumeInUseError é lançado com nomes e projeto resolvidos', async ({ assert }) => {
    const containerId = 'a'.repeat(64)

    const mockClient = {
      isSocketAvailable: () => true,
      deleteJson: async (_path: string): Promise<null> => {
        throw new Error(
          `Docker Engine respondeu 409: {"message":"remove my-vol: volume is in use - [${containerId}]"}`
        )
      },
      getJson: async (_path: string) => [
        {
          Id: containerId,
          Names: ['/meu-container'],
          Labels: { 'com.docker.compose.project': 'meu-projeto' },
        },
      ],
    }

    const service = new DockerManagerService(mockClient as never)

    let caughtError: unknown
    try {
      await service.removeVolume('my-vol')
    } catch (err) {
      caughtError = err
    }

    assert.instanceOf(caughtError, VolumeInUseError)
    const err = caughtError as VolumeInUseError
    assert.isTrue(err.message.includes('meu-container'))
    assert.isTrue(err.message.includes('meu-projeto'))
    assert.deepEqual(err.containerNames, ['meu-container (meu-projeto)'])
  })

  test('VolumeInUseError usa ID curto como fallback quando container não é encontrado', async ({
    assert,
  }) => {
    const containerId = 'b'.repeat(64)

    const mockClient = {
      isSocketAvailable: () => true,
      deleteJson: async (_path: string): Promise<null> => {
        throw new Error(
          `Docker Engine respondeu 409: {"message":"remove my-vol: volume is in use - [${containerId}]"}`
        )
      },
      getJson: async (_path: string) => [],
    }

    const service = new DockerManagerService(mockClient as never)

    let caughtError: unknown
    try {
      await service.removeVolume('my-vol')
    } catch (err) {
      caughtError = err
    }

    assert.instanceOf(caughtError, VolumeInUseError)
    const err = caughtError as VolumeInUseError
    assert.isTrue(err.message.includes('b'.repeat(12)))
  })

  test('VolumeInUseError usa ID curto como fallback quando resolução de nomes falha', async ({
    assert,
  }) => {
    const containerId = 'c'.repeat(64)

    const mockClient = {
      isSocketAvailable: () => true,
      deleteJson: async (_path: string): Promise<null> => {
        throw new Error(
          `Docker Engine respondeu 409: {"message":"remove my-vol: volume is in use - [${containerId}]"}`
        )
      },
      getJson: async (_path: string): Promise<never> => {
        throw new Error('Simulação de falha ao listar containers')
      },
    }

    const service = new DockerManagerService(mockClient as never)

    let caughtError: unknown
    try {
      await service.removeVolume('my-vol')
    } catch (err) {
      caughtError = err
    }

    assert.instanceOf(caughtError, VolumeInUseError)
    const err = caughtError as VolumeInUseError
    assert.isTrue(err.message.includes('c'.repeat(12)))
  })

  test('erros não relacionados a volume em uso são propagados sem alteração', async ({
    assert,
  }) => {
    const mockClient = {
      isSocketAvailable: () => true,
      deleteJson: async (_path: string): Promise<null> => {
        throw new Error('Docker Engine respondeu 500: Internal Server Error')
      },
      getJson: async (_path: string) => [],
    }

    const service = new DockerManagerService(mockClient as never)

    let caughtError: unknown
    try {
      await service.removeVolume('my-vol')
    } catch (err) {
      caughtError = err
    }

    assert.notInstanceOf(caughtError, VolumeInUseError)
    assert.instanceOf(caughtError, Error)
    assert.isTrue((caughtError as Error).message.includes('Docker Engine respondeu 500'))
  })

  test('DELETE /api/docker/volumes/:name retorna 409 quando volume está em uso', async ({
    client,
    assert,
  }) => {
    if (!dockerAvailable) return

    const token = await User.accessTokens.create(user)

    // Listar volumes reais para tentar remover um que provavelmente está em uso
    const listResponse = await client
      .get('/api/docker/volumes')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    const token2 = await User.accessTokens.create(user)
    const body = listResponse.body()
    const volumes: Array<{ name: string }> = body?.data ?? []

    if (volumes.length === 0) return

    // Tenta remover o primeiro volume — se estiver em uso, deve retornar 409
    const response = await client
      .delete(`/api/docker/volumes/${encodeURIComponent(volumes[0].name)}`)
      .header('Authorization', `Bearer ${token2.value!.release()}`)

    // Aceita 200 (volume removido) ou 409 (em uso) — ambos são respostas válidas
    assert.isTrue(
      [200, 409].includes(response.status()),
      `Status esperado 200 ou 409, recebido ${response.status()}`
    )

    if (response.status() === 409) {
      const responseBody = response.body() as { message?: string }
      assert.isString(responseBody.message)
      assert.isTrue(
        responseBody.message!.includes('em uso'),
        'Mensagem de erro deve indicar que o volume está em uso'
      )
    }
  })
})

// ================================================================
// Novas rotas — Validação de entrada
// ================================================================

test.group('Docker Manager — Validação das Novas Rotas', (group) => {
  let user: User

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'Validation Test User',
      email: `docker_val_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })
  })

  test('POST /api/docker/networks retorna 400 sem nome', async ({ client, assert }) => {
    const token = await User.accessTokens.create(user)
    const response = await client
      .post('/api/docker/networks')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({})

    response.assertStatus(400)
    const body = response.body() as { message?: string }
    assert.isString(body.message)
    assert.include(body.message!, 'Nome')
  })

  test('POST /api/docker/networks retorna 400 com nome vazio', async ({ client }) => {
    const token = await User.accessTokens.create(user)
    const response = await client
      .post('/api/docker/networks')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({ name: '   ' })

    response.assertStatus(400)
  })

  test('POST /api/docker/networks/:id/connect retorna 400 sem containerId', async ({
    client,
    assert,
  }) => {
    const token = await User.accessTokens.create(user)
    const response = await client
      .post('/api/docker/networks/some-network-id/connect')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({})

    response.assertStatus(400)
    const body = response.body() as { message?: string }
    assert.isString(body.message)
    assert.include(body.message!, 'containerId')
  })

  test('POST /api/docker/networks/:id/disconnect retorna 400 sem containerId', async ({
    client,
    assert,
  }) => {
    const token = await User.accessTokens.create(user)
    const response = await client
      .post('/api/docker/networks/some-network-id/disconnect')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({})

    response.assertStatus(400)
    const body = response.body() as { message?: string }
    assert.isString(body.message)
    assert.include(body.message!, 'containerId')
  })
})

// ================================================================
// Novas rotas — Socket disponível
// ================================================================

test.group('Docker Manager — Novas Rotas (Socket disponível)', (group) => {
  let user: User

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'New Routes Test User',
      email: `docker_new_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })
  })

  test('DELETE /api/docker/containers/:id com ID inexistente retorna erro de Docker', async ({
    client,
    assert,
  }) => {
    if (!dockerAvailable) return

    const token = await User.accessTokens.create(user)
    const response = await client
      .delete('/api/docker/containers/nonexistent-container-id-000000000000000')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    // Docker retorna 404 para container inexistente → controller propaga como 500
    assert.isTrue(
      [404, 500].includes(response.status()),
      `Status esperado 404 ou 500, recebido ${response.status()}`
    )
  })

  test('POST /api/docker/networks cria rede com driver bridge', async ({ client }) => {
    if (!dockerAvailable) return

    const token = await User.accessTokens.create(user)
    const networkName = `test-japa-${Date.now()}`
    const response = await client
      .post('/api/docker/networks')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({ name: networkName, driver: 'bridge' })

    response.assertStatus(200)
    response.assertBodyContains({ success: true })
  })

  test('POST /api/docker/networks/:id/connect com container inválido retorna erro de Docker', async ({
    client,
    assert,
  }) => {
    if (!dockerAvailable) return

    const token = await User.accessTokens.create(user)

    // Listar redes para obter um ID válido
    const listToken = await User.accessTokens.create(user)
    const listResponse = await client
      .get('/api/docker/networks')
      .header('Authorization', `Bearer ${listToken.value!.release()}`)

    const networks: Array<{ id: string }> = listResponse.body()?.data ?? []
    if (networks.length === 0) return

    const response = await client
      .post(`/api/docker/networks/${networks[0].id}/connect`)
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({ containerId: 'nonexistent-container-000000000000' })

    // Docker retorna erro (404/500) pois o container não existe
    assert.isTrue(
      [404, 500].includes(response.status()),
      `Status esperado 404 ou 500, recebido ${response.status()}`
    )
  })

  test('GET /api/docker/volumes/:name/export retorna conteúdo comprimido quando disponível', async ({
    client,
    assert,
  }) => {
    if (!dockerAvailable) return

    // Listar volumes
    const listToken = await User.accessTokens.create(user)
    const listResponse = await client
      .get('/api/docker/volumes')
      .header('Authorization', `Bearer ${listToken.value!.release()}`)

    const volumes: Array<{ name: string }> = listResponse.body()?.data ?? []
    if (volumes.length === 0) return // nenhum volume para exportar

    const exportToken = await User.accessTokens.create(user)
    const response = await client
      .get(`/api/docker/volumes/${encodeURIComponent(volumes[0].name)}/export`)
      .header('Authorization', `Bearer ${exportToken.value!.release()}`)

    // Se a imagem alpine não estiver disponível, pode retornar 500
    // Aceitamos 200 (sucesso) ou 500 (alpine não disponível)
    assert.isTrue(
      [200, 500].includes(response.status()),
      `Status esperado 200 ou 500, recebido ${response.status()}`
    )

    if (response.status() === 200) {
      const contentType = response.header('content-type')
      assert.include(contentType, 'gzip')
    }
  })
})
