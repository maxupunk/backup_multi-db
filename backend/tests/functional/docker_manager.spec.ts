import { test } from '@japa/runner'
import { existsSync } from 'node:fs'
import User from '#models/user'

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
      { method: 'delete', path: '/api/docker/volumes/my-volume' },
      { method: 'delete', path: '/api/docker/images/sha256:abc' },
      { method: 'post', path: '/api/docker/images/prune' },
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
