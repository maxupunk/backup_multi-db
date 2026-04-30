import { spawn } from 'node:child_process'
import logger from '@adonisjs/core/services/logger'
import transmit from '@adonisjs/transmit/services/main'
import type StorageDestination from '#models/storage_destination'
import type { StorageDestinationConfig, StorageProvider } from '#models/storage_destination'
import type { CopyJob, CopyJobResult, CopyOptions } from './types.js'

const COPY_CHANNEL_PREFIX = 'notifications/storage-copy'
const JOB_TTL_MS = 6 * 60 * 60 * 1000 // 6h
const RETENTION_SWEEP_INTERVAL_MS = 5 * 60 * 1000
const MAX_RETAINED_COPY_JOBS = 50

/**
 * Serviço de cópia entre storages usando rclone.
 *
 * rclone recebe credenciais via env vars no processo filho — nunca via arquivo de config em disco.
 * Jobs são mantidos em memória com TTL de 24h.
 */
export class BucketCopyService {
  private static jobs = new Map<string, CopyJob>()
  private static jobProcesses = new Map<string, ReturnType<typeof spawn>>()
  private static cleanupSchedule = new Map<string, number>()
  private static retentionSweepHandle: ReturnType<typeof setInterval> | null = null

  private static generateJobId(): string {
    return `copy-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`
  }

  private static scheduleCleanup(jobId: string): void {
    this.cleanupSchedule.set(jobId, Date.now() + JOB_TTL_MS)
    this.ensureRetentionSweep()
  }

  static getJob(jobId: string): CopyJob | null {
    return this.jobs.get(jobId) ?? null
  }

  static async startCopy(
    source: StorageDestination,
    destination: StorageDestination,
    options: CopyOptions = {}
  ): Promise<CopyJob> {
    await this.runRetentionSweep()

    const jobId = this.generateJobId()

    const job: CopyJob = {
      id: jobId,
      sourceStorageId: source.id,
      destinationStorageId: destination.id,
      status: 'pending',
      filesTransferred: 0,
      totalFiles: null,
      bytesTransferred: 0,
      startedAt: new Date().toISOString(),
    }

    this.jobs.set(jobId, job)

    // Executa a cópia de forma assíncrona
    this.executeCopy(job, source, destination, options).catch((err) => {
      logger.error(`[BucketCopy] Job ${jobId} falhou: ${err.message}`)
    })

    return job
  }

  static cancelJob(jobId: string): boolean {
    const job = this.jobs.get(jobId)
    if (!job || (job.status !== 'pending' && job.status !== 'running')) {
      return false
    }

    const proc = this.jobProcesses.get(jobId)
    if (proc && !proc.killed) {
      proc.kill('SIGTERM')
    }

    job.status = 'cancelled'
    job.completedAt = new Date().toISOString()
    this.scheduleCleanup(jobId)
    this.emitProgress(job)
    return true
  }

  private static async executeCopy(
    job: CopyJob,
    source: StorageDestination,
    destination: StorageDestination,
    options: CopyOptions
  ): Promise<CopyJobResult> {
    const startTime = Date.now()

    try {
      job.status = 'running'
      this.emitProgress(job)

      const sourceConfig = source.getDecryptedConfig()
      const destConfig = destination.getDecryptedConfig()

      if (!sourceConfig || !destConfig) {
        throw new Error('Configuração de storage inválida')
      }

      const sourceRemoteConfig = await this.buildRemoteConfig(
        'src',
        source.getEffectiveProvider(),
        sourceConfig,
        options.sourcePath
      )
      const destRemoteConfig = await this.buildRemoteConfig(
        'dst',
        destination.getEffectiveProvider(),
        destConfig,
        options.destinationPath
      )

      const args = this.buildRcloneArgs(sourceRemoteConfig.path, destRemoteConfig.path, options)

      const env = {
        ...sourceRemoteConfig.env,
        ...destRemoteConfig.env,
      }

      const result = await this.spawnRclone(job, args, env)

      job.status = 'completed'
      job.filesTransferred = result.filesTransferred
      job.bytesTransferred = result.bytesTransferred
      job.completedAt = new Date().toISOString()
      this.scheduleCleanup(job.id)
      this.emitProgress(job)

      return result
    } catch (err: any) {
      job.status = 'failed'
      job.error = err.message
      job.completedAt = new Date().toISOString()
      this.scheduleCleanup(job.id)
      this.emitProgress(job)

      return {
        filesTransferred: job.filesTransferred,
        bytesTransferred: job.bytesTransferred,
        errors: [err.message],
        duration: (Date.now() - startTime) / 1000,
      }
    }
  }

