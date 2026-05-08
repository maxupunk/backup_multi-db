import { mkdir, rm, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

import { test } from '@japa/runner'
import { DateTime } from 'luxon'

import { getBackupStoragePath } from '#config/storage_paths'
import Backup from '#models/backup'
import StorageDestination from '#models/storage_destination'
import { BucketExplorerService } from '#services/storage/bucket_explorer_service'

test.group('Bucket explorer service', () => {
  test('annotates local files with remote bucket replicas from backup records', async ({
    assert,
  }) => {
    const rootPath = join(
      process.cwd(),
      'tmp',
      'bucket-explorer-local-replicas',
      String(Date.now())
    )
    const relativePath = '12/nightly.sql.gz'
    const filePath = join(rootPath, '12', 'nightly.sql.gz')

    await mkdir(join(rootPath, '12'), { recursive: true })
    await writeFile(filePath, 'backup-content')

    const localStorage = new StorageDestination()
    localStorage.name = 'Local Browse'
    localStorage.type = 'local'
    localStorage.provider = 'local'
    localStorage.status = 'active'
    localStorage.isDefault = false
    localStorage.setConfig({
      type: 'local',
      basePath: rootPath,
    })
    await localStorage.save()

    const remoteStorage = new StorageDestination()
    remoteStorage.name = 'R2 Archive'
    remoteStorage.type = 's3'
    remoteStorage.provider = 'cloudflare_r2'
    remoteStorage.status = 'active'
    remoteStorage.isDefault = false
    remoteStorage.setConfig({
      type: 's3',
      bucket: 'replica-bucket',
      endpoint: 'https://example.r2.cloudflarestorage.com',
      region: 'auto',
      accessKeyId: 'R2_KEY',
      secretAccessKey: 'R2_SECRET',
    })
    await remoteStorage.save()

    await Backup.create({
      connectionId: null,
      connectionDatabaseId: null,
      databaseName: 'nightly',
      storageDestinationId: remoteStorage.id,
      status: 'completed',
      filePath: relativePath,
      fileName: 'nightly.sql.gz',
      fileSize: 14,
      checksum: 'abc123',
      compressed: true,
      retentionType: 'hourly',
      protected: false,
      startedAt: DateTime.now(),
      finishedAt: DateTime.now(),
      durationSeconds: 1,
      errorMessage: null,
      exitCode: 0,
      metadata: null,
      trigger: 'manual',
    })

    try {
      const result = await BucketExplorerService.listObjects(localStorage, '12', {})
      const object = result.objects.find((entry) => entry.name === 'nightly.sql.gz')

      assert.exists(object)
      assert.deepEqual(object?.replicas, [
        {
          locationType: 'remote',
          storageId: remoteStorage.id,
          storageName: 'R2 Archive',
          provider: 'cloudflare_r2',
          path: relativePath,
        },
      ])
    } finally {
      await rm(rootPath, { recursive: true, force: true })
    }
  })

  test('annotates remote bucket files when a local copy still exists', async ({ assert }) => {
    const adapters = (BucketExplorerService as any).adapters as Map<string, unknown>
    const previousAdapter = adapters.get('cloudflare_r2')
    const relativePath = `24/remote-copy-${Date.now()}.sql.gz`
    const localFullPath = join(getBackupStoragePath(), relativePath)

    adapters.set('cloudflare_r2', {
      async listObjects() {
        return {
          objects: [
            {
              key: `archive/${relativePath}`,
              name: relativePath.split('/').at(-1)!,
              size: 10,
              lastModified: '2026-05-08T12:00:00.000Z',
              isDirectory: false,
            },
          ],
          nextCursor: null,
          isTruncated: false,
        }
      },
    })

    await mkdir(join(localFullPath, '..'), { recursive: true })
    await writeFile(localFullPath, 'backup-content')

    const remoteStorage = new StorageDestination()
    remoteStorage.name = 'Remote R2'
    remoteStorage.type = 's3'
    remoteStorage.provider = 'cloudflare_r2'
    remoteStorage.status = 'active'
    remoteStorage.isDefault = false
    remoteStorage.setConfig({
      type: 's3',
      bucket: 'replica-bucket',
      endpoint: 'https://example.r2.cloudflarestorage.com',
      region: 'auto',
      accessKeyId: 'R2_KEY',
      secretAccessKey: 'R2_SECRET',
      prefix: 'archive',
    })
    await remoteStorage.save()

    try {
      const result = await BucketExplorerService.listObjects(remoteStorage, '', {})
      const object = result.objects[0]
      const localReplica = object?.replicas?.find((replica) => replica.locationType === 'local')

      assert.exists(object)
      assert.isArray(object?.replicas)
      assert.exists(localReplica)
      assert.equal(localReplica?.provider, 'local')
      assert.equal(localReplica?.path, relativePath)
    } finally {
      if (previousAdapter) {
        adapters.set('cloudflare_r2', previousAdapter)
      } else {
        adapters.delete('cloudflare_r2')
      }

      await rm(join(getBackupStoragePath(), '24'), { recursive: true, force: true })
    }
  })
})
