import { test } from '@japa/runner'
import User from '#models/user'
import Connection from '#models/connection'

test.group('Connections', (group) => {
  let user: User

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'Connections User',
      email: `conn_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })
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
        database: 'test_db',
        username: 'user',
        passwordEncrypted: 'password',
      })

    response.assertStatus(201)
    response.assertBodyContains({
      success: true,
      data: {
        name: 'Test Connection',
      },
    })
  })

  test('list connections', async ({ client }) => {
    await Connection.create({
      name: 'List Connection',
      type: 'mysql',
      host: '127.0.0.1',
      port: 3306,
      database: 'mydb',
      username: 'root',
      passwordEncrypted: 'password',
      status: 'active',
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
      database: 'show_db',
      username: 'user',
      passwordEncrypted: 'password',
      status: 'active',
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
      database: 'update_db',
      username: 'user',
      passwordEncrypted: 'password',
      status: 'active',
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

  test('delete a connection', async ({ client }) => {
    const connection = await Connection.create({
      name: 'Delete Connection',
      type: 'postgresql',
      host: 'localhost',
      port: 5432,
      database: 'del_db',
      username: 'user',
      passwordEncrypted: 'password',
      status: 'active',
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
      database: 'test_db',
      username: 'user',
      passwordEncrypted: 'password',
      status: 'active',
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
})
