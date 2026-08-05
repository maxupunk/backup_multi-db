import { type Readable } from 'node:stream'
import type { Archiver, EntryData } from 'archiver'
import { createWriteStream, createReadStream } from 'node:fs'
import { unlink } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import logger from '@adonisjs/core/services/logger'
import transmit from '@adonisjs/transmit/services/main'
import type StorageDestination from '#models/storage_destination'
import { BucketExplorerService } from './bucket_explorer_service.js'
import { StorageDestinationService } from '#services/storage_destination_service'
import type { ArchiveJob, BucketObject } from './types.js'

const ARCHIVE_CHANNEL_PREFIX = 'notifications/storage-archive'
const ARCHIVE_TTL_MS = 15 * 60 * 1000 // 15 min
const JOB_CLEANUP_TTL_MS = 60 * 60 * 1000 // 1h
const RETENTION_SWEEP_INTERVAL_MS = 60 * 1000
const MAX_RETAINED_ARCHIVE_JOBS = 50
/** Intervalo mínimo entre eventos SSE de progresso por arquivo processado. */
const PROGRESS_THROTTLE_MS = 500

/**
 * Serviço de geração de archives (tar.gz) de storages.
 *
 * Usa `archiver` para criar um arquivo .tar.gz temporário em disco.
 * Gravar em disco evita o deadlock de backpressure que ocorre ao fazer pipe
 * para um PassThrough sem consumidor ativo.
 * Objetos são listados recursivamente via BucketExplorerService, baixados via SDK nativo
 * e piped para o archiver.
 */
export class BucketArchiveService {
  private static jobs = new Map<string, ArchiveJob>()
  /** Mapeia jobId → caminho do arquivo temporário em disco */
  private static tmpFiles = new Map<string, string>()
  private static expirations = new Map<string, number>()
  private static cleanupSchedule = new Map<string, number>()
  private static retentionSweepHandle: ReturnType<typeof setInterval> | null = null

  private static generateJobId(): string {
    return `archive-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`
  }

  static getJob(jobId: string): ArchiveJob | null {
    return this.jobs.get(jobId) ?? null
  }

  /** Cria um ReadStream do arquivo temporário gerado. Cada chamada abre um novo stream. */
  static getDownloadStream(jobId: string): Readable | null {
    const filePath = this.tmpFiles.get(jobId)
    if (!filePath) return null
    return createReadStream(filePath)
  }

