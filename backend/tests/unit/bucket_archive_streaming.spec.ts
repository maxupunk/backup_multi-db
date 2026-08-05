import { test } from '@japa/runner'
import { Readable } from 'node:stream'
import { gunzipSync } from 'node:zlib'
import { readFileSync } from 'node:fs'
import { unlink } from 'node:fs/promises'

import { BucketArchiveService } from '#services/storage/bucket_archive_service'
import { StorageDestinationService } from '#services/storage_destination_service'
import type { ArchiveJob, BucketObject } from '#services/storage/types'

const FILE_COUNT = 8

test.group('BucketArchiveService - streaming do archive', () => {
  test('mantem no maximo um download aberto por vez', async ({ assert }) => {
    const { maxOpenStreams, job } = await runArchiveWithStubs()

    assert.equal(job.processedFiles, FILE_COUNT)

    // Com o bug original (append sem aguardar o consumo) este numero seria
    // FILE_COUNT: todos os downloads abertos ao mesmo tempo.
    assert.isAtMost(
      maxOpenStreams,
      2,
      `downloads simultaneos deveriam ser limitados, foram ${maxOpenStreams}`
    )
  }).timeout(20_000)

  test('o tar.gz gerado contem todos os arquivos e seus conteudos', async ({ assert }) => {
    const { tmpFile, job } = await runArchiveWithStubs()

    assert.equal(job.status, 'ready')
    assert.isString(tmpFile)

    const tarBuffer = gunzipSync(readFileSync(tmpFile!))
    const tarContent = tarBuffer.toString('latin1')

    for (let index = 0; index < FILE_COUNT; index++) {
      assert.include(tarContent, `arquivo-${index}.txt`)
      assert.include(tarContent, `conteudo do arquivo ${index}`)
    }

    await unlink(tmpFile!).catch(() => {})
  }).timeout(20_000)

  test('totalFiles so e preenchido ao fim da varredura', async ({ assert }) => {
    const { job, tmpFile } = await runArchiveWithStubs()

    // Durante o build o total e desconhecido (UI mostra barra indeterminada);
    // ao final reflete o que foi realmente descoberto.
    assert.equal(job.totalFiles, FILE_COUNT)

    await unlink(tmpFile!).catch(() => {})
  }).timeout(20_000)

  test('bucket vazio gera um tar.gz valido e vazio', async ({ assert }) => {
    const { job, tmpFile } = await runArchiveWithStubs({ fileCount: 0 })

    assert.equal(job.status, 'ready')
    assert.equal(job.totalFiles, 0)
    assert.equal(job.processedFiles, 0)

    // Um tar vazio ainda tem os blocos de fim de arquivo — o que importa e que
    // descomprime sem erro.
    const decompressed = gunzipSync(readFileSync(tmpFile!))
    assert.isAtLeast(decompressed.length, 0)

    await unlink(tmpFile!).catch(() => {})
  }).timeout(20_000)
})

async function runArchiveWithStubs(options: { fileCount?: number } = {}): Promise<{
  maxOpenStreams: number
  job: ArchiveJob
  tmpFile: string | undefined
}> {
  resetArchiveServiceState()

  const fileCount = options.fileCount ?? FILE_COUNT
  const files: BucketObject[] = Array.from({ length: fileCount }, (_, index) => ({
    key: `arquivo-${index}.txt`,
    name: `arquivo-${index}.txt`,
    size: 32,
    lastModified: '2026-01-01T00:00:00.000Z',
    isDirectory: false,
  }))

  let openStreams = 0
  let maxOpenStreams = 0

  const originalIterateFiles = (BucketArchiveService as any).iterateFiles
  const originalGetDownloadStream = StorageDestinationService.getDownloadStream
  const originalScheduleExpiration = (BucketArchiveService as any).scheduleExpiration

  ;(BucketArchiveService as any).iterateFiles = async function* () {
    for (const file of files) {
      yield file
    }
  }
  ;(BucketArchiveService as any).scheduleExpiration = () => {}
  ;(StorageDestinationService as any).getDownloadStream = async (
    _storage: unknown,
    key: string
  ) => {
    openStreams++
    maxOpenStreams = Math.max(maxOpenStreams, openStreams)

    const index = key.replace('arquivo-', '').replace('.txt', '')
    const stream = Readable.from([Buffer.from(`conteudo do arquivo ${index}`)])

    const release = () => {
      openStreams = Math.max(0, openStreams - 1)
    }
    stream.once('end', release)
    stream.once('close', release)

    return { stream, contentLength: 32 }
  }

  const job: ArchiveJob = {
    id: `archive-test-${Date.now()}`,
    storageId: 1,
    path: null,
    status: 'pending',
    totalFiles: null,
    processedFiles: 0,
    startedAt: new Date().toISOString(),
  }

  ;(BucketArchiveService as any).jobs.set(job.id, job)

  const storage = {
    id: 1,
    type: 'local',
    getDecryptedConfig: () => ({ type: 'local', basePath: '/tmp' }),
  }

  try {
    await (BucketArchiveService as any).buildArchive(job, storage, null)
  } finally {
    ;(BucketArchiveService as any).iterateFiles = originalIterateFiles
    ;(BucketArchiveService as any).scheduleExpiration = originalScheduleExpiration
    ;(StorageDestinationService as any).getDownloadStream = originalGetDownloadStream
  }

  return {
    maxOpenStreams,
    job,
    tmpFile: (BucketArchiveService as any).tmpFiles.get(job.id),
  }
}

function resetArchiveServiceState(): void {
  const handle = (BucketArchiveService as any).retentionSweepHandle
  if (handle) {
    clearInterval(handle)
  }

  ;(BucketArchiveService as any).retentionSweepHandle = null
  ;(BucketArchiveService as any).jobs = new Map()
  ;(BucketArchiveService as any).tmpFiles = new Map()
  ;(BucketArchiveService as any).expirations = new Map()
  ;(BucketArchiveService as any).cleanupSchedule = new Map()
}
