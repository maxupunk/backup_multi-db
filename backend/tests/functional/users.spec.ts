import { test } from '@japa/runner'
import User from '#models/user'

test.group('Users', (group) => {
  let currentUser: User

  group.each.setup(async () => {
    currentUser = await User.create({
      fullName: 'Admin User',
      email: `admin_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })
  })

  test('list users', async ({ client }) => {
    // Create another user to ensure list receives it
    await User.create({
      fullName: 'Other User',
      email: `other_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const token = await User.accessTokens.create(currentUser)
    const response = await client.get('/api/users').header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      meta: { perPage: 10 }
    })
  })

  test('toggle user status', async ({ client }) => {
    const targetUser = await User.create({
      fullName: 'Target User',
      email: `target_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const token = await User.accessTokens.create(currentUser)
    const response = await client.patch(`/api/users/${targetUser.id}/status`).header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
    })
    const body = response.body()
    if (!body.message || (!body.message.includes('desativado com sucesso') && !body.message.includes('ativado com sucesso'))) {
        throw new Error('Message not matching expected pattern')
    }

    // Verify change in DB
    await targetUser.refresh()
    if (targetUser.isActive) {
        throw new Error('User status should be toggled to false')
    }
  })
})
