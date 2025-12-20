import { test } from '@japa/runner'
import User from '#models/user'
import AuditLog from '#models/audit_log'

test.group('Audit Logs', (group) => {
  let user: User

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'Audit User',
      email: `audit_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })
  })

  test('list audit logs', async ({ client }) => {
    // Create a dummy log
    await AuditLog.create({
      action: 'connection.created',
      entityType: 'connection',
      entityId: 1,
      details: { foo: 'bar' },
      description: 'Test log',
      ipAddress: '127.0.0.1',
      userAgent: 'TestAgent',
      status: 'success'
    })

    const token = await User.accessTokens.create(user)
    const response = await client.get('/api/audit-logs').header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      meta: {}
    })
  })

  test('get audit log stats', async ({ client }) => {
    const token = await User.accessTokens.create(user)
    const response = await client.get('/api/audit-logs/stats').header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      data: {}
    })
  })

  test('show specific audit log', async ({ client }) => {
    const log = await AuditLog.create({
      action: 'connection.created',
      entityType: 'connection',
      entityId: 2,
      details: {},
      description: 'Test log view',
      ipAddress: '127.0.0.1',
      userAgent: 'TestAgent',
      status: 'success'
    })

    const token = await User.accessTokens.create(user)
    const response = await client.get(`/api/audit-logs/${log.id}`).header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      data: {
        id: log.id,
        action: 'connection.created'
      }
    })
  })
})
