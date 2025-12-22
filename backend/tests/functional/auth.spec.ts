import { test } from '@japa/runner'
import User from '#models/user'

test.group('Auth', () => {
  test('register a new user', async ({ client }) => {
    const email = `register_${Date.now()}@example.com`
    const password = 'Password123!'

    const response = await client.post('/api/auth/register').json({
      fullName: 'Test Register',
      email,
      password,
    })

    response.assertStatus(201)
    response.assertBodyContains({
      success: true,
    })

    // Verify in DB
    const user = await User.findBy('email', email)
    if (!user) {
      throw new Error('User was not persisted to database')
    }
  })

  test('login with valid credentials', async ({ client }) => {
    const email = `login_${Date.now()}@example.com`
    const password = 'Password123!'
    await User.create({ fullName: 'Login User', email, password, isActive: true })

    const response = await client.post('/api/auth/login').json({
      email,
      password,
    })

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
    })
    response.assertBodyContains({
      data: {
        type: 'bearer',
        user: {
          email: email,
        },
      },
    })
  })

  test('login with invalid credentials', async ({ client }) => {
    const response = await client.post('/api/auth/login').json({
      email: 'nonexistent@example.com',
      password: 'wrongpassword',
    })

    response.assertStatus(400)
  })

  test('access protected route (me)', async ({ client }) => {
    const email = `me_${Date.now()}@example.com`
    const user = await User.create({
      fullName: 'Me User',
      email,
      password: 'Password123!',
      isActive: true,
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/auth/me')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      data: {
        email,
      },
    })
  })

  test('logout', async ({ client }) => {
    const email = `logout_${Date.now()}@example.com`
    const user = await User.create({
      fullName: 'Logout User',
      email,
      password: 'Password123!',
      isActive: true,
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .post('/api/auth/logout')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      message: 'Logged out successfully',
    })
  })
})
