import { test } from '@japa/runner'

test.group('Health Check', () => {
  test('health check returns 200', async ({ client }) => {
    const response = await client.get('/api/health')

    response.assertStatus(200)
    response.assertBodyContains({
      status: 'ok',
      version: '1.0.0',
    })
  })
})
