import { test } from '@japa/runner'
import User from '#models/user'
import StorageDestination from '#models/storage_destination'

test.group('Storage Space', () => {
  // -------------------------------------------------------------------------
  // Auth guard
  // -------------------------------------------------------------------------

  test('block unauthenticated access to spaceAll', async ({ client }) => {
    const response = await client.get('/api/storage-destinations-space')

    response.assertStatus(401)
  })

  test('block unauthenticated access to space by id', async ({ client }) => {
    const response = await client.get('/api/storage-destinations/1/space')

    response.assertStatus(401)
  })

  // -------------------------------------------------------------------------
  // spaceAll — remote destinations must appear with spaceAvailable: false
  // -------------------------------------------------------------------------

  test('spaceAll returns remote S3 destination with spaceAvailable false', async ({ client }) => {
    const user = await User.create({
      fullName: 'Space User',
      email: `space_s3_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const destination = new StorageDestination()
    destination.name = 'S3 Space Test'
    destination.type = 's3'
    destination.status = 'active'
    destination.isDefault = false
    destination.setConfig({
      type: 's3',
      region: 'us-east-1',
      bucket: 'test-bucket',
      accessKeyId: 'AKIA_TEST',
      secretAccessKey: 'SECRET_TEST',
    })
    await destination.save()

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/storage-destinations-space')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)

    const body = response.body()
    if (!Array.isArray(body?.data)) {
      throw new Error('data deve ser um array')
    }

    const found = body.data.find((s: any) => s.destinationId === destination.id)
    if (!found) {
      throw new Error(`Destino ${destination.id} não encontrado no retorno de spaceAll`)
    }
    if (found.spaceAvailable !== false) {
      throw new Error('Destino S3 remoto deve ter spaceAvailable=false')
    }
    if (found.destinationName !== 'S3 Space Test') {
      throw new Error('destinationName deve ser o nome do destino cadastrado')
    }
    if (found.type !== 's3') {
      throw new Error('type deve ser s3')
    }
  })

  test('spaceAll returns cloudflare_r2 destination with spaceAvailable false', async ({
    client,
  }) => {
    const user = await User.create({
      fullName: 'Space User R2',
      email: `space_r2_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const destination = new StorageDestination()
    destination.name = 'R2 Space Test'
    destination.type = 's3'
    destination.status = 'active'
    destination.isDefault = false
    destination.setConfig({
      type: 's3',
      region: 'auto',
      bucket: 'r2-bucket',
      endpoint: 'https://1234567890abcdef.r2.cloudflarestorage.com',
      accessKeyId: 'AKIA_R2',
      secretAccessKey: 'SECRET_R2',
    })
    await destination.save()

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/storage-destinations-space')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)

    const body = response.body()
    const found = body.data?.find((s: any) => s.destinationId === destination.id)
    if (!found) {
      throw new Error(`Destino R2 ${destination.id} não encontrado em spaceAll`)
    }
    if (found.spaceAvailable !== false) {
      throw new Error('Destino R2 (tipo s3) deve ter spaceAvailable=false')
    }
  })

  test('spaceAll returns gcs destination with spaceAvailable false', async ({ client }) => {
    const user = await User.create({
      fullName: 'Space GCS User',
      email: `space_gcs_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const destination = new StorageDestination()
    destination.name = 'GCS Space Test'
    destination.type = 'gcs'
    destination.status = 'active'
    destination.isDefault = false
    destination.setConfig({
      type: 'gcs',
      bucket: 'gcs-bucket',
    })
    await destination.save()

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/storage-destinations-space')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)

    const body = response.body()
    const found = body.data?.find((s: any) => s.destinationId === destination.id)
    if (!found) {
      throw new Error(`Destino GCS ${destination.id} não encontrado em spaceAll`)
    }
    if (found.spaceAvailable !== false) {
      throw new Error('Destino GCS deve ter spaceAvailable=false')
    }
  })

  test('spaceAll returns azure_blob destination with spaceAvailable false', async ({ client }) => {
    const user = await User.create({
      fullName: 'Space Azure User',
      email: `space_azure_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const destination = new StorageDestination()
    destination.name = 'Azure Space Test'
    destination.type = 'azure_blob'
    destination.status = 'active'
    destination.isDefault = false
    destination.setConfig({
      type: 'azure_blob',
      connectionString: 'DefaultEndpointsProtocol=https;AccountName=test',
      container: 'backups',
    })
    await destination.save()

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/storage-destinations-space')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)

    const body = response.body()
    const found = body.data?.find((s: any) => s.destinationId === destination.id)
    if (!found) {
      throw new Error(`Destino Azure ${destination.id} não encontrado em spaceAll`)
    }
    if (found.spaceAvailable !== false) {
      throw new Error('Destino Azure Blob deve ter spaceAvailable=false')
    }
  })

  test('spaceAll excludes inactive destinations', async ({ client }) => {
    const user = await User.create({
      fullName: 'Space Inactive User',
      email: `space_inactive_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const destination = new StorageDestination()
    destination.name = 'Inactive S3'
    destination.type = 's3'
    destination.status = 'inactive'
    destination.isDefault = false
    destination.setConfig({
      type: 's3',
      region: 'us-east-1',
      bucket: 'inactive-bucket',
      accessKeyId: 'AKIA_INACTIVE',
      secretAccessKey: 'SECRET_INACTIVE',
    })
    await destination.save()

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/storage-destinations-space')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)

    const body = response.body()
    const found = body.data?.find((s: any) => s.destinationId === destination.id)
    if (found) {
      throw new Error('Destino inativo não deve aparecer em spaceAll')
    }
  })

  // -------------------------------------------------------------------------
  // space by id — remote destination returns null (API contract unchanged)
  // -------------------------------------------------------------------------

  test('space by id returns null data for remote S3 destination', async ({ client }) => {
    const user = await User.create({
      fullName: 'Space By Id User',
      email: `space_byid_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const destination = new StorageDestination()
    destination.name = 'S3 By Id Test'
    destination.type = 's3'
    destination.status = 'active'
    destination.isDefault = false
    destination.setConfig({
      type: 's3',
      region: 'us-east-1',
      bucket: 'byid-bucket',
      accessKeyId: 'AKIA_BYID',
      secretAccessKey: 'SECRET_BYID',
    })
    await destination.save()

    const token = await User.accessTokens.create(user)
    const response = await client
      .get(`/api/storage-destinations/${destination.id}/space`)
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)

    const body = response.body()
    if (body?.success !== true) {
      throw new Error('success deve ser true mesmo sem dados de espaço')
    }
    if (body?.data !== null) {
      throw new Error('data deve ser null para destinos remotos no endpoint individual')
    }
  })

  test('space by id returns 404 for unknown destination', async ({ client }) => {
    const user = await User.create({
      fullName: 'Space 404 User',
      email: `space_404_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/storage-destinations/999999/space')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(404)
  })

  // -------------------------------------------------------------------------
  // spaceAvailable shape validation
  // -------------------------------------------------------------------------

  test('spaceAll response items have required shape', async ({ client }) => {
    const user = await User.create({
      fullName: 'Space Shape User',
      email: `space_shape_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    const destination = new StorageDestination()
    destination.name = 'Shape Test S3'
    destination.type = 's3'
    destination.status = 'active'
    destination.isDefault = false
    destination.setConfig({
      type: 's3',
      region: 'eu-west-1',
      bucket: 'shape-bucket',
      accessKeyId: 'AKIA_SHAPE',
      secretAccessKey: 'SECRET_SHAPE',
    })
    await destination.save()

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/storage-destinations-space')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)

    const body = response.body()
    const item = body.data?.find((s: any) => s.destinationId === destination.id)
    if (!item) {
      throw new Error('Item não encontrado')
    }

    const requiredFields: string[] = [
      'destinationId',
      'destinationName',
      'type',
      'spaceAvailable',
      'totalBytes',
      'usedBytes',
      'freeBytes',
      'usedPercent',
      'freePercent',
      'isLowSpace',
      'lowSpaceThreshold',
    ]
    for (const field of requiredFields) {
      if (!(field in item)) {
        throw new Error(`Campo obrigatório ausente: ${field}`)
      }
    }
    if (typeof item.spaceAvailable !== 'boolean') {
      throw new Error('spaceAvailable deve ser boolean')
    }
    if (typeof item.isLowSpace !== 'boolean') {
      throw new Error('isLowSpace deve ser boolean')
    }
    if (typeof item.lowSpaceThreshold !== 'number') {
      throw new Error('lowSpaceThreshold deve ser number')
    }
  })
})
