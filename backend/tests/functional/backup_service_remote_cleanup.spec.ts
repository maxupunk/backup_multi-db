import { existsSync } from 'node:fs'
import { mkdir, rm, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

import { test } from '@japa/runner'

import { getBackupStoragePath } from '#config/storage_paths'
import StorageDestination from '#models/storage_destination'
import { BackupService, type BackupResult } from '#services/backup_service'
import { StorageDestinationService } from '#services/storage_destination_service'

test.group('Backup service remote cleanup', () => {
  test('removes local copy after successful remote upload when cleanup policy is enabled', async ({
    assert,
  }) => {
    const originalUploadBackupFile = StorageDestinationService.uploadBackupFile
    const service = new BackupService() as any
    const relativePath = join('remote-cleanup', `enabled-${Date.now()}.sql.gz`)
    const localFullPath = join(getBackupStoragePath(), relativePath)

    await mkdir(join(localFullPath, '..'), { recursive: true })
    await writeFile(localFullPath, 'backup-content')

    let uploadCalled = false
    StorageDestinationService.uploadBackupFile = async () => {
      uploadCalled = true
    }

    service.shouldDeleteLocalCopyAfterRemoteUpload = () => true

    const destination = createRemoteDestination('Cleanup Enabled')
    const result: BackupResult = {
      success: true,
      filePath: relativePath,
      localFullPath,
      fileName: 'enabled.sql.gz',
      fileSize: 14,
    }

    try {
      await service.uploadToRemoteDestination(destination, result)

      assert.isTrue(uploadCalled)
      assert.isFalse(existsSync(localFullPath))
      assert.isTrue(result.success)
      assert.isUndefined(result.storageWarning)
    } finally {
      StorageDestinationService.uploadBackupFile = originalUploadBackupFile
      await rm(join(getBackupStoragePath(), 'remote-cleanup'), { recursive: true, force: true })
    }
  })

  test('keeps local copy after successful remote upload when cleanup policy is disabled', async ({
    assert,
  }) => {
    const originalUploadBackupFile = StorageDestinationService.uploadBackupFile
    const service = new BackupService() as any
    const relativePath = join('remote-cleanup', `disabled-${Date.now()}.sql.gz`)
    const localFullPath = join(getBackupStoragePath(), relativePath)

    await mkdir(join(localFullPath, '..'), { recursive: true })
    await writeFile(localFullPath, 'backup-content')

    let uploadCalled = false
    StorageDestinationService.uploadBackupFile = async () => {
      uploadCalled = true
    }

    service.shouldDeleteLocalCopyAfterRemoteUpload = () => false

    const destination = createRemoteDestination('Cleanup Disabled')
    const result: BackupResult = {
      success: true,
      filePath: relativePath,
      localFullPath,
      fileName: 'disabled.sql.gz',
      fileSize: 14,
    }

    try {
      await service.uploadToRemoteDestination(destination, result)

      assert.isTrue(uploadCalled)
      assert.isTrue(existsSync(localFullPath))
      assert.isTrue(result.success)
    } finally {
      StorageDestinationService.uploadBackupFile = originalUploadBackupFile
      await rm(join(getBackupStoragePath(), 'remote-cleanup'), { recursive: true, force: true })
    }
  })
})

function createRemoteDestination(name: string): StorageDestination {
  const destination = new StorageDestination()
  destination.name = name
  destination.type = 's3'
  destination.provider = 'cloudflare_r2'
  destination.status = 'active'
  destination.isDefault = false
  destination.setConfig({
    type: 's3',
    bucket: 'replica-bucket',
    endpoint: 'https://example.r2.cloudflarestorage.com',
    region: 'auto',
    accessKeyId: 'R2_KEY',
    secretAccessKey: 'R2_SECRET',
  })

  return destination
}