  /**
   * Builds rclone named remote config via environment variables and returns
   * the reference path. Credentials never appear in the command line.
   *
   * Format: RCLONE_CONFIG_{NAME}_{OPTION}=value → reference as `name:path`
   */
  private static async buildRemoteConfig(
    remoteName: string,
    provider: StorageProvider,
    config: StorageDestinationConfig,
    subPath?: string
  ): Promise<{ env: Record<string, string>; path: string }> {
    const PREFIX = `RCLONE_CONFIG_${remoteName.toUpperCase()}`
    const env: Record<string, string> = {}
    const joinPath = (...parts: (string | undefined)[]): string =>
      parts
        .map((p) => p?.replace(/^\/+|\/+$/g, '') ?? '')
        .filter(Boolean)
        .join('/')

    switch (provider) {
      case 'aws_s3':
      case 'minio':
      case 'cloudflare_r2': {
        const s3 = config as Extract<StorageDestinationConfig, { type: 's3' }>
        env[`${PREFIX}_TYPE`] = 's3'
        env[`${PREFIX}_ENV_AUTH`] = 'false'
        env[`${PREFIX}_ACCESS_KEY_ID`] = s3.accessKeyId
        env[`${PREFIX}_SECRET_ACCESS_KEY`] = s3.secretAccessKey
        env[`${PREFIX}_REGION`] = s3.region || 'us-east-1'
        if (s3.endpoint) env[`${PREFIX}_ENDPOINT`] = s3.endpoint
        if (s3.forcePathStyle) env[`${PREFIX}_FORCE_PATH_STYLE`] = 'true'

        return { env, path: `${remoteName}:${joinPath(s3.bucket, s3.prefix, subPath)}` }
      }

      case 'google_gcs': {
        const gcs = config as Extract<StorageDestinationConfig, { type: 'gcs' }>
        env[`${PREFIX}_TYPE`] = 'google cloud storage'
        env[`${PREFIX}_ENV_AUTH`] = 'false'
        if (gcs.projectId) env[`${PREFIX}_PROJECT_NUMBER`] = gcs.projectId
        if (gcs.credentialsJson) env[`${PREFIX}_SERVICE_ACCOUNT_CREDENTIALS`] = gcs.credentialsJson

        return { env, path: `${remoteName}:${joinPath(gcs.bucket, gcs.prefix, subPath)}` }
      }

      case 'azure_blob': {
        const azure = config as Extract<StorageDestinationConfig, { type: 'azure_blob' }>
        const accountMatch = azure.connectionString.match(/AccountName=([^;]+)/)
        const keyMatch = azure.connectionString.match(/AccountKey=([^;]+)/)
        env[`${PREFIX}_TYPE`] = 'azureblob'
        if (accountMatch) env[`${PREFIX}_ACCOUNT`] = accountMatch[1]
        if (keyMatch) env[`${PREFIX}_KEY`] = keyMatch[1]

        return { env, path: `${remoteName}:${joinPath(azure.container, azure.prefix, subPath)}` }
      }

      case 'sftp': {
        const sftp = config as Extract<StorageDestinationConfig, { type: 'sftp' }>
        env[`${PREFIX}_TYPE`] = 'sftp'
        env[`${PREFIX}_HOST`] = sftp.host
        env[`${PREFIX}_PORT`] = String(sftp.port ?? 22)
        env[`${PREFIX}_USER`] = sftp.username
        if (sftp.password) env[`${PREFIX}_PASS`] = await this.obscurePassword(sftp.password)
        if (sftp.privateKey) env[`${PREFIX}_KEY_PEM`] = sftp.privateKey

        return { env, path: `${remoteName}:${joinPath(sftp.basePath, subPath)}` }
      }

      case 'local': {
        const local = config as Extract<StorageDestinationConfig, { type: 'local' }>
        env[`${PREFIX}_TYPE`] = 'local'
        const basePath = local.basePath ?? '/storage/backups'

        return { env, path: `${remoteName}:${joinPath(basePath, subPath)}` }
      }

      default:
        throw new Error(`Provider rclone não suportado: ${provider}`)
    }
  }