  static async startArchive(
    storage: StorageDestination,
    path: string | null = null
  ): Promise<ArchiveJob> {
    await this.runRetentionSweep()

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

      // 1. Criar arquivo temporário em disco para evitar backpressure.
      //
      // Um bucket vazio não é mais um caso especial: o laço simplesmente não
      // executa e o `finalize()` produz o mesmo tar.gz vazio de antes.
      const tmpFile = join(tmpdir(), `${job.id}.tar.gz`)
      const archiver = await import('archiver')
      const archive = archiver.default('tar', { gzip: true, gzipOptions: { level: 6 } })
      const writeStream = createWriteStream(tmpFile)
      this.tmpFiles.set(job.id, tmpFile)

      // Registra erro do archiver antes de fazer pipe
      archive.on('error', (err: Error) => {
        logger.error(`[BucketArchive] Archiver error: ${err.message}`)
        job.status = 'failed'
        job.error = err.message
        job.completedAt = new Date().toISOString()
        this.emitProgress(job)
      })

      archive.pipe(writeStream)

      // 2. Para cada arquivo descoberto, baixar e adicionar ao archive.
      //
      // A listagem é consumida página a página (`iterateFiles`), sem materializar
      // o bucket inteiro: num bucket de 100k objetos o array de metadados sozinho
      // já custava dezenas de MB retidos durante todo o job.
      //
      // Um arquivo por vez, aguardando o archiver consumir a entrada antes de
      // abrir o próximo download. `archive.append()` apenas enfileira e retorna,
      // então abrir todos os downloads em sequência deixaria N respostas HTTP
      // abertas simultaneamente — o pico de memória (e de sockets) cresceria com
      // o número de objetos do bucket. O archiver processa uma entrada por vez
      // de qualquer forma; serializar aqui não custa throughput de compressão.
      const config = storage.getDecryptedConfig()
      if (!config) throw new Error('Configuração do storage inválida')

      let lastProgressEmitAt = 0
      let discoveredFiles = 0

      for await (const file of this.iterateFiles(storage, path ?? '')) {
        if (job.status !== 'building') break

        discoveredFiles++

        try {
          const downloadResult = await StorageDestinationService.getDownloadStream(
            storage,
            file.key
          )

          await this.appendEntry(archive, downloadResult.stream as Readable, file.key)

          job.processedFiles++

          const now = Date.now()
          if (now - lastProgressEmitAt >= PROGRESS_THROTTLE_MS) {
            lastProgressEmitAt = now
            this.emitProgress(job)
          }
        } catch (err: any) {
          logger.warn(`[BucketArchive] Falha ao baixar ${file.key}: ${err.message}`)
          // Continua com os próximos arquivos
        }
      }

      // O total só é conhecido ao fim da varredura. Até aqui `totalFiles` fica
      // null e a UI mostra barra indeterminada com "N / ?" — honesto, porque o
      // número real de objetos não é sabido antes de percorrer todas as páginas.
      job.totalFiles = discoveredFiles
      this.emitProgress(job)

      // 4. Finalizar e aguardar o writeStream terminar de escrever em disco
      // Usar o evento 'finish' do writeStream (não archive.finalize()) evita
      // o deadlock de backpressure que ocorre com PassThrough sem consumidor.
      await new Promise<void>((resolve, reject) => {
        writeStream.on('finish', resolve)
        writeStream.on('error', reject)
        archive.on('error', reject)
        archive.finalize()
      })

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

  /**
   * Enfileira uma entrada no archive e só resolve quando ela termina de ser
   * escrita — é o que mantém apenas um download aberto por vez.
   *
   * Em caso de falha o stream de origem é destruído explicitamente: sem isso o
   * socket da resposta HTTP ficaria pendurado até o GC, segurando os buffers
   * que este método existe para evitar.
   */
  private static appendEntry(archive: Archiver, source: Readable, name: string): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      const cleanup = (): void => {
        archive.off('entry', onEntry)
        archive.off('error', onError)
        source.off('error', onSourceError)
      }

      // Só existe uma entrada em voo, então qualquer 'entry' é a nossa.
      const onEntry = (_entry: EntryData): void => {
        cleanup()
        resolve()
      }

      const onError = (err: Error): void => {
        cleanup()
        source.destroy()
        reject(err)
      }

      const onSourceError = (err: Error): void => {
        cleanup()
        source.destroy()
        reject(err)
      }

      archive.on('entry', onEntry)
      archive.on('error', onError)
      source.on('error', onSourceError)

      archive.append(source, { name })
    })
  }

  /**
   * Percorre o storage recursivamente entregando um arquivo por vez.
   *
   * Acumular a listagem inteira antes de comprimir fazia a memória crescer com
   * o tamanho do bucket. Aqui só a página corrente (até 1000 entradas) e a pilha
   * de diretórios pendentes ficam em memória — a pilha é limitada pela
   * profundidade/largura da árvore de pastas, não pelo número de arquivos.
   */
  private static async *iterateFiles(
    storage: StorageDestination,
    path: string
  ): AsyncGenerator<BucketObject> {
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
            yield obj
          }
        }

        cursor = result.nextCursor ?? undefined
      } while (cursor)
    }
  }

  private static scheduleExpiration(jobId: string): void {
    this.expirations.set(jobId, Date.now() + ARCHIVE_TTL_MS)
    this.ensureRetentionSweep()
  }

  private static scheduleJobCleanup(jobId: string): void {
    this.cleanupSchedule.set(jobId, Date.now() + JOB_CLEANUP_TTL_MS)
    this.ensureRetentionSweep()
  }

  private static async deleteTmpFile(jobId: string): Promise<void> {
    const filePath = this.tmpFiles.get(jobId)
    if (filePath) {
      this.tmpFiles.delete(jobId)
      await unlink(filePath).catch((err) => {
        logger.warn(
          `[BucketArchive] Falha ao remover arquivo temporário ${filePath}: ${err.message}`
        )
      })
    }
  }

  private static ensureRetentionSweep(): void {
    if (this.retentionSweepHandle !== null) {
      return
    }

    this.retentionSweepHandle = setInterval(() => {
      void this.runRetentionSweep()
    }, RETENTION_SWEEP_INTERVAL_MS)
    this.retentionSweepHandle.unref?.()
  }

  private static stopRetentionSweepIfIdle(): void {
    if (
      this.retentionSweepHandle === null ||
      this.expirations.size > 0 ||
      this.cleanupSchedule.size > 0
    ) {
      return
    }

    clearInterval(this.retentionSweepHandle)
    this.retentionSweepHandle = null
  }

  private static async runRetentionSweep(nowMs = Date.now()): Promise<void> {
    for (const [jobId, expiresAt] of this.expirations.entries()) {
      if (expiresAt > nowMs) {
        continue
      }

      this.expirations.delete(jobId)
      await this.expireJob(jobId)
    }

    for (const [jobId, cleanupAt] of this.cleanupSchedule.entries()) {
      if (cleanupAt > nowMs) {
        continue
      }

      this.cleanupSchedule.delete(jobId)
      await this.removeJob(jobId)
    }

    await this.pruneOverflowJobs()
    this.stopRetentionSweepIfIdle()
  }

  private static async expireJob(jobId: string): Promise<void> {
    const job = this.jobs.get(jobId)

    if (job?.status === 'ready') {
      job.status = 'expired'
      this.emitProgress(job)
    }

    await this.deleteTmpFile(jobId)
    this.scheduleJobCleanup(jobId)
  }

  private static async pruneOverflowJobs(): Promise<void> {
    const overflow = this.jobs.size - MAX_RETAINED_ARCHIVE_JOBS

    if (overflow <= 0) {
      return
    }

    const removableJobs = [...this.jobs.values()]
      .filter((job) => job.status === 'failed' || job.status === 'expired')
      .sort((left, right) => {
        const leftTime = this.getRetentionTimestamp(left)
        const rightTime = this.getRetentionTimestamp(right)
        return leftTime - rightTime
      })
      .slice(0, overflow)

    for (const job of removableJobs) {
      await this.removeJob(job.id)
    }
  }

  private static getRetentionTimestamp(job: ArchiveJob): number {
    return new Date(job.completedAt ?? job.startedAt).getTime()
  }

  private static async removeJob(jobId: string): Promise<void> {
    this.expirations.delete(jobId)
    this.cleanupSchedule.delete(jobId)
    await this.deleteTmpFile(jobId)
    this.jobs.delete(jobId)
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
