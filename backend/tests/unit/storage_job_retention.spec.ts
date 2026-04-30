import { test } from '@japa/runner'

import { BucketArchiveService } from '#services/storage/bucket_archive_service'
import { BucketCopyService } from '#services/storage/bucket_copy_service'

test.group('Storage job retention', () => {
  test('copy service removes expired retained jobs during sweep', async ({ assert }) => {
    resetCopyServiceState()

    const job = {
      id: 'copy-expired',
      sourceStorageId: 1,
      destinationStorageId: 2,
      status: 'completed' as const,
      filesTransferred: 3,
      totalFiles: 3,
      bytesTransferred: 1024,
      startedAt: '2026-01-01T00:00:00.000Z',
      completedAt: '2026-01-01T00:10:00.000Z',
    }

    ;(BucketCopyService as any).jobs.set(job.id, job)
    ;(BucketCopyService as any).cleanupSchedule.set(job.id, Date.UTC(2026, 0, 1, 6, 0, 0))

    await (BucketCopyService as any).runRetentionSweep(Date.UTC(2026, 0, 1, 6, 0, 1))

    assert.isNull(BucketCopyService.getJob(job.id))
  })

  test('copy service caps retained terminal jobs to fifty entries', async ({ assert }) => {
    resetCopyServiceState()

    for (let index = 0; index < 55; index++) {
      const iso = new Date(Date.UTC(2026, 0, 1, 0, index, 0)).toISOString()
      ;(BucketCopyService as any).jobs.set(`copy-${index}`, {
        id: `copy-${index}`,
        sourceStorageId: 1,
        destinationStorageId: 2,
        status: 'completed',
        filesTransferred: index,
        totalFiles: index,
        bytesTransferred: index,
        startedAt: iso,
        completedAt: iso,
      })
    }

    await (BucketCopyService as any).runRetentionSweep(Date.UTC(2026, 0, 1, 2, 0, 0))

    assert.lengthOf([...(BucketCopyService as any).jobs.values()], 50)
    assert.isNull(BucketCopyService.getJob('copy-0'))
    assert.isNotNull(BucketCopyService.getJob('copy-54'))
  })

  test('archive service expires ready jobs and schedules cleanup', async ({ assert }) => {
    resetArchiveServiceState()

    const deletedJobIds: string[] = []
    const originalDeleteTmpFile = (BucketArchiveService as any).deleteTmpFile
    ;(BucketArchiveService as any).deleteTmpFile = async (jobId: string) => {
      deletedJobIds.push(jobId)
    }

    try {
      const job = {
        id: 'archive-ready',
        storageId: 1,
        path: null,
        status: 'ready' as const,
        totalFiles: 10,
        processedFiles: 10,
        startedAt: '2026-01-01T00:00:00.000Z',
        completedAt: '2026-01-01T00:10:00.000Z',
        expiresAt: '2026-01-01T00:25:00.000Z',
      }

      ;(BucketArchiveService as any).jobs.set(job.id, job)
      ;(BucketArchiveService as any).expirations.set(job.id, Date.UTC(2026, 0, 1, 0, 25, 0))

      await (BucketArchiveService as any).runRetentionSweep(Date.UTC(2026, 0, 1, 0, 25, 1))

      assert.equal(BucketArchiveService.getJob(job.id)?.status, 'expired')
      assert.deepEqual(deletedJobIds, [job.id])
      assert.isTrue((BucketArchiveService as any).cleanupSchedule.has(job.id))
    } finally {
      ;(BucketArchiveService as any).deleteTmpFile = originalDeleteTmpFile
    }
  })
})

function resetCopyServiceState(): void {
  clearIntervalIfPresent((BucketCopyService as any).retentionSweepHandle)
  ;(BucketCopyService as any).retentionSweepHandle = null
  ;(BucketCopyService as any).jobs = new Map()
  ;(BucketCopyService as any).jobProcesses = new Map()
  ;(BucketCopyService as any).cleanupSchedule = new Map()
}

function resetArchiveServiceState(): void {
  clearIntervalIfPresent((BucketArchiveService as any).retentionSweepHandle)
  ;(BucketArchiveService as any).retentionSweepHandle = null
  ;(BucketArchiveService as any).jobs = new Map()
  ;(BucketArchiveService as any).tmpFiles = new Map()
  ;(BucketArchiveService as any).expirations = new Map()
  ;(BucketArchiveService as any).cleanupSchedule = new Map()
}

function clearIntervalIfPresent(handle: ReturnType<typeof setInterval> | null): void {
  if (handle) {
    clearInterval(handle)
  }
}
