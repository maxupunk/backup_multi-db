import { test } from '@japa/runner'
import User from '#models/user'

test.group('Storages', () => {
  test('create cloudflare_r2 without region defaults to auto', async ({ client }) => {
    const user = await User.create({
      fullName: 'Storage User',
      email: `storage_r2_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .post('/api/storages')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({
        name: 'R2 Storage',
        provider: 'cloudflare_r2',
        status: 'active',
        config: {
          endpoint: 'https://1234567890abcdef.r2.cloudflarestorage.com',
          bucket: 'backup-bucket',
          accessKeyId: 'AKIA_TEST',
          secretAccessKey: 'SECRET_TEST',
        },
      })

    response.assertStatus(201)
    response.assertBodyContains({
      success: true,
      data: {
        provider: 'cloudflare_r2',
      },
    })

    const body = response.body()
    if (body?.data?.config?.region !== 'auto') {
      throw new Error('Era esperado region=auto para cloudflare_r2 quando nao informado')
    }
  })

  test('reject create minio without endpoint', async ({ client }) => {
    const user = await User.create({
      fullName: 'Storage User',
      email: `storage_minio_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .post('/api/storages')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({
        name: 'MinIO Storage',
        provider: 'minio',
        status: 'active',
        config: {
          bucket: 'backup-bucket',
          accessKeyId: 'MINIO_KEY',
          secretAccessKey: 'MINIO_SECRET',
        },
      })

    response.assertStatus(422)
  })

  test('reject create aws_s3 without region', async ({ client }) => {
    const user = await User.create({
      fullName: 'Storage User',
      email: `storage_aws_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .post('/api/storages')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({
        name: 'AWS Storage',
        provider: 'aws_s3',
        status: 'active',
        config: {
          bucket: 'backup-bucket',
          accessKeyId: 'AWS_KEY',
          secretAccessKey: 'AWS_SECRET',
        },
      })

    response.assertStatus(422)
  })

  test('update cloudflare_r2 without region keeps normalized auto region', async ({ client }) => {
    const user = await User.create({
      fullName: 'Storage User',
      email: `storage_update_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const token = await User.accessTokens.create(user)

    const createResponse = await client
      .post('/api/storages')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({
        name: 'R2 Storage Update',
        provider: 'cloudflare_r2',
        status: 'active',
        config: {
          endpoint: 'https://1111111111111111.r2.cloudflarestorage.com',
          bucket: 'bucket-a',
          accessKeyId: 'AKIA_A',
          secretAccessKey: 'SECRET_A',
        },
      })

    createResponse.assertStatus(201)
    const storageId = createResponse.body()?.data?.id

    const updateResponse = await client
      .put(`/api/storages/${storageId}`)
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({
        provider: 'cloudflare_r2',
        config: {
          endpoint: 'https://2222222222222222.r2.cloudflarestorage.com',
          bucket: 'bucket-b',
          accessKeyId: 'AKIA_B',
          secretAccessKey: 'SECRET_B',
        },
      })

    updateResponse.assertStatus(200)

    const body = updateResponse.body()
    if (body?.data?.config?.region !== 'auto') {
      throw new Error('Era esperado region=auto apos update cloudflare_r2 sem region explicita')
    }
  })
})
