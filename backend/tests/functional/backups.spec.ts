import { test } from '@japa/runner'
import User from '#models/user'
import Connection from '#models/connection'
import Backup from '#models/backup'
import { DateTime } from 'luxon'

test.group('Backups', (group) => {
  let user: User
  let connection: Connection

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'Backups User',
      email: `backup_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    connection = await Connection.create({
      name: 'Backup Connection',
      type: 'postgresql',
      host: 'localhost',
      port: 5432,
      database: 'backup_db',
      username: 'user',
      passwordEncrypted: 'password',
      status: 'active',
    })
  })

  test('list all backups', async ({ client }) => {
    await Backup.create({
      connectionId: connection.id,
      status: 'completed',
      startedAt: DateTime.now(),
      finishedAt: DateTime.now(),
      filePath: 'dummy.sql',
      fileName: 'backup.sql',
      fileSize: 1024,
      trigger: 'manual',
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .get('/api/backups')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
    })
    response.assertBodyContains({
      data: {
        meta: { total: 1 }, // at least 1
      },
    })
  })

  test('list backups by connection', async ({ client }) => {
    const token = await User.accessTokens.create(user)
    const response = await client
      .get(`/api/connections/${connection.id}/backups`)
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
    })
  })

  test('show specific backup', async ({ client }) => {
    const backup = await Backup.create({
      connectionId: connection.id,
      status: 'completed',
      startedAt: DateTime.now(),
      finishedAt: DateTime.now(),
      filePath: 'dummy_show.sql',
      fileName: 'backup_show.sql',
      fileSize: 1024,
      trigger: 'manual',
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .get(`/api/backups/${backup.id}`)
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      data: {
        id: backup.id,
        fileName: 'backup_show.sql',
      },
    })
  })

  test('download backup (404 if file missing)', async ({ client }) => {
    const backup = await Backup.create({
      connectionId: connection.id,
      status: 'completed',
      startedAt: DateTime.now(),
      finishedAt: DateTime.now(),
      filePath: 'non_existent_file.sql',
      fileName: 'backup.sql',
      fileSize: 1024,
      trigger: 'manual',
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .get(`/api/backups/${backup.id}/download`)
      .header('Authorization', `Bearer ${token.value!.release()}`)

    // Should return 404 because file doesn't exist on disk
    response.assertStatus(404)
    response.assertBodyContains({
      success: false,
      message: 'Arquivo de backup não encontrado no servidor',
    })
  })

  test('delete a backup', async ({ client }) => {
    const backup = await Backup.create({
      connectionId: connection.id,
      status: 'completed',
      startedAt: DateTime.now(),
      finishedAt: DateTime.now(),
      filePath: 'delete_me.sql',
      fileName: 'delete_me.sql',
      fileSize: 1024,
      trigger: 'manual',
    })

    const token = await User.accessTokens.create(user)
    const response = await client
      .delete(`/api/backups/${backup.id}`)
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)

    const check = await Backup.find(backup.id)
    if (check) {
      throw new Error('Backup should be deleted')
    }
  })
})
