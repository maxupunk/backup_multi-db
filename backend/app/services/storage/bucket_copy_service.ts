import { spawn } from 'node:child_process'
import logger from '@adonisjs/core/services/logger'
import transmit from '@adonisjs/transmit/services/main'
import StorageDestination from '#models/storage_destination'
import type { StorageDestinationConfig, StorageProvider } from '#models/storage_destination'
import type { CopyJob, CopyJobResult, CopyOptions } from './types.js'

const COPY_CHANNEL_PREFIX = 'notifications/storage-copy'
const JOB_TTL_MS = 24 * 60 * 60 * 1000 // 24h

/**
 * Serviço de cópia entre storages usando rclone.
 *
 * rclone recebe credenciais via env vars no processo filho — nunca via arquivo de config em disco.
 * Jobs são mantidos em memória com TTL de 24h.
 */
export class BucketCopyService {
  private static jobs = new Map<string, CopyJob>()
  private static jobProcesses = new Map<string, ReturnType<typeof spawn>>()

  private static generateJobId(): string {
    return `copy-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`
  }

  private static scheduleCleanup(jobId: string): void {
    setTimeout(() => {
      this.jobs.delete(jobId)
      this.jobProcesses.delete(jobId)
    }, JOB_TTL_MS)
  }

  static getJob(jobId: string): CopyJob | null {
    return this.jobs.get(jobId) ?? null
  }

