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
      },
    })

    const body = response.body()
    const jobs = body?.data?.jobs
    const storageSpaces = body?.data?.storageSpaces

    if (!jobs) {
      throw new Error('jobs deve existir no payload de /api/stats')
    }
    if (typeof jobs.isRunning !== 'boolean') {
      throw new Error('jobs.isRunning deve ser boolean')
    }
    if (typeof jobs.activeJobs !== 'number') {
      throw new Error('jobs.activeJobs deve ser number')
    }
    if (!['ok', 'down'].includes(jobs.status)) {
      throw new Error('jobs.status deve ser "ok" ou "down"')
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
      data: {
        jobs: {},
      },
    })

    const body = response.body()
    const jobs = body?.data?.jobs

    if (!jobs) {
      throw new Error('jobs deve existir no payload de /api/system/status')
    }
    if (typeof jobs.isRunning !== 'boolean') {
      throw new Error('jobs.isRunning deve ser boolean')
    }
    if (typeof jobs.activeJobs !== 'number') {
      throw new Error('jobs.activeJobs deve ser number')
    }
    if (!['ok', 'down'].includes(jobs.status)) {
      throw new Error('jobs.status deve ser "ok" ou "down"')
    }

    const expectedStatus = jobs.isRunning ? 'ok' : 'down'
    if (jobs.status !== expectedStatus) {
      throw new Error('jobs.status deve refletir jobs.isRunning')
    }
  })
})
