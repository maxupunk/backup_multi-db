import { existsSync } from 'node:fs'
import { mkdir, rm, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { test } from '@japa/runner'
import StorageDestination from '#models/storage_destination'
import User from '#models/user'

test.group('Storage Explorer Delete', () => {
  test('delete local folder recursively', async ({ client }) => {
    const rootPath = join(process.cwd(), 'tmp', 'storage-explorer-delete', String(Date.now()))
    const targetPath = join(rootPath, 'projects', 'daily')

    await mkdir(targetPath, { recursive: true })
    await writeFile(join(targetPath, 'backup.sql'), 'backup-content')

    try {
      const user = await User.create({
        fullName: 'Storage Explorer User',
        email: `storage_explorer_${Date.now()}@example.com`,
        password: 'Password123!',
        isActive: true,
      })

      const storage = new StorageDestination()
      storage.name = 'Local Explorer Delete'
      storage.type = 'local'
      storage.provider = 'local'
      storage.status = 'active'
      storage.isDefault = false
      storage.setConfig({
        type: 'local',
        basePath: rootPath,
      })
      await storage.save()

      const token = await User.accessTokens.create(user)
      const response = await client
        .delete(`/api/storages/${storage.id}/object`)
        .header('Authorization', `Bearer ${token.value!.release()}`)
        .json({
          key: 'projects',
          isDirectory: true,
        })

      response.assertStatus(200)
      response.assertBodyContains({
        success: true,
        message: 'Pasta excluída com sucesso',
      })

      if (existsSync(targetPath)) {
        throw new Error('A pasta e seu conteúdo deveriam ter sido removidos recursivamente')
      }
    } finally {
      await rm(rootPath, { recursive: true, force: true })
    }
  })

  test('reject deleting storage root', async ({ client }) => {
    const rootPath = join(process.cwd(), 'tmp', 'storage-explorer-delete-root', String(Date.now()))

    await mkdir(join(rootPath, 'safe-folder'), { recursive: true })

    try {
      const user = await User.create({
        fullName: 'Storage Explorer Root User',
        email: `storage_explorer_root_${Date.now()}@example.com`,
        password: 'Password123!',
        isActive: true,
      })

      const storage = new StorageDestination()
      storage.name = 'Local Explorer Root Guard'
      storage.type = 'local'
      storage.provider = 'local'
      storage.status = 'active'
      storage.isDefault = false
      storage.setConfig({
        type: 'local',
        basePath: rootPath,
      })
      await storage.save()

      const token = await User.accessTokens.create(user)
      const response = await client
        .delete(`/api/storages/${storage.id}/object`)
        .header('Authorization', `Bearer ${token.value!.release()}`)
        .json({
          key: '/',
          isDirectory: true,
        })

      response.assertStatus(422)
      response.assertBodyContains({
        success: false,
      })

      if (!existsSync(rootPath)) {
        throw new Error('A raiz do armazenamento não deveria poder ser removida')
      }
    } finally {
      await rm(rootPath, { recursive: true, force: true })
    }
  })
})