  static async startCopy(
    source: StorageDestination,
    destination: StorageDestination,
    options: CopyOptions = {}
  ): Promise<CopyJob> {
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
    this.scheduleCleanup(jobId)

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

      const args = this.buildRcloneArgs(
        sourceConfig,
        destConfig,
        source.getEffectiveProvider(),
        destination.getEffectiveProvider(),
        options
      )

      const env = {
        ...this.buildRcloneEnv(sourceConfig, 'SRC', source.getEffectiveProvider()),
        ...this.buildRcloneEnv(destConfig, 'DST', destination.getEffectiveProvider()),
      }

      const result = await this.spawnRclone(job, args, env)

      job.status = 'completed'
      job.filesTransferred = result.filesTransferred
      job.bytesTransferred = result.bytesTransferred
      job.completedAt = new Date().toISOString()
      this.emitProgress(job)

      return result
    } catch (err: any) {
      job.status = 'failed'
      job.error = err.message
      job.completedAt = new Date().toISOString()
      this.emitProgress(job)

      return {
        filesTransferred: job.filesTransferred,
        bytesTransferred: job.bytesTransferred,
        errors: [err.message],
        duration: (Date.now() - startTime) / 1000,
      }
    }
  }

  private static buildRcloneRemote(
    provider: StorageProvider,
    config: StorageDestinationConfig,
    prefix: string
  ): string {
    const tag = prefix.toLowerCase()

    switch (provider) {
      case 'aws_s3':
      case 'minio':
      case 'cloudflare_r2': {
        const s3Config = config as Extract<StorageDestinationConfig, { type: 's3' }>
        const bucket = s3Config.bucket
        const path = s3Config.prefix?.replace(/^\/+|\/+$/g, '') ?? ''
        return `:s3,env_auth=false,access_key_id={${tag}_access_key},secret_access_key={${tag}_secret_key},region=${s3Config.region}${s3Config.endpoint ? `,endpoint=${s3Config.endpoint}` : ''}${s3Config.forcePathStyle ? ',force_path_style=true' : ''}:${bucket}/${path}`
      }
      case 'google_gcs': {
        const gcsConfig = config as Extract<StorageDestinationConfig, { type: 'gcs' }>
        const bucket = gcsConfig.bucket
        const path = gcsConfig.prefix?.replace(/^\/+|\/+$/g, '') ?? ''
        return `:gcs,env_auth=false${gcsConfig.projectId ? `,project_number=${gcsConfig.projectId}` : ''}:${bucket}/${path}`
      }
      case 'azure_blob': {
        const azureConfig = config as Extract<StorageDestinationConfig, { type: 'azure_blob' }>
        const container = azureConfig.container
        const path = azureConfig.prefix?.replace(/^\/+|\/+$/g, '') ?? ''
        return `:azureblob,env_auth=false:${container}/${path}`
      }
      case 'sftp': {
        const sftpConfig = config as Extract<StorageDestinationConfig, { type: 'sftp' }>
        const basePath = sftpConfig.basePath?.replace(/^\/+|\/+$/g, '') ?? ''
        return `:sftp,host=${sftpConfig.host},port=${sftpConfig.port ?? 22},user=${sftpConfig.username}:${basePath}`
      }
      case 'local': {
        const localConfig = config as Extract<StorageDestinationConfig, { type: 'local' }>
        return localConfig.basePath ?? '/app_data/backups'
      }
      default:
        throw new Error(`Provider rclone não suportado: ${provider}`)
    }
  }

  private static buildRcloneArgs(
    sourceConfig: StorageDestinationConfig,
    destConfig: StorageDestinationConfig,
    sourceProvider: StorageProvider,
    destProvider: StorageProvider,
    options: CopyOptions
  ): string[] {
    const cmd = options.deleteExtraneous ? 'sync' : 'copy'

    const sourceRemote = this.buildRcloneRemote(sourceProvider, sourceConfig, 'SRC')
    const destRemote = this.buildRcloneRemote(destProvider, destConfig, 'DST')

    let sourcePath = sourceRemote
    let destPath = destRemote

    if (options.sourcePath) {
      const clean = options.sourcePath.replace(/^\/+|\/+$/g, '')
      if (clean) sourcePath = `${sourceRemote}${sourceRemote.endsWith('/') ? '' : '/'}${clean}`
    }

    if (options.destinationPath) {
      const clean = options.destinationPath.replace(/^\/+|\/+$/g, '')
      if (clean) destPath = `${destRemote}${destRemote.endsWith('/') ? '' : '/'}${clean}`
    }

    const args = [cmd, sourcePath, destPath, '--progress', '--stats', '2s', '--stats-one-line']

    if (options.dryRun) {
      args.push('--dry-run')
    }

    return args
  }

  private static buildRcloneEnv(
    config: StorageDestinationConfig,
    prefix: string,
    _provider: StorageProvider
  ): Record<string, string> {
    const env: Record<string, string> = {}

    if (config.type === 's3') {
      env[`RCLONE_${prefix}_ACCESS_KEY_ID`] = config.accessKeyId
      env[`RCLONE_${prefix}_SECRET_ACCESS_KEY`] = config.secretAccessKey
    }

    if (config.type === 'azure_blob') {
      // Extrair account e key da connection string
      const accountMatch = config.connectionString.match(/AccountName=([^;]+)/)
      const keyMatch = config.connectionString.match(/AccountKey=([^;]+)/)
      if (accountMatch) env[`RCLONE_${prefix}_ACCOUNT`] = accountMatch[1]
      if (keyMatch) env[`RCLONE_${prefix}_KEY`] = keyMatch[1]
    }

    if (config.type === 'sftp') {
      if (config.password) env[`RCLONE_${prefix}_PASS`] = config.password
    }

    if (config.type === 'gcs' && config.credentialsJson) {
      env[`RCLONE_${prefix}_SERVICE_ACCOUNT_CREDENTIALS`] = config.credentialsJson
    }

    return env
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

      proc.stdout?.on('data', (data: Buffer) => {
        const line = data.toString()
        const redacted = this.redactCredentials(line)

        // Parse rclone stats output para progresso
        const transferredMatch = redacted.match(/Transferred:\s+(\d+)\s+\/\s+(\d+)/)
        if (transferredMatch) {
          job.filesTransferred = Number.parseInt(transferredMatch[1], 10)
          job.totalFiles = Number.parseInt(transferredMatch[2], 10)
          this.emitProgress(job)
        }

        const bytesMatch = redacted.match(/Transferred:\s+([\d.]+)\s*(\w+)/)
        if (bytesMatch) {
          job.bytesTransferred = this.parseBytes(bytesMatch[1], bytesMatch[2])
          this.emitProgress(job)
        }

        logger.debug(`[BucketCopy:${job.id}] ${redacted.trim()}`)
      })

      proc.stderr?.on('data', (data: Buffer) => {
        stderrBuffer += data.toString()
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
