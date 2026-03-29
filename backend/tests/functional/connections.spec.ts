import { test } from '@japa/runner'
import User from '#models/user'
import Connection from '#models/connection'
import ConnectionDatabase from '#models/connection_database'
import { getScheduler } from '#services/scheduler_service'

test.group('Connections', (group) => {
  let user: User

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'Connections User',
      email: `conn_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const scheduler = getScheduler()
    await scheduler.start()

    return () => {
      scheduler.stop()
    }
  })

  test('create a new connection', async ({ client }) => {
    const token = await User.accessTokens.create(user)
    const response = await client
      .post('/api/connections')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({
        name: 'Test Connection',
        type: 'postgresql',
        host: 'localhost',
        port: 5432,
        databases: ['test_db'],
        username: 'user',
        password: 'password',
      })

    response.assertStatus(201)
    response.assertBodyContains({
      success: true,
      data: {
        name: 'Test Connection',
      },
    })
  })

  test('create a scheduled connection registers a scheduler job', async ({ client }) => {
    const initialJobs = getScheduler().getStats().activeJobs
    const token = await User.accessTokens.create(user)
    const response = await client
      .post('/api/connections')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({
        name: 'Scheduled Connection',
        type: 'postgresql',
        host: 'localhost',
        port: 5432,
        databases: ['scheduled_db'],
        username: 'user',
        password: 'password',
        scheduleEnabled: true,
        scheduleFrequency: '1h',
      })

    response.assertStatus(201)

    const schedulerStats = getScheduler().getStats()
    if (schedulerStats.activeJobs !== initialJobs + 1) {
      throw new Error(
        `Era esperado ${initialJobs + 1} job(s) agendado(s), mas foram encontrados ${schedulerStats.activeJobs}`
      )
    }
  })

  test('list connections', async ({ client }) => {
    const connection = await Connection.create({
      name: 'List Connection',
      type: 'mysql',
      host: '127.0.0.1',
      port: 3306,
      username: 'root',
      passwordEncrypted: 'password',
      status: 'active',
    })

    // Create associated database
    await ConnectionDatabase.create({
      connectionId: connection.id,
      databaseName: 'mydb',
      enabled: true,
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/connections')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
    })
    // Check pagination structure
    response.assertBodyContains({
      data: {
        data: [
          {
            name: 'List Connection',
          },
        ],
      },
    })
  })

  test('show a specific connection', async ({ client }) => {
    const connection = await Connection.create({
      name: 'Show Connection',
      type: 'postgresql',
      host: 'localhost',
      port: 5432,
      username: 'user',
      passwordEncrypted: 'password',
      status: 'active',
    })

    // Create associated database
    await ConnectionDatabase.create({
      connectionId: connection.id,
      databaseName: 'show_db',
      enabled: true,
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .get(`/api/connections/${connection.id}`)
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      data: {
        id: connection.id,
        name: 'Show Connection',
      },
    })
  })

  test('update a connection', async ({ client }) => {
    const connection = await Connection.create({
      name: 'Update Connection',
      type: 'postgresql',
      host: 'localhost',
      port: 5432,
      username: 'user',
      passwordEncrypted: 'password',
      status: 'active',
    })

    // Create associated database
    await ConnectionDatabase.create({
      connectionId: connection.id,
      databaseName: 'update_db',
      enabled: true,
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .put(`/api/connections/${connection.id}`)
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({
        name: 'Updated Name',
        host: '192.168.1.1',
      })

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      data: {
        name: 'Updated Name',
        host: '192.168.1.1',
      },
    })
  })

  test('update a connection schedule registers and removes scheduler jobs', async ({ client }) => {
    const initialJobs = getScheduler().getStats().activeJobs
    const connection = await Connection.create({
      name: 'Scheduled Update Connection',
      type: 'postgresql',
      host: 'localhost',
      port: 5432,
      username: 'user',
      passwordEncrypted: 'password',
      status: 'active',
      scheduleEnabled: false,
      scheduleFrequency: null,
    })

    await ConnectionDatabase.create({
      connectionId: connection.id,
      databaseName: 'scheduled_update_db',
      enabled: true,
    })

    const token = await User.accessTokens.create(user)

    const enableResponse = await client
      .put(`/api/connections/${connection.id}`)
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({
        scheduleEnabled: true,
        scheduleFrequency: '1h',
      })

    enableResponse.assertStatus(200)

    let schedulerStats = getScheduler().getStats()
    if (schedulerStats.activeJobs !== initialJobs + 1) {
      throw new Error(
        `Era esperado ${initialJobs + 1} job(s) agendado(s) após habilitar, mas foram encontrados ${schedulerStats.activeJobs}`
      )
    }

    const disableResponse = await client
      .put(`/api/connections/${connection.id}`)
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({
        scheduleEnabled: false,
        scheduleFrequency: null,
      })

    disableResponse.assertStatus(200)

    schedulerStats = getScheduler().getStats()
    if (schedulerStats.activeJobs !== initialJobs) {
      throw new Error(
        `Era esperado ${initialJobs} job(s) agendado(s) após desabilitar, mas foram encontrados ${schedulerStats.activeJobs}`
      )
    }
  })

  test('delete a connection', async ({ client }) => {
    const connection = await Connection.create({
      name: 'Delete Connection',
      type: 'postgresql',
      host: 'localhost',
      port: 5432,
      username: 'user',
      passwordEncrypted: 'password',
      status: 'active',
    })

    // Create associated database
    await ConnectionDatabase.create({
      connectionId: connection.id,
      databaseName: 'del_db',
      enabled: true,
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .delete(`/api/connections/${connection.id}`)
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)

    // Verify deletion
    const check = await Connection.find(connection.id)
    if (check) {
      throw new Error('Connection should be deleted')
    }
  })

  test('test connection (expect failure due to fake host)', async ({ client }) => {
    const connection = await Connection.create({
      name: 'Test Connection Fail',
      type: 'postgresql',
      host: 'invalid-host-name-xyz',
      port: 5432,
      username: 'user',
      passwordEncrypted: 'password',
      status: 'active',
    })

    // Create associated database
    await ConnectionDatabase.create({
      connectionId: connection.id,
      databaseName: 'test_db',
      enabled: true,
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .post(`/api/connections/${connection.id}/test`)
      .header('Authorization', `Bearer ${token.value!.release()}`)

    // Should be unprocessableEntity or internalServerError or 422 depending on controller logic for connection failure
    // Controller returns 422 for connection failure inside try/catch block if result.success is false
    // but if it throws, it returns 500. `performConnectionTest` catches errors and returns success:false usually.
    // However, if the `host` is unresolvable, it might throw inside `performConnectionTest`, caught there, returns error string.
    // So controller sees success: false and returns 422.

    response.assertStatus(422)
    response.assertBodyContains({
      success: false,
    })
  })

  test('list docker hosts suggestions', async ({ client }) => {
    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/connections/docker-hosts')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({ success: true })
    const body = response.body()
    if (!Array.isArray(body.data?.hosts)) {
      throw new Error('Resposta inválida: data.hosts deve ser um array')
    }
  })
})
