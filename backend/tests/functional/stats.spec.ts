import { test } from '@japa/runner'
import User from '#models/user'

test.group('Stats', (group) => {
  let user: User

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'Stats User',
      email: `stats_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })
  })

  test('block unauthenticated access to dashboard stats', async ({ client }) => {
    const response = await client.get('/api/stats')

    response.assertStatus(401)
  })

  test('block unauthenticated access to system status', async ({ client }) => {
    const response = await client.get('/api/system/status')

    response.assertStatus(401)
  })

  test('block unauthenticated access to system heap', async ({ client }) => {
    const response = await client.get('/api/system/heap')

    response.assertStatus(401)
  })

  test('get dashboard stats', async ({ client }) => {
    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/stats')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      data: {
        connections: {},
        backups: {},
        recentBackups: [],
        system: {},
      },
    })

    const body = response.body()
    const system = body?.data?.system
    const jobs = system?.jobs
    const resources = system?.resources
    const storageSpaces = body?.data?.storageSpaces

    if (!system) {
      throw new Error('system deve existir no payload de /api/stats')
    }
    if (typeof system.version !== 'string' || !system.version) {
      throw new Error('system.version deve ser string')
    }
    if (typeof system.hostname !== 'string' || !system.hostname) {
      throw new Error('system.hostname deve ser string')
    }
    if (typeof system.uptimeSeconds !== 'number') {
      throw new Error('system.uptimeSeconds deve ser number')
    }
    if (!resources) {
      throw new Error('system.resources deve existir no payload de /api/stats')
    }
    if (typeof resources.cpu?.usagePercent !== 'number') {
      throw new Error('system.resources.cpu.usagePercent deve ser number')
    }
    if (typeof resources.memory?.usagePercent !== 'number') {
      throw new Error('system.resources.memory.usagePercent deve ser number')
    }
    if (!jobs) {
      throw new Error('system.jobs deve existir no payload de /api/stats')
    }
    if (typeof jobs.isRunning !== 'boolean') {
      throw new Error('system.jobs.isRunning deve ser boolean')
    }
    if (typeof jobs.activeJobs !== 'number') {
      throw new Error('system.jobs.activeJobs deve ser number')
    }
    if (!['ok', 'down'].includes(jobs.status)) {
      throw new Error('system.jobs.status deve ser "ok" ou "down"')
    }
    if (!Array.isArray(storageSpaces)) {
      throw new Error('storageSpaces deve ser um array')
    }

    const expectedStatus = jobs.isRunning ? 'ok' : 'down'
    if (jobs.status !== expectedStatus) {
      throw new Error('jobs.status deve refletir jobs.isRunning')
    }
  })

  test('get system status', async ({ client }) => {
    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/system/status')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      data: {},
    })

    const body = response.body()
    const system = body?.data
    const resources = system?.resources
    const jobs = system?.jobs
    if (!system) {
      throw new Error('data deve existir no payload de /api/system/status')
    }
    if (typeof system.version !== 'string' || !system.version) {
      throw new Error('data.version deve ser string')
    }
    if (typeof system.platform !== 'string' || !system.platform) {
      throw new Error('data.platform deve ser string')
    }
    if (typeof system.nodeVersion !== 'string' || !system.nodeVersion) {
      throw new Error('data.nodeVersion deve ser string')
    }
    if (!resources) {
      throw new Error('data.resources deve existir no payload de /api/system/status')
    }
    if (typeof resources.memory?.totalBytes !== 'number') {
      throw new Error('data.resources.memory.totalBytes deve ser number')
    }
    if (!jobs) {
      throw new Error('data.jobs deve existir no payload de /api/system/status')
    }
    if (typeof jobs.isRunning !== 'boolean') {
      throw new Error('data.jobs.isRunning deve ser boolean')
    }
    if (typeof jobs.activeJobs !== 'number') {
      throw new Error('data.jobs.activeJobs deve ser number')
    }
    if (!['ok', 'down'].includes(jobs.status)) {
      throw new Error('data.jobs.status deve ser "ok" ou "down"')
    }

    const expectedStatus = jobs.isRunning ? 'ok' : 'down'
    if (jobs.status !== expectedStatus) {
      throw new Error('jobs.status deve refletir jobs.isRunning')
    }
  })

  test('get system heap snapshot', async ({ client }) => {
    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/system/heap')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      data: {},
    })

    const body = response.body()
    const heap = body?.data

    if (!heap) {
      throw new Error('data deve existir no payload de /api/system/heap')
    }

    if (typeof heap.timestamp !== 'string' || !heap.timestamp) {
      throw new Error('data.timestamp deve ser string')
    }

    for (const field of [
      'rssBytes',
      'heapTotalBytes',
      'heapUsedBytes',
      'heapUsagePercent',
      'externalBytes',
      'arrayBuffersBytes',
      'activeHandles',
      'activeRequests',
      'uptimeSeconds',
    ]) {
      if (typeof heap[field] !== 'number') {
        throw new Error(`data.${field} deve ser number`)
      }
    }

    if (heap.heapTotalBytes < heap.heapUsedBytes) {
      throw new Error('data.heapTotalBytes deve ser maior ou igual a data.heapUsedBytes')
    }
  })
})
