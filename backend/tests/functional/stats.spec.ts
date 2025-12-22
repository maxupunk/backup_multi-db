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
  })
})
