import { Readable, PassThrough } from 'node:stream'
import logger from '@adonisjs/core/services/logger'
import transmit from '@adonisjs/transmit/services/main'
import StorageDestination from '#models/storage_destination'
import { BucketExplorerService } from './bucket_explorer_service.js'
import { StorageDestinationService } from '#services/storage_destination_service'
import type { ArchiveJob, BucketObject } from './types.js'

const ARCHIVE_CHANNEL_PREFIX = 'notifications/storage-archive'
const ARCHIVE_TTL_MS = 15 * 60 * 1000 // 15 min
const JOB_CLEANUP_TTL_MS = 60 * 60 * 1000 // 1h

/**
 * Serviço de geração de archives (tar.gz) de storages.
 *
 * Usa `archiver` para criar streams tar.gz sem buffer total em memória.
 * Objetos são listados recursivamente via BucketExplorerService, baixados via SDK nativo
 * e piped para o archiver.
 */
export class BucketArchiveService {
  private static jobs = new Map<string, ArchiveJob>()
  private static streams = new Map<string, PassThrough>()

  private static generateJobId(): string {
    return `archive-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`
  }

  static getJob(jobId: string): ArchiveJob | null {
    return this.jobs.get(jobId) ?? null
  }

  static getDownloadStream(jobId: string): PassThrough | null {
    return this.streams.get(jobId) ?? null
  }

  static async startArchive(
    storage: StorageDestination,
    path: string | null = null
  ): Promise<ArchiveJob> {
    const jobId = this.generateJobId()

    const job: ArchiveJob = {
      id: jobId,
      storageId: storage.id,
      path,
      status: 'pending',
      totalFiles: null,
      processedFiles: 0,
      startedAt: new Date().toISOString(),
    }

    this.jobs.set(jobId, job)

    // Executa em background
    this.buildArchive(job, storage, path).catch((err) => {
      logger.error(`[BucketArchive] Job ${jobId} falhou: ${err.message}`)
    })

    return job
  }

  private static async buildArchive(
    job: ArchiveJob,
    storage: StorageDestination,
    path: string | null
  ): Promise<void> {
    try {
      job.status = 'building'
      this.emitProgress(job)

      // 1. Listar todos os arquivos recursivamente
      const allFiles = await this.listAllFiles(storage, path ?? '')
      job.totalFiles = allFiles.length
      this.emitProgress(job)

      if (allFiles.length === 0) {
        job.status = 'ready'
        job.completedAt = new Date().toISOString()
        this.emitProgress(job)
        // Cria stream vazio
        const emptyStream = new PassThrough()
        this.streams.set(job.id, emptyStream)
        emptyStream.end()
        this.scheduleExpiration(job.id)
        return
      }

      // 2. Criar archive stream
      const archiver = await import('archiver')
      const archive = archiver.default('tar', { gzip: true, gzipOptions: { level: 6 } })
      const passThrough = new PassThrough()

      archive.pipe(passThrough)
      this.streams.set(job.id, passThrough)

      archive.on('error', (err: Error) => {
        logger.error(`[BucketArchive] Archiver error: ${err.message}`)
        job.status = 'failed'
        job.error = err.message
        this.emitProgress(job)
      })

      // 3. Para cada arquivo, baixar e adicionar ao archive
      const config = storage.getDecryptedConfig()
      if (!config) throw new Error('Configuração do storage inválida')

      for (const file of allFiles) {
        if (job.status !== 'building') break

        try {
          const downloadResult = await StorageDestinationService.getDownloadStream(
            storage,
            file.key
          )

          archive.append(downloadResult.stream as Readable, {
            name: file.key,
          })

          job.processedFiles++
          this.emitProgress(job)
        } catch (err: any) {
          logger.warn(`[BucketArchive] Falha ao baixar ${file.key}: ${err.message}`)
          // Continua com os próximos arquivos
        }
      }

      await archive.finalize()

      job.status = 'ready'
      job.completedAt = new Date().toISOString()
      job.expiresAt = new Date(Date.now() + ARCHIVE_TTL_MS).toISOString()
      this.emitProgress(job)
      this.scheduleExpiration(job.id)
    } catch (err: any) {
      job.status = 'failed'
      job.error = err.message
      job.completedAt = new Date().toISOString()
      this.emitProgress(job)
      this.scheduleJobCleanup(job.id)
    }
  }

  private static async listAllFiles(
    storage: StorageDestination,
    path: string
  ): Promise<BucketObject[]> {
    const allFiles: BucketObject[] = []
    const stack = [path]

    while (stack.length > 0) {
      const currentPath = stack.pop()!
      let cursor: string | undefined

      do {
        const result = await BucketExplorerService.listObjects(storage, currentPath, {
          limit: 1000,
          cursor,
        })

        for (const obj of result.objects) {
          if (obj.isDirectory) {
            stack.push(obj.key)
          } else {
            allFiles.push(obj)
          }
        }

        cursor = result.nextCursor ?? undefined
      } while (cursor)
    }

    return allFiles
  }

  private static scheduleExpiration(jobId: string): void {
    setTimeout(() => {
      const job = this.jobs.get(jobId)
      if (job && job.status === 'ready') {
        job.status = 'expired'
        this.emitProgress(job)
      }
      this.streams.delete(jobId)
      this.scheduleJobCleanup(jobId)
    }, ARCHIVE_TTL_MS)
  }

  private static scheduleJobCleanup(jobId: string): void {
    setTimeout(() => {
      this.jobs.delete(jobId)
      this.streams.delete(jobId)
    }, JOB_CLEANUP_TTL_MS)
  }

  private static emitProgress(job: ArchiveJob): void {
    try {
      transmit.broadcast(`${ARCHIVE_CHANNEL_PREFIX}/${job.id}`, {
        jobId: job.id,
        storageId: job.storageId,
        path: job.path ?? '',
        status: job.status,
        totalFiles: job.totalFiles ?? 0,
        processedFiles: job.processedFiles,
        error: job.error ?? '',
        expiresAt: job.expiresAt ?? '',
        timestamp: new Date().toISOString(),
      })
    } catch (err) {
      logger.error(`[BucketArchive] Erro ao emitir progresso: ${err}`)
    }
  }
}