  private static obscurePassword(plain: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const proc = spawn('rclone', ['obscure', plain])
      let output = ''
      proc.stdout?.on('data', (d: Buffer) => {
        output += d.toString()
      })
      proc.on('close', (code) => {
        if (code === 0) resolve(output.trim())
        else reject(new Error(`rclone obscure falhou com código ${code}`))
      })
      proc.on('error', reject)
    })
  }

  private static buildRcloneArgs(
    sourcePath: string,
    destPath: string,
    options: CopyOptions
  ): string[] {
    const cmd = options.deleteExtraneous ? 'sync' : 'copy'
    const args = [cmd, sourcePath, destPath, '--stats', '2s', '--log-level', 'INFO']
    if (options.dryRun) args.push('--dry-run')
    return args
  }

  private static redactCredentials(text: string): string {
    // Remove qualquer credencial que possa ter vazado nos logs do rclone
    return text
      .replace(/access_key_id=[^\s,}]+/gi, 'access_key_id=***')
      .replace(/secret_access_key=[^\s,}]+/gi, 'secret_access_key=***')
      .replace(/AccountKey=[^\s;]+/gi, 'AccountKey=***')
      .replace(/password=[^\s,}]+/gi, 'password=***')
  }

  private static spawnRclone(
    job: CopyJob,
    args: string[],
    env: Record<string, string>
  ): Promise<CopyJobResult> {
    return new Promise((resolve, reject) => {
      const startTime = Date.now()
      let stderrBuffer = ''

      const proc = spawn('rclone', args, {
        env: { ...process.env, ...env },
        stdio: ['ignore', 'pipe', 'pipe'],
      })

      this.jobProcesses.set(job.id, proc)

      const parseRcloneOutput = (data: Buffer): string => {
        const text = data.toString()
        const redacted = this.redactCredentials(text)

        // Parse file count: "Transferred:   5 / 10 Files, 50%"
        const transferredMatch = redacted.match(/Transferred:\s+(\d+)\s+\/\s+(\d+)/)
        if (transferredMatch) {
          job.filesTransferred = Number.parseInt(transferredMatch[1], 10)
          job.totalFiles = Number.parseInt(transferredMatch[2], 10)
          this.emitProgress(job)
        }

        // Parse bytes: "Transferred:   2.5 GiB / 5 GiB, 50%, ..."
        const bytesMatch = redacted.match(
          /Transferred:\s+([\d.]+)\s*(B|KiB|MiB|GiB|TiB|KB|MB|GB|TB)/
        )
        if (bytesMatch) {
          job.bytesTransferred = this.parseBytes(bytesMatch[1], bytesMatch[2])
          this.emitProgress(job)
        }

        for (const line of redacted.split('\n').filter(Boolean)) {
          logger.debug(`[BucketCopy:${job.id}] ${line.trim()}`)
        }
        return text
      }

      proc.stdout?.on('data', parseRcloneOutput)

      proc.stderr?.on('data', (data: Buffer) => {
        const text = parseRcloneOutput(data)
        stderrBuffer += text
      })

      proc.on('close', (code) => {
        this.jobProcesses.delete(job.id)
        const duration = (Date.now() - startTime) / 1000

        if (code === 0) {
          resolve({
            filesTransferred: job.filesTransferred,
            bytesTransferred: job.bytesTransferred,
            errors: [],
            duration,
          })
        } else {
          const redactedError = this.redactCredentials(stderrBuffer.trim())
          reject(new Error(`rclone finalizou com código ${code}: ${redactedError}`))
        }
      })

      proc.on('error', (err) => {
        this.jobProcesses.delete(job.id)
        reject(new Error(`Falha ao executar rclone: ${err.message}`))
      })
    })
  }

  private static parseBytes(value: string, unit: string): number {
    const num = Number.parseFloat(value)
    const multipliers: Record<string, number> = {
      B: 1,
      KB: 1024,
      KiB: 1024,
      MB: 1024 ** 2,
      MiB: 1024 ** 2,
      GB: 1024 ** 3,
      GiB: 1024 ** 3,
      TB: 1024 ** 4,
      TiB: 1024 ** 4,
    }
    return Math.round(num * (multipliers[unit] ?? 1))
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
    if (this.retentionSweepHandle === null || this.cleanupSchedule.size > 0) {
      return
    }

    clearInterval(this.retentionSweepHandle)
    this.retentionSweepHandle = null
  }

  private static async runRetentionSweep(nowMs = Date.now()): Promise<void> {
    for (const [jobId, cleanupAt] of this.cleanupSchedule.entries()) {
      if (cleanupAt > nowMs) {
        continue
      }

      this.cleanupSchedule.delete(jobId)
      this.removeJob(jobId)
    }

    this.pruneOverflowJobs()
    this.stopRetentionSweepIfIdle()
  }

  private static pruneOverflowJobs(): void {
    const overflow = this.jobs.size - MAX_RETAINED_COPY_JOBS

    if (overflow <= 0) {
      return
    }

    const removableJobs = [...this.jobs.values()]
      .filter(
        (job) => job.status === 'completed' || job.status === 'failed' || job.status === 'cancelled'
      )
      .sort((left, right) => {
        const leftTime = this.getRetentionTimestamp(left)
        const rightTime = this.getRetentionTimestamp(right)
        return leftTime - rightTime
      })
      .slice(0, overflow)

    for (const job of removableJobs) {
      this.removeJob(job.id)
    }
  }

  private static getRetentionTimestamp(job: CopyJob): number {
    return new Date(job.completedAt ?? job.startedAt).getTime()
  }

  private static removeJob(jobId: string): void {
    this.cleanupSchedule.delete(jobId)
    this.jobs.delete(jobId)
    this.jobProcesses.delete(jobId)
  }

  private static emitProgress(job: CopyJob): void {
    try {
      transmit.broadcast(`${COPY_CHANNEL_PREFIX}/${job.id}`, {
        jobId: job.id,
        status: job.status,
        sourceStorageId: job.sourceStorageId,
        destinationStorageId: job.destinationStorageId,
        filesTransferred: job.filesTransferred,
        totalFiles: job.totalFiles ?? 0,
        bytesTransferred: job.bytesTransferred,
        error: job.error ?? '',
        timestamp: new Date().toISOString(),
      })
    } catch (err) {
      logger.error(`[BucketCopy] Erro ao emitir progresso: ${err}`)
    }
  }
}
